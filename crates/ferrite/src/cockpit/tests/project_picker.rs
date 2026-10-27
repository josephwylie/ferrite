//! The Project chip and the Project card.
#[allow(unused_imports)]
use super::*;

/// The draft's Project chip offers what the nav's dropdown offers: the
/// registered Projects to pick from, and one row that opens the card.
#[gpui::test]
fn the_project_chip_picks_a_project_or_adds_one(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("chip-matches-filter", 1);
    let repo = repo_in(&scratch("chip-matches-filter-repo"));
    let other = core.register_project(&repo).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let added = scratch("chip-matches-filter-added");
    std::fs::create_dir_all(&added).unwrap();

    view.update(cx, |view, cx| {
        view.open_draft(DraftTarget::Main, cx);
        let draft = view.panes[view.focused()].draft().unwrap();
        let rows = view.band_rows(draft, pane::BandChip::Project, cx);

        let labels: Vec<_> = rows.iter().map(|row| row.row.name.to_string()).collect();
        assert_eq!(
            rows.iter()
                .filter(|row| matches!(row.consequence, Consequence::Band(BandChoice::Project(_))))
                .count(),
            2,
            "both registered Projects are offered: {labels:?}"
        );
        assert_eq!(labels.last().unwrap(), "Add project…");
        assert!(matches!(
            rows.last().unwrap().consequence,
            Consequence::Band(BandChoice::AddProject)
        ));
        assert!(
            rows.iter().any(|row| row.active
                && row.consequence
                    == Consequence::Band(BandChoice::Project(
                        view.launch_project.expect("a launch Project"),
                    ))),
            "the draft's own Project is the ticked row"
        );

        // The row opens the same card the nav's dropdown opens, and
        // what it creates is what the draft ends up on.
        view.open_project_creator(cx);
        assert!(view.popover.is_none(), "the card replaces the popover");
        view.adopt_editor_directory(added.clone(), cx);
        view.confirm_project_editor(cx);

        let created = view.nav_filter.expect("the new Project is the filter");
        assert_ne!(created, other);
        assert_eq!(
            view.panes[view.focused()]
                .draft()
                .unwrap()
                .binding
                .project(),
            created,
            "the chip follows the Project just added"
        );
    });
}

