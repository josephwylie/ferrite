//! Launch: the Project and the threads the Cockpit opens on.
#[allow(unused_imports)]
use super::*;

#[test]
fn startup_does_not_make_the_app_directory_a_project() {
    let (core, _fake) = cockpit("startup-project", 0);

    assert_eq!(startup_project(&core), None);
    assert!(core.registry().projects().is_empty());
}

/// The draft opens where the operator left off: the Project of the
/// Thread they used last, not the newest one they happened to register.
#[test]
fn startup_starts_on_the_last_project_worked_in() {
    let (mut core, _fake) = cockpit("startup-last-project", 0);
    let first = core.register_project(&here()).unwrap();
    let older = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    // A second registered Project, newer than the Thread's — the old
    // "newest registered" answer would name this one.
    let second = core
        .register_project(&std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    assert_ne!(first, second);

    assert_eq!(core.project_id(older), Some(first));
    assert_eq!(startup_project(&core), Some(first));
}

#[gpui::test]
fn launch_provider_seeds_the_first_empty_store_draft(cx: &mut TestAppContext) {
    let fake = Fake::default();
    let store = Store::open(scratch("launch-provider-draft")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_provider(core, Provider::Codex, cx)
    });

    view.read_with(cx, |view, _| {
        assert_eq!(
            *view.panes[0].draft().unwrap().binding.provider(),
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            }
        );
    });
}

/// A launch that opens nothing — the newest Thread would not revive,
/// or the operator had parked everything — still lists every parked
/// Thread from the first frame. There is no later change to wait for.
#[gpui::test]
fn a_launch_that_opens_nothing_still_lists_the_parked_threads(cx: &mut TestAppContext) {
    let dir = scratch("launch-parked");
    let parked = {
        // An earlier run: one Thread opened, then the process gone.
        let store = Store::open(dir.clone()).unwrap();
        let mut earlier = Cockpit::new(store, Box::new(Fake::default()));
        earlier
            .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
            .unwrap();
        earlier.threads()[0]
    };
    let store = Store::open(dir).unwrap();
    let core = Cockpit::new(store, Box::new(Fake::default()));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));

    view.read_with(cx, |view, _| {
        assert!(
            view.panes[0].draft().is_some(),
            "nothing revived: the launch is a draft"
        );
        assert_eq!(
            view.facts.parked().to_vec(),
            vec![parked],
            "and the parked Thread has its row"
        );
    });
}
