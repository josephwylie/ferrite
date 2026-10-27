//! What the Settings sheet holds: pages of groups of rows, each row a
//! setting and the control that changes it. The Cockpit says which
//! settings exist and what a change does (`CockpitView::change_settings`
//! owns values and persistence); the frame lays them out, searches them
//! and draws their controls (`ferrite-frame`).

use std::rc::Rc;

use gpui::component::input::InputState;
use gpui::{App, Entity, SharedString, Window};

/// The Settings pages, in sidebar order. `About` is pinned to the
/// sidebar's foot, apart from the settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageKey {
    #[default]
    NewThreads,
    Permissions,
    Behaviour,
    Notifications,
    Server,
    About,
}

impl PageKey {
    pub const ALL: [PageKey; 6] = [
        PageKey::NewThreads,
        PageKey::Permissions,
        PageKey::Behaviour,
        PageKey::Notifications,
        PageKey::Server,
        PageKey::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            PageKey::NewThreads => "New threads",
            PageKey::Permissions => "Permissions",
            PageKey::Behaviour => "Behaviour",
            PageKey::Notifications => "Notifications",
            PageKey::Server => "Server",
            PageKey::About => "About",
        }
    }

    /// The sidebar row's selector, `settings-page-<slug>`.
    pub fn selector(self) -> &'static str {
        match self {
            PageKey::NewThreads => "settings-page-new-threads",
            PageKey::Permissions => "settings-page-permissions",
            PageKey::Behaviour => "settings-page-behaviour",
            PageKey::Notifications => "settings-page-notifications",
            PageKey::Server => "settings-page-server",
            PageKey::About => "settings-page-about",
        }
    }

    /// The page `delta` rows away in sidebar order, held at either end.
    pub fn step(self, delta: isize) -> PageKey {
        let at = PageKey::ALL
            .iter()
            .position(|page| *page == self)
            .unwrap_or(0) as isize;
        let to = (at + delta).clamp(0, PageKey::ALL.len() as isize - 1);
        PageKey::ALL[to as usize]
    }
}

/// What a row's token does when pressed.
pub type Act = Rc<dyn Fn(&mut App)>;

/// A row's control, as the frame draws it.
#[derive(Clone)]
pub enum Control {
    /// Two options, a token each, the chosen one inverse.
    Tokens {
        id: &'static str,
        options: Vec<(SharedString, bool)>,
        pick: Rc<dyn Fn(usize, &mut App)>,
    },
    /// More options: the chosen one's label and a chevron that opens them.
    Chooser {
        id: &'static str,
        selected: SharedString,
        options: Vec<(SharedString, bool)>,
        pick: Rc<dyn Fn(usize, &mut App)>,
    },
    /// On or off, as one word.
    Toggle {
        id: &'static str,
        checked: bool,
        change: Rc<dyn Fn(bool, &mut App)>,
    },
    /// `‹ 14px ›`.
    Stepper {
        id: &'static str,
        value: SharedString,
        can_down: bool,
        can_up: bool,
        step: Rc<dyn Fn(i32, &mut App)>,
    },
    /// A chosen file's name, then a token per act.
    FileChoice {
        id: &'static str,
        value: SharedString,
        acts: Vec<(&'static str, Act)>,
    },
    /// A token per act, the `nth` of its kind.
    Named {
        id: &'static str,
        nth: usize,
        acts: Vec<(SharedString, Act)>,
    },
    /// A field and the token that takes what is typed in it.
    Entry {
        id: &'static str,
        field: Entity<InputState>,
        label: &'static str,
        act: Rc<dyn Fn(&mut Window, &mut App)>,
    },
    /// A read-only value.
    Fact { value: SharedString },
    /// The one button that does something about the row, or its value as a
    /// fact reads when there is nothing to do.
    Action {
        id: &'static str,
        value: SharedString,
        button: Option<SharedString>,
        act: Act,
    },
    /// A path shown from `~`, copied whole on a click.
    PathFact {
        path: String,
        shown: SharedString,
        copied: bool,
        on_copy: Act,
    },
}

/// One setting: its label, an optional one-line hint, the words search
/// also finds it by (every option label), and its control.
#[derive(Clone)]
pub struct Row {
    pub title: SharedString,
    pub hint: SharedString,
    pub words: Vec<SharedString>,
    pub control: Control,
}

impl Row {
    pub fn new(
        title: impl Into<SharedString>,
        hint: impl Into<SharedString>,
        words: Vec<SharedString>,
        control: Control,
    ) -> Self {
        Self {
            title: title.into(),
            hint: hint.into(),
            words,
            control,
        }
    }

