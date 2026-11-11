//! The model picker (T3's docked `ProviderModelPicker`): one popover over
//! the Composer holding the provider rail, the searchable model list, the
//! Reasoning column and a footer, `MODEL_PICKER_H` tall whatever it shows.
//! The model chip opens it on the list; `/effort` and the palette's
//! `effort` open it in the Reasoning column. A pick re-aims the Thread (or
//! the draft's band) and leaves it open, the keyboard then in the column:
//! esc or a press outside shuts it.
//!
//! The rows are the popover's own (`Popover::rows`, every Provider's
//! models, each carrying its pick); the picker keeps which of them the tab
//! or the search lists, the Reasoning column's rungs and the search line.

use super::*;
use ferrite_pane::composer::Role;
use ferrite_ui::theme::*;

gpui::actions!(
    model_picker,
    [Up, Down, Confirm, Dismiss, Left, Right, Jump1, Jump2, Jump3, Jump4, Jump5, Jump6, Jump7, Jump8, Jump9]
);

/// The soft key caps the first nine listed models wear (`⌘1` … `⌘9`).
const JUMP_LABELS: [&str; 9] = [
    "\u{2318}1", "\u{2318}2", "\u{2318}3", "\u{2318}4", "\u{2318}5", "\u{2318}6", "\u{2318}7",
    "\u{2318}8", "\u{2318}9",
];

/// What the list shows while nothing is searched: the starred models, or
/// one Provider's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tab {
    Favourites,
    Provider(Provider),
}

/// Where the arrows and ↵ act: the model list, or the Reasoning column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Column {
    List,
    Reasoning,
}

pub(super) struct ModelPicker {
    pub(super) tab: Tab,
    pub(super) column: Column,
    /// The `Search models...` line. It holds the keyboard while the picker
    /// is open; the picker's keys ride its `ModelPicker` context.
    pub(super) search: Entity<Composer>,
    /// The rows listed now, in order: indices into `Popover::rows`.
    pub(super) shown: Vec<usize>,
    /// The Reasoning column's rungs, each carrying its pick.
    pub(super) efforts: Vec<Row>,
    pub(super) effort_cursor: usize,
    /// The model the column serves, named as the chip names it.
    pub(super) model_label: SharedString,
    /// The Provider a pick hands the Thread over to, once its first prompt
    /// has fixed the other.
    handover: Option<Provider>,
    /// Providers that cannot serve, and why (their CLI is missing).
    pub(super) unavailable: Vec<(Provider, SharedString)>,
    /// The rail pill's slide between tabs.
    pill: std::cell::Cell<Option<ferrite_ui::motion::Tween>>,
    focused: std::cell::Cell<bool>,
    _edited: gpui::Subscription,
}

impl ModelPicker {
    pub(super) fn query(&self, cx: &gpui::App) -> String {
        self.search.read(cx).text().trim().to_string()
    }
}

/// What a model row picks.
fn row_choice(row: &Row) -> Option<&ProviderChoice> {
    match &row.consequence {
        Consequence::Provision(choice) | Consequence::Band(BandChoice::Provider(choice)) => {
            Some(choice)
        }
        _ => None,
    }
}

/// The value a favourite records a row's model by: the provider's own
/// default is `default`, as its catalogue spells it.
fn favourite_value(choice: &ProviderChoice) -> &str {
    choice.model.as_deref().unwrap_or("default")
}

fn is_favourite(settings: &ferrite_core::settings::Settings, row: &Row) -> bool {
    row_choice(row)
        .is_some_and(|choice| settings.is_favourite(choice.provider, favourite_value(choice)))
}

/// The rows a tab lists (T3's `sortProviderModelItems`): a Provider's tab
/// its models with its favourites first; Favourites the starred models in
/// Provider order.
fn tab_rows(rows: &[Row], tab: Tab, settings: &ferrite_core::settings::Settings) -> Vec<usize> {
    let of = |provider: Provider| {
        (0..rows.len()).filter(move |at| {
            row_choice(&rows[*at]).is_some_and(|choice| choice.provider == provider)
        })
    };
    match tab {
        Tab::Favourites => Provider::ALL
            .into_iter()
            .flat_map(of)
            .filter(|at| is_favourite(settings, &rows[*at]))
            .collect(),
        Tab::Provider(provider) => {
            let (mut starred, rest): (Vec<usize>, Vec<usize>) =
                of(provider).partition(|at| is_favourite(settings, &rows[*at]));
            starred.extend(rest);
            starred
        }
    }
}

/// The search across every Provider (T3's `rankModelPickerSearch`): each
/// word of the query must match the model's name or its Provider's, the
/// closest matches first, a favourite a little ahead, an exact name first
/// of all; ties keep the catalogue's order.
fn ranked(rows: &[Row], query: &str, settings: &ferrite_core::settings::Settings) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut scored: Vec<(i64, usize)> = rows
        .iter()
        .enumerate()
        .filter_map(|(at, row)| {
            let choice = row_choice(row)?;
            let name = row.name.to_lowercase();
            if words.join(" ") == name {
                return Some((i64::MAX, at));
            }
            let haystack = format!("{name} {}", ferrite_ui::provider::word(choice.provider));
            let mut score = 0;
            for word in &words {
                score += crate::fuzzy::matches(word, &haystack)?.0;
            }
            if is_favourite(settings, row) {
                score += 24;
            }
            Some((score, at))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, at)| at).collect()
}

/// The rail's buttons from the top: Favourites, then each Provider.
fn rail_tabs() -> impl Iterator<Item = Tab> {
    std::iter::once(Tab::Favourites).chain(Provider::ALL.into_iter().map(Tab::Provider))
}

/// Where the rail's pill stands beside `tab`'s button: buttons are square
/// across the rail's padded width, `MODEL_PICKER_RAIL_PAD` apart, a
/// hairline (with its gap) under Favourites; the pill centred on its
/// button.
fn pill_top(tab: Tab) -> f32 {
    let button = MODEL_PICKER_RAIL_W - 2. * MODEL_PICKER_RAIL_PAD;
    let step = button + MODEL_PICKER_RAIL_PAD;
    let top = match tab {
        Tab::Favourites => MODEL_PICKER_RAIL_PAD,
        Tab::Provider(provider) => {
            let at = Provider::ALL.iter().position(|p| *p == provider).unwrap_or(0);
            MODEL_PICKER_RAIL_PAD + step + 1. + MODEL_PICKER_RAIL_PAD + at as f32 * step
        }
    };
    top + button / 2. - MODEL_PICKER_PILL_H / 2.
}