/// The create card (#29): `Add Project…` opens on nothing, each press of
/// Add Directory contributes exactly one directory, the first names the
/// Project until the operator types over it, and nothing is registered
/// until Create.
#[gpui::test]
fn the_project_card_stages_directories_and_registers_on_confirm(cx: &mut TestAppContext) {
    let (core, _) = cockpit("project-card-create", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let base = scratch("project-card-create-folders");
    let main = base.join("primary");
    let extra = base.join("extra");
    std::fs::create_dir_all(&main).unwrap();
    std::fs::create_dir_all(&extra).unwrap();

    view.update(cx, |view, cx| {
        let before = view.cockpit.registry().projects().len();
        view.open_project_creator(cx);
        assert!(
            !view.project_editor_ready(),
            "a Project with no directory cannot be created"
        );

        view.adopt_editor_directory(main.clone(), cx);
        let editor = view.project_editor.as_ref().unwrap();
        assert_eq!(editor.staged, [main.clone()]);
        assert_eq!(
            editor.name.read(cx).text(),
            "primary",
            "the main directory names the Project until one is typed"
        );

        view.adopt_editor_directory(extra.clone(), cx);
        assert_eq!(
            view.project_editor.as_ref().unwrap().staged,
            [main.clone(), extra.clone()]
        );
        assert_eq!(
            view.cockpit.registry().projects().len(),
            before,
            "nothing is registered before Create"
        );

        view.confirm_project_editor(cx);
        assert!(view.project_editor.is_none());
        assert_eq!(view.cockpit.registry().projects().len(), before + 1);

        let project = view
            .cockpit
            .registry()
            .project(view.nav_filter.expect("the new Project is the filter"))
            .unwrap();
        assert_eq!(project.root, main.canonicalize().unwrap());
        assert_eq!(project.additional_roots, [extra.canonicalize().unwrap()]);
        assert_eq!(project.title, "primary");
    });
}

/// The edit card writes through: a directory added there is on the
/// Project at once, and the name line renames it on Done.
#[gpui::test]
fn the_project_card_edits_an_existing_project(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("project-card-edit", 1);
    let repo = repo_in(&scratch("project-card-edit-repo"));
    let project = core.register_project(&repo).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let extra = scratch("project-card-edit-extra");
    std::fs::create_dir_all(&extra).unwrap();

    view.update(cx, |view, cx| {
        view.open_project_editor(project, cx);
        view.adopt_editor_directory(extra.clone(), cx);
        assert_eq!(
            view.cockpit
                .registry()
                .project(project)
                .unwrap()
                .additional_roots,
            [extra.canonicalize().unwrap()],
            "the edit card attaches without waiting for Done"
        );

        let name = view.project_editor.as_ref().unwrap().name.clone();
        name.update(cx, |name, cx| name.set("Renamed".into(), cx));
        view.confirm_project_editor(cx);

        assert!(view.project_editor.is_none());
        assert_eq!(
            view.cockpit.registry().project(project).unwrap().title,
            "Renamed"
        );
    });
}

/// The card is a card, not a title bar: whatever the head says, the
/// body under it has to be on screen.
#[gpui::test]
fn the_project_card_draws_its_body(cx: &mut TestAppContext) {
    let (core, _) = cockpit("project-card-body", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.open_project_creator(cx));
    cx.run_until_parked();

    let card = cx
        .debug_bounds("project-editor-card")
        .expect("the card is up");
    let add = cx
        .debug_bounds("project-add-directory")
        .expect("the add-directory control is on screen");
    assert!(
        add.size.height > px(0.) && card.contains(&add.origin),
        "the body is inside the card, not clipped under it:              card {card:?}, button {add:?}"
    );
}

/// With no Thread to go by, a draft opened while the nav names a
/// Project starts on that Project, so the Thread it becomes runs in
/// that Project's directories rather than the launch one's.
#[gpui::test]
fn a_draft_starts_on_the_filtered_project(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("draft-follows-filter", 0);
    let repo = repo_in(&scratch("draft-follows-filter-repo"));
    let chosen = core.register_project(&repo).unwrap();
    let extra = scratch("draft-follows-filter-extra");
    std::fs::create_dir_all(&extra).unwrap();
    core.add_project_directories(chosen, &[extra.clone()])
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));

    view.update(cx, |view, cx| {
        view.open_draft(DraftTarget::Main, cx);
        assert_eq!(
            view.panes[view.focused()]
                .draft()
                .unwrap()
                .binding
                .project(),
            view.launch_project.expect("a launch Project"),
            "with no filter the draft starts where Ferrite was launched"
        );
    });

    view.update(cx, |view, cx| {
        let standing = view.panes[view.focused()].identity.draft().unwrap();
        view.cockpit.discard_draft(standing).unwrap();
        view.sync_panes(cx);
        view.nav_filter = Some(chosen);
        view.open_draft(DraftTarget::Main, cx);

        let draft = view.panes[view.focused()].draft().unwrap();
        assert_eq!(draft.binding.project(), chosen);
        let project = view.cockpit.registry().project(chosen).unwrap();
        assert_eq!(
            project.directories().collect::<Vec<_>>(),
            [repo.canonicalize().unwrap(), extra.canonicalize().unwrap()],
            "and that Project brings all of its directories"
        );
    });
}

/// Choosing a Project in the nav while a draft is standing carries the
/// draft with it: the chip in the Composer names the Project the send
/// will run in, so it cannot be left naming the previous one.
#[gpui::test]
fn choosing_a_project_moves_the_standing_draft(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("filter-moves-draft", 1);
    let repo = repo_in(&scratch("filter-moves-draft-repo"));
    let chosen = core.register_project(&repo).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));

    view.update(cx, |view, cx| {
        view.open_draft(DraftTarget::Main, cx);
        let draft = view.panes[view.focused()].draft().unwrap();
        assert_eq!(
            draft.binding.project(),
            view.launch_project.expect("a launch Project")
        );

        view.choose_nav_filter(Some(chosen), cx);

        let draft = view.panes[view.focused()].draft().unwrap();
        assert_eq!(draft.binding.project(), chosen);
        assert_eq!(
            *draft.binding.target(),
            DraftTarget::Main,
            "the workspace under it resets with the repo it named"
        );

        // `All Projects` names no Project, so there is nothing to move
        // the draft onto: it stays where the operator put it.
        view.choose_nav_filter(None, cx);
        assert_eq!(
            view.panes[view.focused()]
                .draft()
                .unwrap()
                .binding
                .project(),
            chosen
        );
    });
}
