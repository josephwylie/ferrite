//! A Pane's cell in a window of its own (`ferrite_ui::testing`): what it
//! draws from the inputs it reads and what its presses ask, with no window
//! behind it. The test keeps the inputs, as the window's root does.

use ferrite_core::draft::{DraftBinding, DraftTarget};
use ferrite_core::project::ProjectId;
use ferrite_core::provider::{Provider, ProviderChoice};
use ferrite_core::roster::DraftId;
use ferrite_core::ProjectKey;
use ferrite_ui::testing::{click, mount, record, Heard};
use gpui::{point, px, AppContext as _, Modifiers, TestAppContext, VisualTestContext};

use super::*;

/// What the window keeps of a draft's cell: its Pane, and its machine
/// while that is lost.
struct Kept {
    pane: Entity<PaneView>,
    lost: Option<MachineId>,
}

impl Kept {
    fn inputs(&self, ask: &CellAsk) -> CellInputs {
        CellInputs {
            index: 0,
            identity: ask.pane,
            view: self.pane.clone(),
            focused: true,
            lost: self.lost,
            sheet_up: false,
            drop_target: false,
            composer_menu: None,
            doors: Doors {
                hero: Some(MenuDoor::new(false, |_, _, _| {})),
                ..Doors::default()
            },
            controls: None,
            kind: KindInputs::Draft(DraftInputs {
                project: "ferrite".into(),
                environment: None,
                machine: None,
                copy_live: None,
                mode: "default".into(),
            }),
        }
    }
}

/// An empty draft's cell, its machine `lost` or not, what it asks, and
/// what it reads, which the test holds as the window's root would.
fn draft_cell(
    lost: Option<MachineId>,
    cx: &mut TestAppContext,
) -> (Entity<Kept>, Heard<String>, &mut VisualTestContext) {
    let draft = DraftId::new(1);
    let binding = pane::DraftBinding {
        binding: DraftBinding::new(
            ProviderChoice {
                provider: Provider::Claude,
                model: None,
            },
            ProjectKey::fixture(1, ProjectId(1)),
            DraftTarget::Main,
        ),
        band_focus: None,
        error: None,
    };
    let pane = cx.new(|cx| PaneView::new_draft(draft, binding, cx));
    let kept = cx.new(|_| Kept { pane, lost });
    let model = cx.new(|_| Cockpit::empty());
    let inputs = Source::of(&kept, |kept: &Kept, ask: &CellAsk, _| Some(kept.inputs(ask)));
    let details = Source::of(&kept, |_: &Kept, _: &DetailsAsk, _| None);
    let (view, cx) = mount(cx, |_, _| CellView::new(PaneIdentity::Draft(draft), model, inputs, details));
    let heard = record(cx, &view, |event: &PaneEvent| format!("{:?}", event.ask));
    (kept, heard, cx)
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<Pixels> {
    cx.update(|window, _| window.debug_bounds(selector))
        .unwrap_or_else(|| panic!("nothing drawn as {selector}"))
}

/// An empty draft's body holds the start hero, above its Composer; the
/// hero's Project word asks for the Project menu, from the hero, and the
/// press goes no further.
#[gpui::test]
fn a_drafts_hero_word_asks_for_its_project_menu(cx: &mut TestAppContext) {
    let (_kept, heard, cx) = draft_cell(None, cx);
    let empty = bounds(cx, "draft-empty");
    let block = bounds(cx, "composer-block");
    assert!(empty.bottom() <= block.top(), "{empty:?} / {block:?}");
    let hero = bounds(cx, "start-hero");
    assert!(empty.contains(&hero.center()), "{hero:?} in {empty:?}");

    click(cx, "start-hero-project");
    assert_eq!(heard.take(), ["BandMenu { chip: Project, from_hero: true }"]);
}

/// A press anywhere else on the cell asks for the Pane's focus.
#[gpui::test]
fn a_press_on_the_cell_asks_for_the_focus(cx: &mut TestAppContext) {
    let (_kept, heard, cx) = draft_cell(None, cx);
    let empty = bounds(cx, "draft-empty");
    cx.simulate_click(empty.origin + point(px(8.), px(8.)), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(heard.take(), ["Pressed"]);
}

/// While its machine is lost the cell is dimmed under a cover that takes
/// every press: nothing under it asks anything (ADR 0026).
#[gpui::test]
fn a_lost_machines_cell_takes_no_press(cx: &mut TestAppContext) {
    let machine = ThreadKey::fixture(1, 1).machine();
    let (_kept, heard, cx) = draft_cell(Some(machine), cx);
    let cover = format!("machine-lost-{}", machine.get());
    assert!(ferrite_ui::testing::shows(cx, &cover));
    click(cx, "start-hero-project");
    assert_eq!(heard.take(), Vec::<String>::new());
}