/// The key a model chip records its bounds under, where the picker hangs.
pub(super) fn chip_trigger(pane: PaneIdentity) -> SharedString {
    format!("model-chip-{pane:?}").into()
}

/// The picker's rows, rungs and names for one Pane.
struct Content {
    rows: Vec<Row>,
    efforts: Vec<Row>,
    label: SharedString,
    current: Provider,
    handover: Option<Provider>,
    hold: Option<&'static str>,
}

impl CockpitView {
    /// The model chip's door: the picker on its model list.
    pub(super) fn open_provider_picker(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.open_model_picker(PaneIdentity::Thread(thread), Column::List, cx);
    }

    /// `/effort`'s and the palette's door: the same picker, the keyboard in
    /// its Reasoning column.
    pub(super) fn open_effort_picker(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.open_model_picker(PaneIdentity::Thread(thread), Column::Reasoning, cx);
    }

    pub(super) fn model_picker_open(&self, pane: PaneIdentity) -> bool {
        self.model_picker().is_some_and(|(open, _)| open.pane == pane)
    }

    /// A model chip as a control: a press opens its Pane's picker, or
    /// shuts it, and the chip records where it stands for the picker to
    /// hang from. The press stops there, so the root's dismissal never
    /// undoes it.
    pub(super) fn model_chip_control(
        &self,
        index: usize,
        chip: gpui::component::button::Button,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let identity = self.panes[index].identity;
        let chip = chip.group(chip_trigger(identity)).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let Some(index) = view.index_of(identity) else {
                    return;
                };
                view.focus_pane(index);
                if view.model_picker_open(identity) {
                    view.popover = None;
                    cx.notify();
                } else {
                    view.open_model_picker(identity, Column::List, cx);
                }
            }),
        );
        ferrite_ui::components::on_bounds(
            div().flex().min_w_0().child(chip),
            self.record_trigger(chip_trigger(identity)),
        )
        .into_any_element()
    }

    /// Open the picker for `pane` (a Thread's, or a draft's band): on
    /// Favourites when any model is starred, else on the Pane's Provider
    /// (T3's); the cursor on the model in force.
    pub(super) fn open_model_picker(
        &mut self,
        pane: PaneIdentity,
        column: Column,
        cx: &mut Context<Self>,
    ) {
        let Some(content) = self.model_picker_content(pane) else {
            return;
        };
        let search = cx.new(|cx| {
            let mut line = Composer::new(cx);
            line.set_role(Role::Field(Some("ModelPicker")), cx);
            line.set_visible_row_limit(1, cx);
            line
        });
        let edited = cx.subscribe(&search, |view: &mut Self, _, _: &Edited, cx| {
            view.model_picker_searched(cx);
        });
        let tab = if self.prefs.settings.favourite_models.is_empty() {
            Tab::Provider(content.current)
        } else {
            Tab::Favourites
        };
        let picker = ModelPicker {
            tab,
            column,
            search,
            shown: Vec::new(),
            efforts: Vec::new(),
            effort_cursor: 0,
            model_label: SharedString::default(),
            handover: None,
            unavailable: Vec::new(),
            pill: Default::default(),
            focused: Default::default(),
            _edited: edited,
        };
        self.popover = Some(Popover {
            scroll: ScrollHandle::new(),
            revealed: Default::default(),
            hold: None,
            pane,
            kind: Kind::Provider(Box::new(picker)),
            rows: Vec::new(),
            selected: 0,
        });
        self.fill_model_picker(content, None, cx);
    }

    /// Derive the open picker again from what stands now — after a pick,
    /// an announcement, a turn starting or ending — keeping its tab, its
    /// search, its column (unless `column` moves it) and its cursor's row.
    pub(super) fn rebuild_model_picker(&mut self, column: Option<Column>, cx: &mut Context<Self>) {
        let Some(pane) = self
            .popover
            .as_ref()
            .filter(|open| matches!(open.kind, Kind::Provider(_)))
            .map(|open| open.pane)
        else {
            return;
        };
        match self.model_picker_content(pane) {
            Some(content) => self.fill_model_picker(content, column, cx),
            None => {
                self.popover = None;
                cx.notify();
            }
        }
    }

    fn fill_model_picker(&mut self, content: Content, column: Option<Column>, cx: &mut Context<Self>) {
        let unavailable: Vec<(Provider, SharedString)> = Provider::ALL
            .into_iter()
            .filter_map(|provider| {
                matches!(
                    self.cli_updates.state(provider),
                    crate::cli_updates::State::Current(ferrite_core::providers::update::Status::Missing)
                )
                .then(|| {
                    (
                        provider,
                        format!("{} is not installed", crate::cli_updates::name(provider)).into(),
                    )
                })
            })
            .collect();
        let Some(open) = self.popover.as_mut() else {
            return;
        };
        let previous = open
            .rows
            .get(open.selected)
            .map(|row| row.consequence.clone());
        let Kind::Provider(picker) = &mut open.kind else {
            return;
        };
        let Content {
            mut rows,
            mut efforts,
            label,
            handover,
            hold,
            ..
        } = content;
        Self::hold_tuning_rows(&mut rows, hold);
        Self::hold_tuning_rows(&mut efforts, hold);
        let previous_rung = picker
            .efforts
            .get(picker.effort_cursor)
            .map(|row| row.consequence.clone());
        picker.effort_cursor = previous_rung
            .and_then(|rung| efforts.iter().position(|row| row.consequence == rung))
            .or_else(|| efforts.iter().position(|row| row.active))
            .unwrap_or(0);
        picker.efforts = efforts;
        picker.model_label = label;
        picker.handover = handover;
        picker.unavailable = unavailable;
        if let Some(column) = column {
            picker.column = column;
        }
        open.hold = hold;
        open.rows = rows;
        open.selected = previous
            .and_then(|consequence| open.rows.iter().position(|row| row.consequence == consequence))
            .or_else(|| open.rows.iter().position(|row| row.active))
            .unwrap_or(0);
        self.list_model_picker(cx);
    }

    /// Re-list the open picker's rows for its tab or its search, the
    /// cursor kept on its row while it is listed, else on the first.
    fn list_model_picker(&mut self, cx: &mut Context<Self>) {
        let settings = &self.prefs.settings;
        let Some(open) = self.popover.as_mut() else {
            return;
        };
        let Kind::Provider(picker) = &mut open.kind else {
            return;
        };
        let query = picker.query(cx);
        picker.shown = if query.is_empty() {
            tab_rows(&open.rows, picker.tab, settings)
        } else {
            ranked(&open.rows, &query, settings)
        };
        if !picker.shown.contains(&open.selected) {
            open.selected = picker
                .shown
                .iter()
                .copied()
                .find(|at| open.rows[*at].active)
                .or_else(|| picker.shown.first().copied())
                .unwrap_or(0);
        }
        open.revealed.set(false);
        cx.notify();
    }

    fn model_picker_content(&self, pane: PaneIdentity) -> Option<Content> {
        match pane {
            PaneIdentity::Thread(thread) => {
                let open = self.cockpit.thread(thread)?;
                let current = open.provider();
                let chosen = open.model().map(str::to_string);
                let serving = open.transcript().model().map(str::to_string);
                let locked = open.first_prompt_sent();
                let (rows, _) = self.provider_rows(
                    current,
                    chosen.as_deref(),
                    serving.as_deref(),
                    locked,
                    Consequence::Provision,
                );
                let label = ferrite_core::providers::models::footer_name(
                    current,
                    open.model().or(open.transcript().model()),
                    open.models(),
                );
                let efforts = self
                    .effort_ladder(thread)
                    .map(|(steps, _, chosen)| {
                        let default = effort_in_force(
                            None,
                            self.prefs.settings.effort_for(current),
                            current,
                            open.model(),
                            open.models(),
                        );
                        effort_rows(steps, chosen, default, |level| {
                            Consequence::Effort(Some(level))
                        })
                    })
                    .unwrap_or_default();
                Some(Content {
                    rows: self.listable(rows, current, serving.as_deref()),
                    efforts,
                    label: label.into(),
                    current,
                    handover: Provider::ALL
                        .into_iter()
                        .find(|provider| locked && *provider != current),
                    hold: tuning_hold(&open),
                })
            }
            PaneIdentity::Draft(_) => {
                let index = self.index_of(pane)?;
                let draft = self.panes[index].draft()?;
                let choice = draft.binding.provider();
                let current = choice.provider;
                let model = choice.model.clone();
                let (rows, _) = self.provider_rows(current, model.as_deref(), None, false, |choice| {
                    Consequence::Band(BandChoice::Provider(choice))
                });
                let announced = self.cockpit.announced_models(current);
                let label =
                    ferrite_core::providers::models::footer_name(current, model.as_deref(), &announced);
                let efforts = self
                    .draft_effort_ladder(index)
                    .map(|(steps, _, chosen)| {
                        let default = effort_in_force(
                            None,
                            self.prefs.settings.effort_for(current),
                            current,
                            model.as_deref(),
                            &announced,
                        );
                        effort_rows(steps, chosen, default, |level| {
                            Consequence::Band(BandChoice::Effort(Some(level)))
                        })
                    })
                    .unwrap_or_default();
                Some(Content {
                    rows: self.listable(rows, current, None),
                    efforts,
                    label: label.into(),
                    current,
                    handover: None,
                    hold: None,
                })
            }
        }
    }

    /// `provider_rows` as the list shows them: no section titles (the rail
    /// and each row's mark name the Provider), and the CLI's own `Default`
    /// only while nothing else can carry the ✓ — a known serving model
    /// takes it instead (Claude's model menu).
    fn listable(&self, rows: Vec<Row>, current: Provider, serving: Option<&str>) -> Vec<Row> {
        let mut rows: Vec<Row> = rows
            .into_iter()
            .filter(|row| row_choice(row).is_some())
            .collect();
        let of_current = |row: &Row| row_choice(row).is_some_and(|choice| choice.provider == current);
        let Some(default) = rows
            .iter()
            .position(|row| of_current(row) && row_choice(row).is_some_and(|c| c.model.is_none()))
        else {
            return rows;
        };
        let serving_row = serving
            .and_then(|serving| {
                self.cockpit
                    .model_catalog(current)
                    .into_iter()
                    .find(|model| model.value != "default" && model.is(serving))
            })
            .and_then(|model| {
                rows.iter().position(|row| {
                    of_current(row)
                        && row_choice(row).and_then(|c| c.model.as_deref()) == Some(model.value.as_str())
                })
            });
        let active = rows[default].active;
        if !active || serving_row.is_some() {
            if let Some(serving_row) = serving_row.filter(|_| active) {
                rows[serving_row].active = true;
            }
            rows.remove(default);
        }
        rows
    }

    /// The search line moved: list what matches across every Provider, the
    /// keyboard back on the list.
    fn model_picker_searched(&mut self, cx: &mut Context<Self>) {
        if let Some(Popover {
            kind: Kind::Provider(picker),
            ..
        }) = self.popover.as_mut()
        {
            picker.column = Column::List;
        }
        self.list_model_picker(cx);
    }

    fn model_picker(&self) -> Option<(&Popover, &ModelPicker)> {
        let open = self.popover.as_ref()?;
        match &open.kind {
            Kind::Provider(picker) => Some((open, picker)),
            _ => None,
        }
    }

    fn model_picker_mut(&mut self) -> Option<(&mut Vec<Row>, &mut usize, &mut ModelPicker)> {
        let open = self.popover.as_mut()?;
        match &mut open.kind {
            Kind::Provider(picker) => Some((&mut open.rows, &mut open.selected, picker)),
            _ => None,
        }
    }

    /// Show `tab`'s models, the rail's pill sliding to its button.
    pub(super) fn model_picker_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        let Some((_, _, picker)) = self.model_picker_mut() else {
            return;
        };
        if picker.tab != tab {
            let from = pill_top(picker.tab);
            picker.pill.set(Some(ferrite_ui::motion::Tween::retarget(
                picker.pill.get(),
                from,
                pill_top(tab),
                ferrite_ui::motion::RAIL_SLIDE,
                now,
                reduced,
            )));
            picker.tab = tab;
        }
        picker.column = Column::List;
        self.list_model_picker(cx);
    }

    /// ↑ / ↓: the cursor down the list or the column, held at the ends.
    fn model_picker_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some((_, selected, picker)) = self.model_picker_mut() else {
            return;
        };
        match picker.column {
            Column::List => {
                let Some(last) = picker.shown.len().checked_sub(1) else {
                    return;
                };
                let at = picker.shown.iter().position(|row| row == selected).unwrap_or(0);
                let next = (at as isize + delta).clamp(0, last as isize) as usize;
                *selected = picker.shown[next];
            }
            Column::Reasoning => {
                let Some(last) = picker.efforts.len().checked_sub(1) else {
                    return;
                };
                picker.effort_cursor =
                    (picker.effort_cursor as isize + delta).clamp(0, last as isize) as usize;
            }
        }
        if let Some(open) = self.popover.as_ref() {
            let at = self
                .model_picker()
                .and_then(|(_, picker)| picker.shown.iter().position(|row| *row == open.selected));
            if let Some(at) = at {
                open.scroll.scroll_to_item(at);
            }
        }
        cx.notify();
    }

    /// ↵: pick the cursor's model, or its rung.
    fn model_picker_confirm(&mut self, cx: &mut Context<Self>) {
        let Some((open, picker)) = self.model_picker() else {
            return;
        };
        match picker.column {
            Column::List => {
                let row = open.selected;
                self.pick_model_row(row, cx);
            }
            Column::Reasoning => {
                let rung = picker.effort_cursor;
                self.pick_effort_rung(rung, cx);
            }
        }
    }

    /// ←: from the column back to the list; on the list with nothing
    /// typed, the rail's tab above. Otherwise the line's caret moves.
    fn model_picker_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, picker)) = self.model_picker() else {
            return;
        };
        if picker.column == Column::Reasoning {
            if let Some((_, _, picker)) = self.model_picker_mut() {
                picker.column = Column::List;
            }
            cx.notify();
            return;
        }
        if !picker.query(cx).is_empty() {
            window.dispatch_action(Box::new(ferrite_pane::composer::Left), cx);
            return;
        }
        let tabs: Vec<Tab> = rail_tabs().collect();
        let at = tabs.iter().position(|tab| *tab == picker.tab).unwrap_or(0);
        let tab = tabs[(at + tabs.len() - 1) % tabs.len()];
        self.model_picker_tab(tab, cx);
    }

    /// →: at the line's end, into the Reasoning column — picking the
    /// cursor's model first when it is not the one in force, so the column
    /// shows its rungs (T3's).
    fn model_picker_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((open, picker)) = self.model_picker() else {
            return;
        };
        if picker.column == Column::Reasoning {
            return;
        }
        let search = picker.search.read(cx);
        if search.cursor() < search.text().len() {
            window.dispatch_action(Box::new(ferrite_pane::composer::Right), cx);
            return;
        }
        let row = open.selected;
        if open.rows.get(row).is_some_and(|row| !row.active && !row.inert) {
            self.pick_model_row(row, cx);
        } else if let Some((_, _, picker)) = self.model_picker_mut() {
            picker.column = Column::Reasoning;
            cx.notify();
        }
    }

    /// ⌘N: pick the Nth listed model (T3's `modelPicker.jump.N`).
    fn model_picker_jump(&mut self, nth: usize, cx: &mut Context<Self>) {
        let Some((open, picker)) = self.model_picker() else {
            return;
        };
        let Some(row) = picker.shown.iter().copied().filter(|at| !open.rows[*at].inert).nth(nth)
        else {
            return;
        };
        self.pick_model_row(row, cx);
    }

    fn model_picker_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.popover.take() else {
            return;
        };
        if let Some(index) = self.index_of(open.pane) {
            let focus = self.panes[index].composer.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// Pick model row `row` (an index into the popover's rows): the Thread
    /// re-aimed — or the draft's band choice made — and the picker kept
    /// open on the new model, the keyboard in its column. A turn that began
    /// since the picker drew holds every row instead.
    pub(super) fn pick_model_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some((open, _)) = self.model_picker() else {
            return;
        };
        let pane = open.pane;
        if self.picker_held(pane) {
            self.rebuild_model_picker(None, cx);
            return;
        }
        let Some(consequence) = open
            .rows
            .get(row)
            .filter(|row| !row.inert)
            .map(|row| row.consequence.clone())
        else {
            return;
        };
        if let Some((_, selected, _)) = self.model_picker_mut() {
            *selected = row;
        }
        self.apply_picker_choice(pane, consequence, cx);
        self.rebuild_model_picker(Some(Column::Reasoning), cx);
    }

    /// Pick rung `at` of the Reasoning column; the picker stays open.
    pub(super) fn pick_effort_rung(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some((open, picker)) = self.model_picker() else {
            return;
        };
        let pane = open.pane;
        if self.picker_held(pane) {
            self.rebuild_model_picker(None, cx);
            return;
        }
        let Some(consequence) = picker
            .efforts
            .get(at)
            .filter(|row| !row.inert)
            .map(|row| row.consequence.clone())
        else {
            return;
        };
        if let Some((_, _, picker)) = self.model_picker_mut() {
            picker.effort_cursor = at;
            picker.column = Column::Reasoning;
        }
        self.apply_picker_choice(pane, consequence, cx);
        self.rebuild_model_picker(None, cx);
    }

    /// A turn started, or a change still waits on the provider: a visible
    /// choice must not become a certain refusal in the transcript.
    fn picker_held(&self, pane: PaneIdentity) -> bool {
        pane.thread().is_some_and(|thread| {
            self.cockpit
                .thread(thread)
                .and_then(|thread| tuning_hold(&thread))
                .is_some()
        })
    }

    fn apply_picker_choice(&mut self, pane: PaneIdentity, choice: Consequence, cx: &mut Context<Self>) {
        match (choice, pane.thread()) {
            (Consequence::Provision(choice), Some(thread)) => self.pick_provider(thread, choice, cx),
            (Consequence::Effort(effort), Some(thread)) => self.pick_effort(thread, effort, cx),
            (Consequence::Band(choice), None) => {
                let Some(index) = self.index_of(pane) else {
                    return;
                };
                self.focus_pane(index);
                let composer = self.panes[index].composer.clone();
                self.pick_band(&choice, &composer, cx);
            }
            _ => {}
        }
    }

    /// The star on row `row`: the model joins Favourites, or leaves them,
    /// saved at once.
    pub(super) fn toggle_favourite(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(choice) = self
            .model_picker()
            .and_then(|(open, _)| open.rows.get(row))
            .and_then(row_choice)
            .cloned()
        else {
            return;
        };
        self.change_settings(
            |settings| settings.toggle_favourite(choice.provider, favourite_value(&choice)),
            cx,
        );
        self.list_model_picker(cx);
    }

    /// The picker's keys, heard on the cockpit's root while its line holds
    /// the keyboard.
    pub(super) fn register_model_picker_actions(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|view, _: &Up, _, cx| view.model_picker_step(-1, cx)))
            .on_action(cx.listener(|view, _: &Down, _, cx| view.model_picker_step(1, cx)))
            .on_action(cx.listener(|view, _: &Confirm, _, cx| view.model_picker_confirm(cx)))
            .on_action(
                cx.listener(|view, _: &Dismiss, window, cx| view.model_picker_dismiss(window, cx)),
            )
            .on_action(cx.listener(|view, _: &Left, window, cx| view.model_picker_left(window, cx)))
            .on_action(
                cx.listener(|view, _: &Right, window, cx| view.model_picker_right(window, cx)),
            )
            .on_action(cx.listener(|view, _: &Jump1, _, cx| view.model_picker_jump(0, cx)))
            .on_action(cx.listener(|view, _: &Jump2, _, cx| view.model_picker_jump(1, cx)))
            .on_action(cx.listener(|view, _: &Jump3, _, cx| view.model_picker_jump(2, cx)))
            .on_action(cx.listener(|view, _: &Jump4, _, cx| view.model_picker_jump(3, cx)))
            .on_action(cx.listener(|view, _: &Jump5, _, cx| view.model_picker_jump(4, cx)))
            .on_action(cx.listener(|view, _: &Jump6, _, cx| view.model_picker_jump(5, cx)))
            .on_action(cx.listener(|view, _: &Jump7, _, cx| view.model_picker_jump(6, cx)))
            .on_action(cx.listener(|view, _: &Jump8, _, cx| view.model_picker_jump(7, cx)))
            .on_action(cx.listener(|view, _: &Jump9, _, cx| view.model_picker_jump(8, cx)))
    }

    /// The open picker, hung over its Pane's Composer: its foot
    /// `MODEL_PICKER_GAP` over the box, its left edge at the chip's (T3's
    /// `side="top" align="start"`), kept inside the window. It waits a
    /// frame for the box to be laid out.
    pub(super) fn model_picker_element(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (open, picker) = self.model_picker()?;
        let index = self.index_of(open.pane)?;
        let Some(field) = self.panes[index].geometry.get().composer else {
            ferrite_ui::motion::request_frame(window);
            return None;
        };
        if !picker.focused.replace(true) {
            let focus = picker.search.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        let left = self
            .float_triggers
            .borrow()
            .get(chip_trigger(open.pane).as_ref())
            .map_or(field.left(), |chip| chip.left());
        let viewport = f32::from(window.viewport_size().width);
        let width = (MODEL_PICKER_LIST_W + REASONING_W).min(viewport - 2. * GRID_PAD);
        let surface = self.model_picker_surface(open, picker, width, window, cx);
        Some(
            deferred(
                anchored()
                    .anchor(gpui::Anchor::BottomLeft)
                    .position(gpui::point(left, field.top() - px(MODEL_PICKER_GAP)))
                    .snap_to_window_with_margin(px(GRID_PAD))
                    .child(ferrite_ui::motion::menu_in(
                        "model-picker-in",
                        surface,
                        ferrite_ui::motion::Opens::Up,
                    )),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    fn model_picker_surface(
        &self,
        open: &Popover,
        picker: &ModelPicker,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let searching = !picker.query(cx).is_empty();
        let body = div()
            .debug_selector(|| "model-picker-body".into())
            .flex()
            .flex_row()
            .h(px(MODEL_PICKER_H))
            .min_h_0()
            .when(!searching, |body| body.child(self.model_picker_rail(picker, window, cx)))
            .child(self.model_picker_list(open, picker, searching, cx))
            .child(self.model_picker_reasoning(picker, cx));
        ferrite_ui::menu::float()
            .debug_selector(|| "model-picker".into())
            .w(px(width))
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(body)
            .child(self.model_picker_footer(open, cx))
    }

    fn model_picker_rail(
        &self,
        picker: &ModelPicker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        let top = match picker.pill.get() {
            Some(tween) => {
                if tween.running(now, reduced) {
                    ferrite_ui::motion::request_frame(window);
                }
                tween.value(now, reduced)
            }
            None => pill_top(picker.tab),
        };
        let button = MODEL_PICKER_RAIL_W - 2. * MODEL_PICKER_RAIL_PAD;
        let mut rail = div()
            .debug_selector(|| "model-picker-rail".into())
            .relative()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(MODEL_PICKER_RAIL_W))
            .p(px(MODEL_PICKER_RAIL_PAD))
            .gap(px(MODEL_PICKER_RAIL_PAD))
            .bg(rgba(MODEL_PICKER_RAIL_GROUND))
            .child(
                div()
                    .debug_selector(|| "model-picker-pill".into())
                    .absolute()
                    .right(px(MODEL_PICKER_RAIL_PAD))
                    .top(px(top))
                    .w(px(MODEL_PICKER_PILL_W))
                    .h(px(MODEL_PICKER_PILL_H))
                    .rounded_full()
                    .bg(rgb(SEND_LIT)),
            );
        for tab in rail_tabs() {
            let (selector, (path, ink), tip, reason) = match tab {
                Tab::Favourites => (
                    "model-picker-rail-favourites".to_string(),
                    (ferrite_ui::icons::STAR, INK_STRONG),
                    SharedString::from("Favourites"),
                    None,
                ),
                Tab::Provider(provider) => {
                    let reason = picker
                        .unavailable
                        .iter()
                        .find(|(p, _)| *p == provider)
                        .map(|(_, why)| why.clone());
                    (
                        format!("model-picker-rail-{}", ferrite_ui::provider::word(provider)),
                        ferrite_ui::provider::mark(provider),
                        reason
                            .clone()
                            .unwrap_or_else(|| ferrite_ui::provider::title(provider).into()),
                        reason,
                    )
                }
            };
            let dead = reason.is_some();
            let id = SharedString::from(selector.clone());
            rail = rail.child(
                div()
                    .id(id)
                    .debug_selector(move || selector.clone())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(px(button))
                    .rounded(px(R_CONTROL))
                    .when(dead, |button| button.opacity(0.5))
                    .when(!dead, |button| {
                        button
                            .cursor_pointer()
                            .hover(|style| style.bg(rgba(MODEL_PICKER_RAIL_HOVER)))
                    })
                    .tooltip(ferrite_ui::menu::tooltip(tip))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if !dead {
                                view.model_picker_tab(tab, cx);
                            }
                        }),
                    )
                    .child(ferrite_ui::icons::icon(path, MODEL_PICKER_RAIL_ICON, ink)),
            );
            if tab == Tab::Favourites {
                rail = rail.child(
                    div()
                        .flex_shrink_0()
                        .h(px(1.))
                        .bg(rgba(MODEL_PICKER_EDGE)),
                );
            }
        }
        rail
    }

    fn model_picker_list(
        &self,
        open: &Popover,
        picker: &ModelPicker,
        searching: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let settings = &self.prefs.settings;
        let search = div()
            .flex_shrink_0()
            .px(px(SPACE_3))
            .pt(px(MODEL_PICKER_INSET_T))
            .child(
                div()
                    .debug_selector(|| "model-picker-search".into())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1))
                    .pb(px(SPACE_1_5))
                    .border_b_1()
                    .border_color(rgba(MODEL_PICKER_EDGE))
                    .child(
                        div()
                            .flex_shrink_0()
                            .opacity(0.55)
                            .child(ferrite_ui::icons::icon(
                                ferrite_ui::icons::SEARCH,
                                MODEL_PICKER_SEARCH_ICON,
                                INK_SUBTLE,
                            )),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h(px(MODEL_PICKER_SEARCH_H))
                            .flex()
                            .items_center()
                            .font_family(FONT_UI)
                            .text_size(px(FS_UI))
                            .line_height(px(LH_UI))
                            .text_color(rgb(INK_STRONG))
                            .when(!searching, |line| {
                                line.child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .text_color(rgb(PLACEHOLDER_INK))
                                        .child("Search models..."),
                                )
                            })
                            .child(div().w_full().child(picker.search.clone())),
                    ),
            );
        let live: Vec<usize> = picker
            .shown
            .iter()
            .copied()
            .filter(|at| !open.rows[*at].inert)
            .take(JUMP_LABELS.len())
            .collect();
        let mut rows = div()
            .id("model-picker-rows")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .p(px(SPACE_1))
            .gap(px(MODEL_PICKER_ROW_GAP))
            .overflow_y_scroll()
            .track_scroll(&open.scroll);
        if !open.revealed.replace(true) {
            if let Some(at) = picker.shown.iter().position(|row| *row == open.selected) {
                open.scroll.scroll_to_item(at);
            }
        }
        for (place, at) in picker.shown.iter().copied().enumerate() {
            let row = &open.rows[at];
            let choice = row_choice(row);
            let favourite = is_favourite(settings, row);
            let jump = live.iter().position(|live| *live == at).map(|nth| JUMP_LABELS[nth]);
            let handover = choice.is_some_and(|choice| Some(choice.provider) == picker.handover);
            rows = rows.child(self.model_row(
                place,
                at,
                row,
                ModelRowFace {
                    provider: choice.map(|choice| choice.provider),
                    cursor: picker.column == Column::List && open.selected == at,
                    favourite,
                    star_on_hover: picker.tab == Tab::Favourites && !searching,
                    jump,
                    note: handover.then_some("handover"),
                    why: open.hold.map(SharedString::from).or_else(|| {
                        (!row.detail.is_empty()).then(|| row.detail.clone())
                    }),
                },
                cx,
            ));
        }
        if picker.shown.is_empty() {
            rows = rows.child(
                div()
                    .px(px(SPACE_2))
                    .py(px(SPACE_1_5))
                    .text_size(px(FS_UI))
                    .text_color(rgb(INK_SUBTLE))
                    .child(if picker.tab == Tab::Favourites && !searching {
                        "No favourites yet"
                    } else {
                        "No models found"
                    }),
            );
        }
        div()
            .debug_selector(|| "model-picker-list".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .bg(rgba(MODEL_PICKER_LIST_GROUND))
            .when(!searching, |list| list.border_l_1().border_color(rgba(MODEL_PICKER_EDGE)))
            .child(search)
            .child(rows)
    }

    fn model_row(
        &self,
        place: usize,
        at: usize,
        row: &Row,
        face: ModelRowFace,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let group = SharedString::from(format!("model-picker-row-{place}"));
        let selector = group.to_string();
        let star_selector = format!("model-picker-star-{place}");
        let star_ink = if face.favourite { YELLOW } else { INK_SUBTLE };
        let star_shown = face.favourite && !face.star_on_hover || face.cursor;
        let star = div()
            .id(SharedString::from(star_selector.clone()))
            .debug_selector(move || star_selector.clone())
            .flex()
            .items_center()
            .justify_center()
            .size(px(KEY_CAP_SOFT))
            .rounded(px(R_CONTROL))
            .cursor_pointer()
            .when(!star_shown, |star| {
                star.opacity(0.).group_hover(group.clone(), |style| style.opacity(1.))
            })
            .hover(|style| style.bg(rgba(MODEL_PICKER_RAIL_HOVER)))
            .tooltip(ferrite_ui::menu::tooltip(if face.favourite {
                "Remove from favourites"
            } else {
                "Add to favourites"
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.toggle_favourite(at, cx);
                }),
            )
            .child(ferrite_ui::icons::icon(ferrite_ui::icons::STAR, 12., star_ink));
        let inert = row.inert;
        div()
            .id(group.clone())
            .group(group.clone())
            .debug_selector(move || selector.clone())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(MODEL_PICKER_ROW_H))
            .px(px(SPACE_2))
            .rounded(px(MODEL_PICKER_ROW_R))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .when(face.cursor, |row| row.bg(rgba(MODEL_PICKER_ROW_HOVER)))
            .when(inert, |row| row.opacity(0.64))
            .when(!inert, |row| {
                row.cursor_pointer()
                    .hover(|style| style.bg(rgba(MODEL_PICKER_ROW_HOVER)))
            })
            .when_some(face.why, |row, why| row.tooltip(ferrite_ui::menu::tooltip(why)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.pick_model_row(at, cx);
                }),
            )
            .children(face.provider.map(|provider| {
                let (path, ink) = ferrite_ui::provider::mark(provider);
                ferrite_ui::icons::icon(path, MODEL_PICKER_MARK, ink)
            }))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(INK_STRONG))
                            .child(row.name.clone()),
                    )
                    .when(row.active, |name| {
                        name.child(
                            div()
                                .debug_selector(|| "model-picker-check".into())
                                .child(ferrite_ui::icons::icon(
                                    ferrite_ui::icons::CHECK,
                                    14.,
                                    MENU_CHECK_INK,
                                )),
                        )
                    })
                    .children(face.note.map(|note| {
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(FS_BADGE))
                            .text_color(rgb(INK_SUBTLE))
                            .child(note)
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .child(star)
                    .children(face.jump.map(super::panel::soft_key_cap)),
            )
            .into_any_element()
    }

    fn model_picker_reasoning(&self, picker: &ModelPicker, cx: &mut Context<Self>) -> Div {
        let head = div().flex_shrink_0().px(px(SPACE_3)).pt(px(MODEL_PICKER_INSET_T)).child(
            div()
                .flex()
                .items_center()
                .gap(px(SPACE_2))
                .h(px(REASONING_HEAD_H))
                .pb(px(SPACE_1_5))
                .border_b_1()
                .border_color(rgba(MODEL_PICKER_EDGE))
                .font_family(FONT_UI)
                .text_size(px(FS_UI))
                .text_color(rgb(INK_SUBTLE))
                .child(div().flex_shrink_0().child("Reasoning"))
                .child(
                    div()
                        .ml_auto()
                        .min_w_0()
                        .truncate()
                        .text_size(px(FS_BADGE))
                        .child(picker.model_label.clone()),
                ),
        );
        let mut rungs = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(MODEL_PICKER_ROW_GAP))
            .px(px(SPACE_2))
            .pt(px(MODEL_PICKER_INSET_T))
            .pb(px(SPACE_1_5))
            .font_family(FONT_UI)
            .text_size(px(FS_UI));
        if picker.efforts.is_empty() {
            rungs = rungs.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(REASONING_ROW_H))
                    .px(px(SPACE_2))
                    .text_color(rgb(INK_SUBTLE))
                    .child("No effort levels"),
            );
        }
        for (at, rung) in picker.efforts.iter().enumerate() {
            let selector = format!("model-picker-effort-{at}");
            let cursor = picker.column == Column::Reasoning && picker.effort_cursor == at;
            let inert = rung.inert;
            let why = (!rung.detail.is_empty()).then(|| rung.detail.clone());
            rungs = rungs.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(REASONING_ROW_H))
                    .px(px(SPACE_2))
                    .rounded(px(MODEL_PICKER_ROW_R))
                    .when(cursor, |row| row.bg(rgba(MODEL_PICKER_ROW_HOVER)))
                    .when(inert, |row| row.opacity(0.64))
                    .when(!inert, |row| {
                        row.cursor_pointer()
                            .hover(|style| style.bg(rgba(MODEL_PICKER_ROW_HOVER)))
                    })
                    .when_some(why, |row, why| row.tooltip(ferrite_ui::menu::tooltip(why)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.pick_effort_rung(at, cx);
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(INK_STRONG))
                            .child(rung.name.clone()),
                    )
                    .map(|row| {
                        if rung.active {
                            row.child(ferrite_ui::icons::icon(
                                ferrite_ui::icons::CHECK,
                                14.,
                                MENU_CHECK_INK,
                            ))
                        } else if let Some(tag) = rung.tag.clone() {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(FS_BADGE))
                                    .text_color(rgb(INK_SUBTLE))
                                    .child(tag),
                            )
                        } else {
                            row
                        }
                    }),
            );
        }
        div()
            .debug_selector(|| "model-picker-reasoning".into())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(REASONING_W))
            .border_l_1()
            .border_color(rgba(MODEL_PICKER_EDGE))
            .bg(rgba(MODEL_PICKER_LIST_GROUND))
            .child(head)
            .child(rungs)
    }

    /// The footer (T3's `ComposerPickerFooter`): the permission mode at its
    /// far end, opening the permission menu over it. Speed and Ultracode
    /// segments stand only where a model announces them, and no provider
    /// announces either to Ferrite yet.
    fn model_picker_footer(&self, open: &Popover, cx: &mut Context<Self>) -> Div {
        div()
            .debug_selector(|| "model-picker-footer".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_3))
            .py(px(SPACE_2))
            .border_t_1()
            .border_color(rgba(MODEL_PICKER_EDGE))
            .bg(rgba(MODEL_PICKER_LIST_GROUND))
            .children(self.picker_permissions(open.pane, cx))
    }

    /// The footer's permissions button: the mode's icon, its label and a
    /// chevron; on a Thread that can switch modes it opens the permission
    /// menu, on a draft it says the mode the Thread will start in.
    fn picker_permissions(&self, pane: PaneIdentity, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (label, mode, opens) = match pane {
            PaneIdentity::Thread(thread) => {
                let open = self.cockpit.thread(thread)?;
                let modes = open.permission_modes();
                if modes.is_empty() && open.permission_mode().is_none() {
                    return None;
                }
                let mode = open.permission_mode().unwrap_or("default").to_string();
                let opens = (!modes.is_empty()
                    && open.supports_control(ferrite_core::ControlKind::SetPermissionMode))
                .then(|| (thread, open.generation()));
                (pane::permission_mode_label(&mode, &modes), mode, opens)
            }
            PaneIdentity::Draft(_) => {
                let index = self.index_of(pane)?;
                let draft = self.panes[index].draft()?;
                let mode = self
                    .prefs
                    .settings
                    .permission_mode_for(draft.binding.provider().provider)
                    .to_string();
                (pane::permission_mode_label(&mode, &[]), mode, None)
            }
        };
        let ink = if ferrite_ui::theme::mode_is_manual(&mode) {
            MODE_ATTENTION_INK
        } else {
            INK_SUBTLE
        };
        let lit = opens.is_some() && self.mode_picker == opens;
        Some(
            ferrite_ui::components::on_bounds(
                div()
                    .id("model-picker-permissions")
                    .debug_selector(|| "model-picker-permissions".into())
                    .ml_auto()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .h(px(PICKER_FOOT_BUTTON_H))
                    .px(px(SPACE_2))
                    .rounded(px(R_CONTROL))
                    .font_family(FONT_UI)
                    .text_size(px(FS_BADGE))
                    .text_color(rgb(if lit { INK_STRONG } else { ink }))
                    .when(lit, |button| button.bg(rgba(MODEL_PICKER_ROW_HOVER)))
                    .when(opens.is_some(), |button| {
                        button.cursor_pointer().hover(|style| {
                            style
                                .bg(rgba(MODEL_PICKER_ROW_HOVER))
                                .text_color(rgb(INK_STRONG))
                        })
                    })
                    .tooltip(ferrite_ui::menu::tooltip("Permission mode"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if let Some((thread, generation)) = opens {
                                view.toggle_mode_menu(thread, generation, true, cx);
                            }
                        }),
                    )
                    .child(ferrite_ui::icons::icon(ferrite_ui::icons::SHIELD, 14., ink))
                    .child(label)
                    .when(opens.is_some(), |button| {
                        button.child(ferrite_ui::icons::icon(
                            ferrite_ui::icons::CHEVRON_UP,
                            12.,
                            INK_SUBTLE,
                        ))
                    }),
                self.record_trigger(PICKER_PERMISSIONS.into()),
            )
            .into_any_element(),
        )
    }
}