    /// A fact's value gives way (wraps) rather than hold its width.
    pub fn is_fact(&self) -> bool {
        matches!(
            self.control,
            Control::Fact { .. } | Control::PathFact { .. }
        )
    }

    /// Case-insensitive substring on the label, the hint, the option
    /// labels, and the group and page it sits in (`claude` finds every
    /// Claude row).
    pub fn matches(&self, query: &str, page: &str, group: Option<&str>) -> bool {
        let query = query.to_lowercase();
        [self.title.as_ref(), self.hint.as_ref(), page]
            .into_iter()
            .chain(group)
            .chain(self.words.iter().map(|word| word.as_ref()))
            .any(|text| text.to_lowercase().contains(&query))
    }
}

/// A group of rows under a quiet label; a provider group's label leads
/// with its logomark in brand colour.
#[derive(Clone)]
pub struct Group {
    pub title: Option<&'static str>,
    pub mark: Option<(&'static str, u32)>,
    pub rows: Vec<Row>,
}

impl Group {
    pub fn new(title: Option<&'static str>) -> Self {
        Self {
            title,
            mark: None,
            rows: Vec::new(),
        }
    }

    pub fn mark(mut self, path: &'static str, ink: u32) -> Self {
        self.mark = Some((path, ink));
        self
    }

    pub fn rows(mut self, rows: impl IntoIterator<Item = Row>) -> Self {
        self.rows.extend(rows);
        self
    }
}

pub struct Page {
    pub key: PageKey,
    pub groups: Vec<Group>,
}

pub fn page(key: PageKey, groups: Vec<Group>) -> Page {
    Page { key, groups }
}

/// `options`' labels and chosen flags, and a pick by index that changes
/// the setting to that option's value.
fn erase<T: Clone + 'static>(
    options: Vec<(SharedString, bool, T)>,
    change: impl Fn(T, &mut App) + 'static,
) -> (Vec<(SharedString, bool)>, Rc<dyn Fn(usize, &mut App)>) {
    let shown = options
        .iter()
        .map(|(label, selected, _)| (label.clone(), *selected))
        .collect();
    let values: Vec<T> = options.into_iter().map(|(_, _, value)| value).collect();
    let pick = Rc::new(move |at: usize, cx: &mut App| change(values[at].clone(), cx));
    (shown, pick)
}

/// Two options are a pair of tokens, the chosen one inverse; more are a
/// chooser (the value and a chevron that opens a menu).
pub fn choices<T: Clone + 'static>(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    options: Vec<(SharedString, bool, T)>,
    change: impl Fn(T, &mut App) + 'static,
) -> Row {
    // Long ladders are values to choose, not a second row of navigation.
    if options.len() > 2 {
        return chooser(id, title, detail, options, change);
    }
    let keywords: Vec<_> = options.iter().map(|(label, _, _)| label.clone()).collect();
    let (options, pick) = erase(options, change);
    Row::new(
        title,
        detail,
        keywords,
        Control::Tokens { id, options, pick },
    )
}

