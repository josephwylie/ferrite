//! Chromium on macOS: a framework inside Ferrite.app, loaded at run time.
//!
//! `scripts/install-app.sh` lays the bundle out as CEF expects:
//! `Contents/Frameworks/Chromium Embedded Framework.framework`, and the five
//! `ferrite Helper*.app`s CEF starts its processes from, each holding this
//! same binary. So a helper finds the framework at `../../..` from its
//! executable, and the app at `../Frameworks`.

use std::path::{Path, PathBuf};

use cef::args::Args;
use cef::library_loader::LibraryLoader;
use cef::sandbox::Sandbox;
use cef::MainArgs;

use super::nsapp;

/// Helpers run in Chromium's (seatbelt) sandbox.
pub(super) const SANDBOXED: bool = true;

/// The loaded framework, for as long as CEF is in use.
pub(super) struct Library {
    _loader: LibraryLoader,
}

impl Library {
    /// Load the framework in the app, and make `NSApp` acceptable to CEF.
    /// Call before GPUI's first `+sharedApplication` (see `nsapp`).
    pub(super) fn load(exe: &Path) -> Result<Self, String> {
        let loader = loader(exe, false)?;
        nsapp::adopt_cef_app_protocol()?;
        Ok(Self { _loader: loader })
    }
}

/// A helper process, readied to run: sandboxed, framework loaded.
pub(super) struct Helper {
    // Fields drop in this order: the framework, then the sandbox.
    _loader: LibraryLoader,
    _sandbox: Option<Sandbox>,
    args: Args,
}

impl Helper {
    pub(super) fn enter(exe: &Path) -> Result<Self, String> {
        let args = Args::new();
        // cef-dll-sys is built with the sandbox on: a helper must enter it
        // before loading the framework, or the GPU/network children die with
        // exit code 5. Skipped when the browser was started with --no-sandbox.
        let sandbox = (!std::env::args().any(|a| a == "--no-sandbox")).then(|| {
            let mut sandbox = Sandbox::new();
            sandbox.initialize(args.as_main_args());
            sandbox
        });
        let loader = loader(exe, true)?;
        Ok(Self {
            _loader: loader,
            _sandbox: sandbox,
            args,
        })
    }

    pub(super) fn args(&self) -> &MainArgs {
        self.args.as_main_args()
    }
}

fn loader(exe: &Path, helper: bool) -> Result<LibraryLoader, String> {
    let exe_dir = exe.parent().ok_or("executable has no parent directory")?;
    // CEF's loader aborts the process if the framework is missing, so check
    // first: `cargo run` outside the bundle should get an error, not a crash.
    let frameworks = if helper {
        exe_dir.join("../../..")
    } else {
        exe_dir.join("../Frameworks")
    };
    let framework = frameworks.join("Chromium Embedded Framework.framework");
    if !framework.join("Chromium Embedded Framework").exists() {
        return Err(format!(
            "Chromium Embedded Framework not found at {} (visuals need the app bundle \
             scripts/install-app.sh builds)",
            framework.display()
        ));
    }
    let loader = LibraryLoader::new(exe, helper);
    if !loader.load() {
        return Err("failed to load the CEF framework".into());
    }
    Ok(loader)
}

pub(super) fn process_alive(pid: u32) -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { kill(pid as i32, 0) == 0 }
}

/// `dir` with symlinks resolved (`$TMPDIR` lives under `/var` →
/// `/private/var`).
pub(super) fn real_path(dir: PathBuf) -> PathBuf {
    dir.canonicalize().unwrap_or(dir)
}