/// The key the picker footer's permissions button records its bounds
/// under, where the permission menu then hangs.
pub(super) const PICKER_PERMISSIONS: &str = "model-picker-permissions";

/// How one model row reads, beyond the row itself.
struct ModelRowFace {
    provider: Option<Provider>,
    cursor: bool,
    favourite: bool,
    /// In Favourites every row is starred, so its star shows on hover.
    star_on_hover: bool,
    jump: Option<&'static str>,
    /// The muted word after the name (`handover`).
    note: Option<&'static str>,
    /// The tooltip: why the row is dead for now, else what the model is.
    why: Option<SharedString>,
}

/// The Reasoning column's rungs: the model's ladder in title case, the ✓
/// on the level in force, `Default` on the level a Thread starts on when
/// none is chosen, each rung's line its tooltip.
fn effort_rows(
    steps: Vec<String>,
    chosen: Option<usize>,
    default: Option<SharedString>,
    pick: impl Fn(String) -> Consequence,
) -> Vec<Row> {
    steps
        .into_iter()
        .enumerate()
        .map(|(at, step)| {
            let is_default = default
                .as_ref()
                .is_some_and(|default| *default == effort_chip_label(&step));
            Row {
                row: pane::MenuRow {
                    insert: SharedString::default(),
                    name: effort_title(&step).into(),
                    matched: Vec::new(),
                    detail: effort_detail(&step).into(),
                    prose_detail: true,
                    inert: false,
                    tag: is_default.then(|| "Default".into()),
                },
                active: chosen == Some(at),
                consequence: pick(step),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(provider: Provider, model: &str, name: &str) -> Row {
        Row {
            row: pane::MenuRow {
                insert: SharedString::default(),
                name: name.to_string().into(),
                matched: Vec::new(),
                detail: SharedString::default(),
                prose_detail: true,
                inert: false,
                tag: None,
            },
            active: false,
            consequence: Consequence::Provision(ProviderChoice {
                provider,
                model: Some(model.into()),
            }),
        }
    }

    /// A search reaches every Provider, the exact name first, then the
    /// closest match.
    #[test]
    fn a_search_ranks_across_providers() {
        let rows = vec![
            row(Provider::Claude, "opus", "Opus 5.5"),
            row(Provider::Claude, "sonnet", "Sonnet 5"),
            row(Provider::Codex, "gpt-5.6", "GPT-5.6"),
            row(Provider::Codex, "gpt-5.6-sol", "GPT-5.6 Sol"),
        ];
        let settings = ferrite_core::settings::Settings::default();
        assert_eq!(ranked(&rows, "gpt-5.6", &settings), [2, 3]);
        assert_eq!(ranked(&rows, "codex", &settings), [2, 3]);
        assert_eq!(ranked(&rows, "so", &settings)[0], 1, "Sonnet's run beats Sol's");
        assert!(ranked(&rows, "zzz", &settings).is_empty());
    }

    /// Favourites lists the starred models in Provider order; a Provider's
    /// tab lists its own with its starred ones first.
    #[test]
    fn favourites_lead_their_tabs() {
        let rows = vec![
            row(Provider::Claude, "opus", "Opus 5.5"),
            row(Provider::Claude, "sonnet", "Sonnet 5"),
            row(Provider::Codex, "gpt-5.6", "GPT-5.6"),
        ];
        let mut settings = ferrite_core::settings::Settings::default();
        settings.toggle_favourite(Provider::Codex, "gpt-5.6");
        settings.toggle_favourite(Provider::Claude, "sonnet");
        assert_eq!(tab_rows(&rows, Tab::Favourites, &settings), [1, 2]);
        assert_eq!(tab_rows(&rows, Tab::Provider(Provider::Claude), &settings), [1, 0]);
        assert_eq!(tab_rows(&rows, Tab::Provider(Provider::Codex), &settings), [2]);
    }

    /// The pill stands centred on its button, past the hairline under
    /// Favourites for the Providers.
    #[test]
    fn the_rail_pill_centres_on_its_button() {
        assert_eq!(pill_top(Tab::Favourites), 4. + 18. - 10.);
        assert_eq!(pill_top(Tab::Provider(Provider::Claude)), 4. + 40. + 1. + 4. + 18. - 10.);
        assert_eq!(
            pill_top(Tab::Provider(Provider::Codex)) - pill_top(Tab::Provider(Provider::Claude)),
            40.
        );
    }
}
