//! The program installs and updates itself (crates/flow/src/install.rs), and bin/run.sh finds or downloads the program
//! herdr runs: platform names, release URLs, versions, checksums, the update put in place, the launcher, and run.sh in
//! a scratch plugin folder with a release folder of its own (file://). No network, never the real ~/.herdr-projects.
use std::path::{Path, PathBuf};
use std::sync::MutexGuard;
use sw::install::*;

static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn set(k: &str, v: &str) { unsafe { std::env::set_var(k, v) } }
/// A scratch home of its own: HOME and HERDR_PROJECTS_HOME point into it.
fn scratch() -> (PathBuf, MutexGuard<'static, ()>) {
    let one = crate::common::one_at_a_time(); // the environment is the whole test program's
    let base = std::env::temp_dir().join(format!("sw-install-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("home")).unwrap();
    let base = std::fs::canonicalize(&base).unwrap();
    set("HOME", &base.join("home").display().to_string());
    set("HERDR_PROJECTS_HOME", &base.join("sw").display().to_string());
    for k in ["HERDR_PROJECTS_DATA_DIR", "HERDR_PROJECTS_CONFIG_DIR", "HERDR_PROJECTS_RELEASES"] { unsafe { std::env::remove_var(k) } }
    (base, one)
}
fn write(p: &Path, text: &str) { std::fs::create_dir_all(p.parent().unwrap()).unwrap(); std::fs::write(p, text).unwrap(); }
fn exec(p: &Path, text: &str) { write(p, text); use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap(); }
fn read(p: &Path) -> String { std::fs::read_to_string(p).unwrap_or_default() }
fn here() -> (&'static str, &'static str) { platform().expect("the tests run on a platform a release is made for") }

#[test]
fn platform_names_urls_and_versions() {
    assert_eq!(platform_of("Linux", "x86_64"), Some(("linux", "x86_64")));
    assert_eq!(platform_of("Darwin", "arm64"), Some(("macos", "aarch64")), "uname on a Mac");
    assert_eq!(platform_of("macos", "aarch64"), Some(("macos", "aarch64")), "Rust's own names");
    assert_eq!(platform_of("linux", "amd64"), Some(("linux", "x86_64")));
    assert_eq!(platform_of("FreeBSD", "x86_64"), None);
    assert_eq!(platform_of("Linux", "riscv64"), None);
    assert_eq!(asset_name(("macos", "aarch64")), "herdr-projects-macos-aarch64");
    let base = "https://github.com/eliasstravik/herdr-projects/releases";
    assert_eq!(asset_url(base, Some("0.3.1"), "herdr-projects-linux-x86_64"), format!("{base}/download/v0.3.1/herdr-projects-linux-x86_64"));
    assert_eq!(asset_url(base, Some("v0.3.1"), "a"), format!("{base}/download/v0.3.1/a"), "a version typed with its v");
    assert_eq!(asset_url(base, None, "a"), format!("{base}/latest/download/a"));
    // The repo is private: gh, logged in, fetches what curl is refused.
    let to = Path::new("/t/f");
    let gh = |u: &str| gh_args(u, to).map(|v| v.join(" "));
    assert_eq!(gh(&format!("{base}/download/v0.3.1/a.sha256")).as_deref(), Some("release download v0.3.1 -R eliasstravik/herdr-projects -p a.sha256 -O /t/f --clobber"));
    assert_eq!(gh(&format!("{base}/latest/download/a")).as_deref(), Some("release download -R eliasstravik/herdr-projects -p a -O /t/f --clobber"));
    assert_eq!(gh("file:///mirror/download/v0.3.1/a"), None, "a mirror is fetched as it is");
    for (a, b, yes) in [("0.3.0", "0.2.0", true), ("0.10.0", "0.9.1", true), ("0.3.0", "0.3.0", false), ("0.2.9", "0.3.0", false), ("1.0", "0.9.9", true), ("0.3.0.1", "0.3.0", true), ("v0.3.1", "0.3.0", true), ("0.3.0", "0.3", false)] {
        assert_eq!(newer(a, b), yes, "{a} newer than {b}");
    }
    let h = "a".repeat(64);
    assert_eq!(sum_in(&format!("{h}  herdr-projects-linux-x86_64\n")), Some(h.clone()));
    assert_eq!(sum_in(&format!("{}\n", h.to_uppercase())), Some(h.clone()));
    assert_eq!(sum_in("Not Found"), None);
    assert_eq!(version_said("herdr-projects 0.3.0 (/x/bin/herdr-projects-linux-x86_64)"), Some("0.3.0".into()));
    assert_eq!(version_said("something else 0.3.0"), None);
    assert!(built_from_source(Path::new("/src/herdr-projects/target/release/herdr-projects")));
    assert!(!built_from_source(Path::new("/p/herdr-projects/bin/herdr-projects-linux-x86_64")));
    assert_eq!(version_file(Path::new("/p/bin/x")), PathBuf::from("/p/bin/x.version"));
}

#[test]
fn releases_are_github_unless_named() {
    let (_b, _one) = scratch();
    assert_eq!(releases(), "https://github.com/eliasstravik/herdr-projects/releases");
    set("HERDR_PROJECTS_RELEASES", "file:///mirror/releases/");
    assert_eq!(releases(), "file:///mirror/releases");
}

/// A release of its own: a program that says it is `version`, and its checksum, handed out by a fetch that records
/// what it was asked for.
struct Release { file: PathBuf, sum: String, asked: std::cell::RefCell<Vec<String>> }
fn release(base: &Path, version: &str, bad_sum: bool) -> Release {
    let file = base.join("release").join(asset_name(here()));
    exec(&file, &format!("#!/bin/sh\necho \"herdr-projects {version} ($0)\"\n"));
    let sum = if bad_sum { "0".repeat(64) } else { sha256(&file).expect("a sha256 tool here") };
    Release { file, sum, asked: Default::default() }
}
impl Release {
    fn fetch(&self, url: &str, to: &Path) -> sw::err::Res<()> {
        self.asked.borrow_mut().push(url.to_string());
        if url.ends_with(".sha256") { std::fs::write(to, format!("{}  {}\n", self.sum, asset_name(here())))?; } else { std::fs::copy(&self.file, to)?; }
        Ok(())
    }
}
fn listing(dir: &Path) -> Vec<String> { let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(); v.sort(); v }

#[test]
fn update_puts_the_newest_release_in_place() {
    let (b, _one) = scratch();
    let exe = b.join("plugin/bin").join(asset_name(here()));
    exec(&exe, "#!/bin/sh\necho old\n");
    let r = release(&b, "9.9.9", false);
    let said = update(&exe, None, &|u, t| r.fetch(u, t)).unwrap();
    assert!(said.starts_with(&format!("Updated herdr-projects {} → 9.9.9 (", sw::store::files::VERSION)), "{said}");
    let name = asset_name(here());
    assert_eq!(*r.asked.borrow(), vec![format!("{}/latest/download/{name}", releases()), format!("{}/latest/download/{name}.sha256", releases())]);
    assert_eq!(read(&exe), read(&r.file), "the program is the release's");
    assert_eq!(read(&version_file(&exe)), "9.9.9\n", "run.sh reads its version without running it");
    assert_eq!(listing(&exe.parent().unwrap()), vec![name.clone(), format!("{name}.version")], "no download left beside it");
    // The launcher in the scratch data folder was written by the one writer of it.
    assert!(b.join("sw/data/bin/herdr-projects").is_file());
    // A version named: that one's file is asked for.
    let r2 = release(&b, "9.9.8", false);
    update(&exe, Some("9.9.8"), &|u, t| r2.fetch(u, t)).unwrap();
    assert_eq!(r2.asked.borrow()[0], format!("{}/download/v9.9.8/{name}", releases()));
    assert_eq!(read(&version_file(&exe)), "9.9.8\n", "named, an older one is put back");
}

#[test]
fn update_changes_nothing_when_anything_is_wrong() {
    let (b, _one) = scratch();
    let exe = b.join("plugin/bin").join(asset_name(here()));
    exec(&exe, "#!/bin/sh\necho old\n");
    let bad = release(&b, "9.9.9", true);
    let e = update(&exe, None, &|u, t| bad.fetch(u, t)).unwrap_err();
    assert!(e.msg.contains("does not match its checksum: nothing was changed"), "{}", e.msg);
    assert_eq!(read(&exe), "#!/bin/sh\necho old\n");
    assert_eq!(listing(&exe.parent().unwrap()), vec![asset_name(here())], "the download is removed again");
    // No network: said, nothing changed.
    let e = update(&exe, None, &|u, _| Err(sw::err::Error::new(format!("could not download {u}: no network, or no such release")))).unwrap_err();
    assert!(e.msg.starts_with("could not download "), "{}", e.msg);
    // The newest release is not newer: the program stays, nothing left behind.
    let same = release(&b, "0.0.1", false);
    assert_eq!(update(&exe, None, &|u, t| same.fetch(u, t)).unwrap(), format!("herdr-projects {} is the newest release.", sw::store::files::VERSION));
    assert_eq!(read(&exe), "#!/bin/sh\necho old\n");
    assert_eq!(listing(&exe.parent().unwrap()), vec![asset_name(here())]);
    // A program cargo built is updated from its source, never by a download.
    let built = b.join("src/target/release/herdr-projects");
    let e = update(&built, None, &|_, _| panic!("nothing is downloaded")).unwrap_err();
    assert!(e.msg.contains("built from source") && e.msg.contains("cargo build --release"), "{}", e.msg);
}

#[test]
fn install_writes_the_launcher_for_this_program() {
    let (b, _one) = scratch();
    let exe = sw::store::files::bin();
    let launcher = b.join("sw/data/bin/herdr-projects");
    // The PATH is the whole test program's: it keeps /bin (sh and cat are only there on macOS) and is given back.
    let was = std::env::var("PATH").unwrap_or_default();
    set("PATH", "/usr/bin:/bin");
    let said = install(&exe);
    let text = read(&launcher);
    assert!(text.starts_with("#!/bin/sh\n") && text.contains(&format!("exec '{}' \"$@\"", exe.display())), "{text}");
    assert!(text.contains(&format!("export HERDR_PROJECTS_DATA_DIR='{}'", b.join("sw/data").display())), "{text}");
    assert_eq!(sw::store::move_home::launcher_target(text.lines().last().unwrap()), Some(exe.display().to_string()), "the move reads it back");
    assert!(said.starts_with(&format!("{} runs {}", launcher.display(), exe.display())), "{said}");
    assert!(said.contains(&format!("export PATH=\"{}:$PATH\"", launcher.parent().unwrap().display())), "{said}");
    // Another herdr-projects comes first on the PATH (npm link's): said, with the way out.
    exec(&b.join("npm/bin/herdr-projects"), "#!/bin/sh\n");
    set("PATH", &format!("{}:{}:/usr/bin:/bin", b.join("npm/bin").display(), launcher.parent().unwrap().display()));
    let said = install(&exe);
    assert!(said.contains(&format!("{} comes first on your PATH", b.join("npm/bin/herdr-projects").display())), "{said}");
    // The launcher's folder is on the PATH first: nothing more to say.
    set("PATH", &format!("{}:/usr/bin:/bin", launcher.parent().unwrap().display()));
    let lines = install(&exe).lines().count();
    set("PATH", &was);
    assert_eq!(lines, 1);
}

// Ferrite: bin/run.sh, the herdr plugin's launcher that downloads the program, is not taken, nor are its tests: Ferrite
// ships the program in its app (ADR 0031).
