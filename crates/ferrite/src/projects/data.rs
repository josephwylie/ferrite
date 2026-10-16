//! The one door from the window into `ferrite_projects` (0.2): where its
//! projects live, and the configure and doctor actions. Every act is the
//! crate's own function or the same CLI subcommand its popup ran
//! (`popup::run_hp`), off the UI thread.

use std::path::PathBuf;

use ferrite_projects::paths::{self, Ctx, Env};
use ferrite_projects::runner::RealRunner;
use ferrite_projects::setup;

/// Where Projects keeps its projects and the operator's settings for them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub root: PathBuf,
    pub config_dir: PathBuf,
}

impl Place {
    /// The operator's: `HERDR_PROJECTS_ROOT`, the config file's `root`, or
    /// the default, exactly as the CLI resolves it.
    pub(crate) fn resolve() -> Option<Place> {
        let env = Env::from_process().ok()?;
        let config_dir = env.config_dir();
        let root = paths::resolve_root(None, &env, &config_dir).ok()?;
        Some(Place { root, config_dir })
    }

    /// Run `f` with a context on this place, as the binary builds one.
    pub(crate) fn with_ctx<R>(&self, f: impl FnOnce(&Ctx) -> R) -> Result<R, String> {
        let env = Env::from_process().map_err(|error| format!("{error:#}"))?;
        let runner = RealRunner;
        let ctx = Ctx {
            env: &env,
            root: self.root.clone(),
            config_dir: self.config_dir.clone(),
            runner: &runner,
            detached_ticker: true,
        };
        Ok(f(&ctx))
    }

    /// Run one CLI subcommand of `ferrite-projects` on this place: whether
    /// it succeeded, and its last line.
    pub(crate) fn run(&self, args: &[&str]) -> (bool, String) {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        self.with_ctx(|ctx| ferrite_projects::popup::run_hp(ctx, &args, None))
            .unwrap_or_else(|error| (false, format!("error: {error}")))
    }
}

/// The doctor action's notification: its title and, by whether every
/// required check passed, its body.
pub(crate) const DOCTOR_TITLE: &str = "Projects: doctor";

pub(crate) fn doctor_body(healthy: bool) -> &'static str {
    if healthy {
        "All required checks passed. Details: ferrite-projects doctor"
    } else {
        "Some checks FAILED. Details: ferrite-projects doctor"
    }
}

/// The configure action's notification: what `setup::configure_ferrite`
/// sets (Ferrite draws the rows Herdr's config did).
pub(crate) const CONFIGURED_TITLE: &str = "Projects configured";
pub(crate) const CONFIGURED_BODY: &str = "Progress hooks and the autoproject skill are set.";
pub(crate) const CONFIGURE_FAILED_TITLE: &str = "Projects: configure failed";

/// The configure action: `setup::configure_ferrite` on this place, in this
/// process (the CLI's `configure` with no flags), as its toast: title,
/// words, whether it failed.
pub(crate) fn configure(place: &Place) -> (String, String, bool) {
    configured(place.with_ctx(configure_in).and_then(|done| done))
}

/// `configure` in `ctx`.
pub(super) fn configure_in(ctx: &Ctx) -> Result<Vec<String>, String> {
    setup::configure_ferrite(ctx).map_err(|error| format!("{error:#}"))
}

pub(super) fn configured(done: Result<Vec<String>, String>) -> (String, String, bool) {
    match done {
        Ok(_) => (CONFIGURED_TITLE.into(), CONFIGURED_BODY.into(), false),
        Err(error) => (CONFIGURE_FAILED_TITLE.into(), error, true),
    }
}