/// A choice from a list: the current value and a chevron; the menu keeps
/// every value, the standing one checked. Search also indexes the hidden
/// labels.
pub fn chooser<T: Clone + 'static>(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    options: Vec<(SharedString, bool, T)>,
    change: impl Fn(T, &mut App) + 'static,
) -> Row {
    let keywords: Vec<_> = options.iter().map(|(label, _, _)| label.clone()).collect();
    let selected = options
        .iter()
        .find(|(_, selected, _)| *selected)
        .map(|(label, _, _)| label.clone())
        .unwrap_or_else(|| "Choose".into());
    let (options, pick) = erase(options, change);
    Row::new(
        title,
        detail,
        keywords,
        Control::Chooser {
            id,
            selected,
            options,
            pick,
        },
    )
}

/// The unset value of every setting a CLI decides for itself.
pub const CLI_DEFAULT: &str = "CLI default";

/// Keep the effective selected value represented even when it is an alias or
/// absent from the current catalog. No choice is silently made for the user.
pub fn model_options(
    catalog: Vec<ferrite_core::ModelInfo>,
    chosen: Option<&str>,
) -> Vec<(SharedString, bool, Option<String>)> {
    let chosen = chosen.filter(|value| *value != "default");
    let mut represented = chosen.is_none();
    let mut options = vec![(CLI_DEFAULT.into(), represented, None)];
    for model in catalog.into_iter().filter(|model| model.value != "default") {
        let selected = !represented && chosen.is_some_and(|chosen| model.is(chosen));
        represented |= selected;
        options.push((model.display.into(), selected, Some(model.value)));
    }
    if !represented {
        let chosen = chosen.expect("the default always has a row");
        options.push((chosen.to_string().into(), true, Some(chosen.to_string())));
    }
    options
}

/// An on/off setting: one word that flips it.
pub fn toggle(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    checked: bool,
    change: impl Fn(bool, &mut App) + 'static,
) -> Row {
    Row::new(
        title,
        detail,
        Vec::new(),
        Control::Toggle {
            id,
            checked,
            change: Rc::new(change),
        },
    )
}

/// A size stepper: `‹ 14px ›`. An end with nowhere to go takes no press.
pub fn stepper(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    value: SharedString,
    can_down: bool,
    can_up: bool,
    step: impl Fn(i32, &mut App) + 'static,
) -> Row {
    Row::new(
        title,
        detail,
        vec![value.clone()],
        Control::Stepper {
            id,
            value,
            can_down,
            can_up,
            step: Rc::new(step),
        },
    )
}

