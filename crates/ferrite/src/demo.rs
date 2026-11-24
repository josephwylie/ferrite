//! The demo (`--demo`, `--load`, `--panes N`, `--demo parity`; ADR 0049):
//! a window on an Engine in this process whose Sessions are scripted
//! (`ferrite_cockpit::demo`), never the operator's Server. That Engine is
//! ferrite-client's fixture, so only a build with the `demo` feature has
//! the demo: `cargo run -p ferrite --features demo -- --demo`.

use ferrite_client::cockpit::Cockpit;
use ferrite_cockpit::demo;
use ferrite_core::provider::Provider;
use ferrite_engine::spawner::{self, ProcessRss};
use ferrite_engine::store::Store;

pub(crate) struct Demo {
    /// `--demo parity`: the UI refresh mockups' world, on its own store.
    parity: bool,
    /// `--load`: Sessions that stream without end, the perf load.
    load: bool,
    /// `--panes N`: how many Panes it fills. The perf run uses 24.
    panes: usize,
}

impl Demo {
    /// The demo `args` ask for, if any.
    pub(crate) fn from_args(args: &[String]) -> Option<Self> {
        super::demo_flag(args)?;
        Some(Self {
            parity: super::flag(args, "--demo") == Some("parity"),
            load: args.iter().any(|arg| arg == "--load"),
            panes: super::flag(args, "--panes")
                .and_then(|n| n.parse().ok())
                .unwrap_or(1),
        })
    }

    /// Its window's Cockpit, on the store at `store`: it watches its
    /// Sessions' memory, as a Server watches its own.
    pub(crate) fn cockpit(&self, store: std::path::PathBuf) -> Cockpit {
        let mut core = if self.parity {
            demo::mockup_world()
        } else {
            let store = Store::open(store).unwrap_or_else(|e| {
                ferrite_core::log!("cannot open the Thread store: {e}");
                std::process::exit(1);
            });
            Cockpit::try_new(store, Box::new(demo::Spawn::new(self.load))).unwrap_or_else(|e| {
                ferrite_core::log!("cannot open the workspace registry: {e}");
                std::process::exit(1);
            })
        };
        core.watch_memory(Box::new(ProcessRss::default()), spawner::RSS_LIMIT);
        core
    }

    /// Fill its Panes when its store opened none of its Threads.
    pub(crate) fn seed(&self, core: &mut Cockpit, provider: Provider) {
        if core.threads().is_empty() {
            demo::seed_panes(core, self.panes, provider);
        }
    }
}