/// A file the operator may choose in place of a built-in (a sound): its
/// name, or `built_in`'s words, then a token per `acts` (`Play`,
/// `Choose…`, `Use built-in`). Searchable by the name.
pub fn file_choice(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    file: Option<&std::path::Path>,
    built_in: &'static str,
    acts: Vec<(&'static str, Act)>,
) -> Row {
    let value: SharedString = file
        .and_then(|file| file.file_name())
        .map(|name| name.to_string_lossy().into_owned().into())
        .unwrap_or_else(|| built_in.into());
    Row::new(
        title,
        detail,
        vec![value.clone()],
        Control::FileChoice { id, value, acts },
    )
}

/// A row whose title and hint are the operator's own words (a machine's
/// name and ssh target), offering a token per act. Searchable by both.
pub fn named(
    id: &'static str,
    nth: usize,
    title: SharedString,
    detail: SharedString,
    acts: Vec<(SharedString, Act)>,
) -> Row {
    let words = vec![title.clone(), detail.clone()];
    Row::new(title, detail, words, Control::Named { id, nth, acts })
}

/// A field, and the token that takes what is typed in it.
pub fn entry(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    field: Entity<InputState>,
    label: &'static str,
    act: impl Fn(&mut Window, &mut App) + 'static,
) -> Row {
    Row::new(
        title,
        detail,
        Vec::new(),
        Control::Entry {
            id,
            field,
            label,
            act: Rc::new(act),
        },
    )
}

/// A read-only fact (About): its key at the left, its value at the right.
/// Searchable by both.
pub fn fact(title: &'static str, value: SharedString) -> Row {
    let words = vec![value.clone()];
    Row::new(title, "", words, Control::Fact { value })
}

/// A row that says where something stands (its hint) and, when there is
/// something to do about it, offers the one button that does it. With
/// nothing to do, the row reads its `value` as a fact does.
pub fn action(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    value: SharedString,
    button: Option<SharedString>,
    act: impl Fn(&mut App) + 'static,
) -> Row {
    let words = std::iter::once(value.clone())
        .chain(button.iter().cloned())
        .collect();
    Row::new(
        title,
        detail,
        words,
        Control::Action {
            id,
            value,
            button,
            act: Rc::new(act),
        },
    )
}

/// A path fact: shown from `~` when it lies under the home directory, and
/// copied whole on a click; `copied` once it has been.
pub fn path_fact(
    title: &'static str,
    path: String,
    copied: bool,
    on_copy: impl Fn(&mut App) + 'static,
) -> Row {
    let shown: SharedString = home_relative(&path, std::env::var("HOME").ok().as_deref()).into();
    let words = vec![shown.clone(), path.clone().into()];
    Row::new(
        title,
        "",
        words,
        Control::PathFact {
            path,
            shown,
            copied,
            on_copy: Rc::new(on_copy),
        },
    )
}

/// `path` from `~` when it lies under `home`.
fn home_relative(path: &str, home: Option<&str>) -> String {
    match home.filter(|home| home.len() > 1) {
        Some(home) => match path.strip_prefix(home.trim_end_matches('/')) {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
            _ => path.to_string(),
        },
        None => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_choices_keep_default_alias_and_custom_values_selected() {
        use ferrite_core::{providers::models::fallback, store::Provider};
        for provider in [Provider::Claude, Provider::Codex] {
            let options = model_options(fallback(provider), None);
            assert_eq!(options.iter().filter(|(_, on, _)| *on).count(), 1);
            assert_eq!(options[0], ("CLI default".into(), true, None));
        }
        let options = model_options(fallback(Provider::Claude), Some("claude-sonnet-5-5"));
        let selected: Vec<_> = options.iter().filter(|(_, on, _)| *on).collect();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].2.as_deref(), Some("sonnet"));
        let options = model_options(fallback(Provider::Codex), Some("private-model"));
        assert_eq!(
            options.last().unwrap(),
            &("private-model".into(), true, Some("private-model".into()))
        );
        assert_eq!(options.iter().filter(|(_, on, _)| *on).count(), 1);
    }

    #[test]
    fn pages_step_in_sidebar_order_and_hold_at_the_ends() {
        assert_eq!(PageKey::NewThreads.step(1), PageKey::Permissions);
        assert_eq!(PageKey::Behaviour.step(1), PageKey::Notifications);
        assert_eq!(PageKey::Notifications.step(1), PageKey::Server);
        assert_eq!(PageKey::About.step(1), PageKey::About);
        assert_eq!(PageKey::NewThreads.step(-1), PageKey::NewThreads);
        assert_eq!(PageKey::About.step(-5), PageKey::NewThreads);
        assert_eq!(PageKey::default(), PageKey::NewThreads);
    }

    #[test]
    fn a_path_under_home_reads_from_tilde() {
        let home = Some("/Users/op");
        assert_eq!(
            home_relative("/Users/op/Library/ferrite/threads", home),
            "~/Library/ferrite/threads"
        );
        assert_eq!(home_relative("/Users/op", home), "~");
        assert_eq!(
            home_relative("/Users/operator/x", home),
            "/Users/operator/x"
        );
        assert_eq!(home_relative("/tmp/x", home), "/tmp/x");
        assert_eq!(home_relative("/tmp/x", None), "/tmp/x");
        assert_eq!(home_relative("/x", Some("/")), "/x");
    }
}
