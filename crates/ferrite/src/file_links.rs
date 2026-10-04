//! Resolve transcript destinations once, then hand the OS a real file URL;
//! and the transcript's path targets — every path the transcript names
//! (a tool call's argument, a prose path, a Markdown file link, the banner's
//! checkout, an inline file chip) underlines under the pointer, opens the
//! hover card (`hover_card`), and ⌘-click opens it in a reader pane beside
//! the Thread (`TranscriptEvent::OpenReader` → `CockpitView::open_beside`).
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};

use gpui::{div, prelude::*, px, AnyElement, App, Bounds, Pixels, SharedString, Window};

use crate::theme;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLink {
    pub path: PathBuf,
    pub location: Option<String>,
}

impl FileLink {
    pub fn resolve(destination: &str, cwd: Option<&Path>) -> Option<Self> {
        let destination = destination.trim();
        if destination.is_empty() || destination.starts_with('#') {
            return None;
        }
        let windows_drive = destination.as_bytes().get(1) == Some(&b':')
            && destination
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic);
        // A relative source reference (main.rs:12) otherwise parses as a URL scheme.
        let (raw, location) = split_location(destination);
        let source_reference = location.is_some() && Path::new(raw).extension().is_some();
        if !windows_drive
            && !source_reference
            && url::Url::parse(destination).is_ok_and(|url| url.scheme() != "file")
        {
            return None;
        }
        if url::Url::parse(raw).is_ok_and(|url| url.scheme() != "file") && !windows_drive {
            return None;
        }
        // File URLs carry escaping and authority; ordinary paths do not.
        let path = if raw.starts_with("file:") {
            url::Url::parse(raw).ok()?.to_file_path().ok()?
        } else {
            let decoded = percent_encoding::percent_decode_str(raw)
                .decode_utf8()
                .unwrap_or(std::borrow::Cow::Borrowed(raw));
            if let Some(rest) = decoded.strip_prefix("~/") {
                PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?)
                    .join(rest)
            } else {
                let path = PathBuf::from(decoded.as_ref());
                if path.is_absolute() {
                    path
                } else {
                    cwd?.join(path)
                }
            }
        };
        Some(Self {
            path,
            location: location.map(str::to_owned),
        })
    }

    pub fn url(&self) -> Option<String> {
        url::Url::from_file_path(&self.path).ok().map(Into::into)
    }

    pub fn open(&self, window: &mut gpui::Window, cx: &mut gpui::App) {
        use gpui::component::{notification::Notification, WindowExt};
        if !self.path.exists() {
            window.push_notification(
                Notification::error(format!("File not found: {}", self.path.display())),
                cx,
            );
            return;
        }
        if let Some(url) = self.url() {
            cx.open_url(&url);
        }
    }
}

fn split_location(value: &str) -> (&str, Option<&str>) {
    // Source references use :line[:column] or GitHub-style #Lline[-Lline].
    if let Some((path, fragment)) = value.rsplit_once('#') {
        if fragment.strip_prefix('L').is_some_and(|line| {
            !line.is_empty()
                && line
                    .split("-L")
                    .all(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
        }) {
            return (path, Some(fragment));
        }
    }
    let mut end = value.len();
    for _ in 0..2 {
        let Some((path, number)) = value[..end].rsplit_once(':') else {
            break;
        };
        if number.is_empty() || !number.bytes().all(|c| c.is_ascii_digit()) {
            break;
        }
        end = path.len();
    }
    if end < value.len() {
        (&value[..end], Some(&value[end + 1..]))
    } else {
        (value, None)
    }
}

/// The line a source reference names: `12` of `12:3` or of `L12-L15`.
pub(crate) fn location_line(location: &str) -> Option<u32> {
    let location = location.strip_prefix('L').unwrap_or(location);
    location
        .split([':', '-'])
        .next()
        .and_then(|line| line.parse().ok())
        .filter(|line| *line > 0)
}

// ------------------------------------------------------------ path targets

/// A path the transcript names, as a hit target: where it resolves, the
/// line it points at, how the hover card's head names it, and the
/// transcript (its namespace) that hosts the card and hears ⌘-click.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PathTarget {
    pub path: PathBuf,
    pub line: Option<u32>,
    pub shown: Option<SharedString>,
    pub scope: Option<SharedString>,
}

impl PathTarget {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            line: None,
            shown: None,
            scope: None,
        }
    }

    pub(crate) fn at_line(mut self, line: Option<u32>) -> Self {
        self.line = line;
        self
    }

    pub(crate) fn shown(mut self, shown: impl Into<SharedString>) -> Self {
        self.shown = Some(shown.into());
        self
    }

    pub(crate) fn in_scope(mut self, scope: impl Into<SharedString>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    /// How the hover card's head names it: as the transcript did, else the
    /// path itself.
    pub(crate) fn display(&self) -> SharedString {
        self.shown
            .clone()
            .unwrap_or_else(|| SharedString::from(self.path.display().to_string()))
    }

    /// A stable key for this target's hover fade and element ids.
    fn key(&self, salt: &str) -> SharedString {
        SharedString::from(format!(
            "path-{salt}-{}-{}",
            self.scope.as_deref().unwrap_or(""),
            self.path.display()
        ))
    }
}

type OpenReader = Rc<dyn Fn(PathBuf, Option<u32>, &mut Window, &mut App)>;

/// Where ⌘-click on a target goes, by the transcript namespace hosting it,
/// and where each target last laid out (a scene's `preview_path` anchors on
/// it). Main-thread state: path elements carry only their scope key, so a
/// Markdown link renderer (which must be `Send`) can build them.
#[derive(Default)]
pub(crate) struct PathScopes {
    open: HashMap<SharedString, OpenReader>,
    laid_out: HashMap<(SharedString, PathBuf), Bounds<Pixels>>,
}

impl gpui::Global for PathScopes {}

/// Route ⌘-click on `scope`'s targets to `open` (the transcript's
/// `TranscriptEvent::OpenReader`).
pub(crate) fn register_scope(scope: SharedString, open: OpenReader, cx: &mut App) {
    cx.default_global::<PathScopes>().open.insert(scope, open);
}

/// Open a target in a reader beside its Thread (its scope's handler), or
/// with the system where no transcript hosts it.
pub(crate) fn open_target(target: &PathTarget, window: &mut Window, cx: &mut App) {
    let handler = target.scope.as_ref().and_then(|scope| {
        cx.try_global::<PathScopes>()
            .and_then(|scopes| scopes.open.get(scope).cloned())
    });
    match handler {
        Some(open) => open(target.path.clone(), target.line, window, cx),
        None => FileLink {
            path: target.path.clone(),
            location: target.line.map(|line| line.to_string()),
        }
        .open(window, cx),
    }
}

/// A transcript renders `scope` anew: the first target of each path laid
/// out from here (the topmost) is the one `laid_out` names.
pub(crate) fn begin_layout(scope: &str, cx: &mut App) {
    if let Some(scopes) = cx.try_global::<PathScopes>() {
        if !scopes
            .laid_out
            .keys()
            .any(|(owner, _)| owner.as_ref() == scope)
        {
            return;
        }
    } else {
        return;
    }
    cx.default_global::<PathScopes>()
        .laid_out
        .retain(|(owner, _), _| owner.as_ref() != scope);
}

/// Where a target naming `path` in `scope` last laid out: the exact path,
/// else one that names the same file by a path suffix.
#[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
pub(crate) fn laid_out(scope: &str, path: &Path, cx: &App) -> Option<Bounds<Pixels>> {
    let scopes = cx.try_global::<PathScopes>()?;
    scopes
        .laid_out
        .iter()
        .filter(|((owner, _), _)| owner.as_ref() == scope)
        .find(|((_, at), _)| at == path)
        .or_else(|| {
            scopes
                .laid_out
                .iter()
                .filter(|((owner, _), _)| owner.as_ref() == scope)
                .find(|((_, at), _)| same_file(at, path))
        })
        .map(|(_, bounds)| *bounds)
}

/// Whether two paths name one file: equal, or one is the other's tail at a
/// component boundary.
pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || (a.is_absolute() && !b.is_absolute() && a.ends_with(b))
        || (b.is_absolute() && !a.is_absolute() && b.ends_with(a))
}

thread_local! {
    /// A path underline mid-fade was drawn this frame: the transcript keeps
    /// rendering until it settles (`take_fading`).
    static PATH_FADING: Cell<bool> = const { Cell::new(false) };
}

/// Whether an underline was drawn mid-fade since the last ask.
pub(crate) fn take_fading() -> bool {
    PATH_FADING.with(|fading| fading.replace(false))
}

/// Something else mid-fade was drawn this frame (the minimap's band, a
/// Markdown link's underline): the transcript keeps rendering until it
/// settles, on the same clock as the path underlines.
pub(crate) fn mark_fading() {
    PATH_FADING.with(|fading| fading.set(true));
}

/// The underline's top in a line box `line` tall at text size `size`:
/// `PATH_UNDERLINE_OFFSET` below the face's baseline.
fn underline_top(size: f32, line: f32) -> f32 {
    let baseline = (line - size * 1.15) / 2.0 + size * 0.9;
    (baseline + theme::PATH_UNDERLINE_OFFSET).round()
}

/// The hover underline under a target: `PATH_UNDERLINE_W` in `ink`, faded
/// in by the pointer over `PATH_UNDERLINE_FADE_MS` (the hover blend), never
/// laid out.
fn underline(key: &str, ink: u32, size: f32, line: f32) -> gpui::Div {
    let shown = crate::motion::hover_t(key);
    if shown > 0. && shown < 1. {
        PATH_FADING.with(|fading| fading.set(true));
    }
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(underline_top(size, line)))
        .h(px(theme::PATH_UNDERLINE_W))
        .bg(gpui::rgb(ink))
        .opacity(shown)
}

/// Wire `element` as a path target: the pointer cursor, the hover card while
/// the pointer is on it, ⌘-click to the reader, and its laid-out box kept
/// for `preview_path`.
fn wire_target(
    element: gpui::Stateful<gpui::Div>,
    target: PathTarget,
    fade: SharedString,
) -> gpui::Stateful<gpui::Div> {
    let bounds: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::default();
    let measured = bounds.clone();
    let recorded = target.clone();
    let hovered = target.clone();
    let clicked = target;
    let fade = crate::motion::hover_listener(fade);
    crate::components::on_bounds(element, move |laid, _, cx| {
        measured.set(Some(laid));
        if let Some(scope) = recorded.scope.clone() {
            cx.default_global::<PathScopes>()
                .laid_out
                .entry((scope, recorded.path.clone()))
                .or_insert(laid);
            crate::hover_card::laid_out(&recorded, laid, cx);
        }
    })
    .cursor_pointer()
    .on_hover(move |over, window, cx| {
        fade(over, window, cx);
        match (*over, bounds.get()) {
            (true, Some(anchor)) => crate::hover_card::enter(&hovered, anchor, window, cx),
            (false, _) => crate::hover_card::leave(&hovered, window, cx),
            _ => {}
        }
    })
    .on_click(move |event, window, cx| {
        if event.modifiers().secondary() {
            cx.stop_propagation();
            gpui::base::TextSelection::end(window, cx);
            open_target(&clicked, window, cx);
        }
    })
}

/// A path in its own run (`path_text`, the Phase-0 seam the banner and the
/// empty board share): the label in `PATH_INK`, underlined under the
/// pointer, the hover card, ⌘-click to a reader. It inherits the face and
/// size it sits in.
pub(crate) fn path_text(label: SharedString, target: PathTarget) -> AnyElement {
    let key = target.key("text");
    let fade = key.clone();
    PathText {
        label,
        target,
        key,
        fade,
    }
    .into_any_element()
}

#[derive(IntoElement)]
struct PathText {
    label: SharedString,
    target: PathTarget,
    key: SharedString,
    fade: SharedString,
}

impl gpui::RenderOnce for PathText {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let style = window.text_style();
        let size = f32::from(style.font_size.to_pixels(window.rem_size()));
        let line = f32::from(style.line_height_in_pixels(window.rem_size()));
        let element = div()
            .id(self.key.clone())
            .relative()
            .flex_shrink_0()
            .whitespace_nowrap()
            .text_color(gpui::rgb(theme::PATH_INK))
            .child(self.label)
            .child(underline(&self.fade, theme::PATH_INK, size, line));
        wire_target(element, self.target, self.fade)
    }
}

/// A path target laid over a run of monospace text the selection owns (a
/// tool call's argument, the banner's checkout): `start` cells in, `width`
/// cells wide, one line tall at reading size `size`. It draws only the
/// hover underline; the text under it stays one selectable run.
pub(crate) fn path_overlay(
    target: PathTarget,
    start: usize,
    width: usize,
    size: f32,
    line: f32,
) -> AnyElement {
    let cell = theme::tx_cell(size);
    let key = target.key(&format!("overlay-{start}"));
    let element = div()
        .id(key.clone())
        .absolute()
        .top_0()
        .left(px(start as f32 * cell))
        .w(px(width as f32 * cell))
        .h(px(line))
        .child(underline(&key, theme::PATH_INK, size, line));
    wire_target(element, target, key).into_any_element()
}

/// Wire an inline file chip (`attachments::inline_file`) as a path target
/// with its underline: `ink` is the name's own.
pub(crate) fn wire_inline(
    element: gpui::Stateful<gpui::Div>,
    target: PathTarget,
    size: f32,
    line: f32,
) -> gpui::Stateful<gpui::Div> {
    let key = target.key("inline");
    wire_target(
        element.child(underline(&key, theme::PATH_INK, size, line)),
        target,
        key,
    )
}

// ------------------------------------------------- prose targets and refs

/// What an answer's Markdown can resolve against: the Thread's checkout and
/// the paths its own tool calls named (`nav.rs` meets
/// `crates/ferrite/src/nav.rs`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TargetContext {
    pub workspace: Option<PathBuf>,
    pub known: Vec<String>,
}

thread_local! {
    static EXISTS: RefCell<HashMap<PathBuf, bool>> = RefCell::new(HashMap::new());
}

/// Whether a file exists, asked once per path (the answer is drawn every
/// frame its row is on screen).
fn is_file(path: &Path) -> bool {
    EXISTS.with(|cache| {
        *cache
            .borrow_mut()
            .entry(path.to_path_buf())
            .or_insert_with(|| path.is_file())
    })
}

impl TargetContext {
    /// Where inline code naming a file resolves: a path the Thread's own
    /// calls named (whole, or by its tail), else a file that exists in the
    /// checkout. `None` for anything that is not a file of this Thread's.
    pub(crate) fn resolve(&self, text: &str) -> Option<PathBuf> {
        let text = text.trim();
        let (raw, _) = split_location(text);
        let looks_like_file = !raw.is_empty()
            && !raw.contains(char::is_whitespace)
            && Path::new(raw).extension().is_some()
            && !raw.contains("::")
            && !raw.starts_with('-');
        if !looks_like_file {
            return None;
        }
        if let Some(known) = self.known.iter().find(|known| {
            known.as_str() == raw
                || known.ends_with(&format!("/{raw}"))
                || raw.ends_with(&format!("/{}", known.trim_start_matches("./")))
        }) {
            let path = PathBuf::from(known);
            return Some(match (&self.workspace, path.is_absolute()) {
                (Some(root), false) => root.join(path),
                _ => path,
            });
        }
        let root = self.workspace.as_ref()?;
        let path = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            root.join(raw)
        };
        (path.starts_with(root) && is_file(&path)).then_some(path)
    }
}

/// An answer's Markdown with its path targets and issue refs made links,
/// so the native renderer draws them as hit targets: inline code naming a
/// file of this Thread's (`TargetContext::resolve`) becomes a file link
/// whose text is the code, and `#212` becomes an `issue:212` link (opened on
/// the checkout's origin remote). Fenced code, inline code that is not a
/// file, and existing links are left as written; the copy reads the same
/// words either way.
pub(crate) fn link_targets(source: &str, context: &TargetContext) -> String {
    let mut out = String::with_capacity(source.len() + 64);
    let mut fence: Option<&str> = None;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        match (fence, marker) {
            (None, Some(open)) => {
                fence = Some(open);
                out.push_str(line);
                continue;
            }
            (Some(open), Some(close)) if open == close => {
                fence = None;
                out.push_str(line);
                continue;
            }
            (Some(_), _) => {
                out.push_str(line);
                continue;
            }
            (None, None) => {}
        }
        // An indented code block is code too.
        if line.starts_with("    ") || line.starts_with('\t') {
            out.push_str(line);
            continue;
        }
        link_line(line, context, &mut out);
    }
    out
}

/// One prose line through `link_targets`.
fn link_line(line: &str, context: &TargetContext, out: &mut String) {
    let bytes = line.as_bytes();
    let mut at = 0;
    let mut plain = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'`' => {
                let ticks = line[at..].bytes().take_while(|b| *b == b'`').count();
                let fence = &line[at..at + ticks];
                let Some(close) = line[at + ticks..].find(fence) else {
                    at += ticks;
                    continue;
                };
                let inner = &line[at + ticks..at + ticks + close];
                let end = at + ticks + close + ticks;
                if let Some(path) = (ticks == 1).then(|| context.resolve(inner)).flatten() {
                    out.push_str(&line[plain..at]);
                    let (_, location) = split_location(inner.trim());
                    out.push('[');
                    out.push_str(&escape_link_text(inner.trim()));
                    out.push_str("](<");
                    out.push_str(&path.display().to_string());
                    if let Some(location) = location {
                        out.push(':');
                        out.push_str(location);
                    }
                    out.push_str(">)");
                    plain = end;
                }
                at = end;
            }
            // An existing link (or image) is left whole.
            b'[' => {
                let Some(close) = line[at..].find("](") else {
                    at += 1;
                    continue;
                };
                let Some(end) = line[at + close..].find(')') else {
                    at += 1;
                    continue;
                };
                at += close + end + 1;
            }
            b'#' if issue_ref_starts(line, at) => {
                let digits = line[at + 1..]
                    .bytes()
                    .take_while(u8::is_ascii_digit)
                    .count();
                let end = at + 1 + digits;
                out.push_str(&line[plain..at]);
                out.push_str(&format!("[#{0}](issue:{0})", &line[at + 1..end]));
                plain = end;
                at = end;
            }
            _ => at += line[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    out.push_str(&line[plain..]);
}

/// `#212` at `at`: a `#`, one to six digits, standing alone (after the line
/// start, whitespace or an opening bracket; before a non-word character).
fn issue_ref_starts(line: &str, at: usize) -> bool {
    let before = line[..at].chars().next_back();
    let alone_before = before.is_none_or(|ch| ch.is_whitespace() || matches!(ch, '(' | '['));
    let digits = line[at + 1..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let after = line[at + 1 + digits..].chars().next();
    let alone_after = after.is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
    alone_before && (1..=6).contains(&digits) && alone_after
}

fn escape_link_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '[' | ']' | '\\' | '*' | '_' | '`') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

thread_local! {
    static ORIGINS: RefCell<HashMap<PathBuf, Option<String>>> = RefCell::new(HashMap::new());
}

/// The web page of issue `number` on the checkout's `origin` remote
/// (`git@github.com:owner/repo.git` → `https://github.com/owner/repo/issues/212`),
/// read once per checkout from its git config.
pub(crate) fn issue_url(workspace: &Path, number: &str) -> Option<String> {
    let base = ORIGINS.with(|cache| {
        cache
            .borrow_mut()
            .entry(workspace.to_path_buf())
            .or_insert_with(|| origin_web_base(workspace))
            .clone()
    })?;
    Some(format!("{base}/issues/{number}"))
}

/// The origin remote's web base for the git checkout holding `dir`.
fn origin_web_base(dir: &Path) -> Option<String> {
    let dot_git = dir
        .ancestors()
        .map(|at| at.join(".git"))
        .find(|at| at.exists())?;
    let git_dir = if dot_git.is_file() {
        // A worktree: `gitdir: <path>`, its config in the common dir.
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let gitdir = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
        let gitdir = if gitdir.is_absolute() {
            gitdir
        } else {
            dot_git.parent()?.join(gitdir)
        };
        match std::fs::read_to_string(gitdir.join("commondir")) {
            Ok(common) => gitdir.join(common.trim()),
            Err(_) => gitdir,
        }
    } else {
        dot_git
    };
    let config = std::fs::read_to_string(git_dir.join("config")).ok()?;
    let mut in_origin = false;
    let mut url = None;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
            continue;
        }
        if in_origin {
            if let Some(value) = line.strip_prefix("url").map(str::trim_start) {
                if let Some(value) = value.strip_prefix('=') {
                    url = Some(value.trim().to_string());
                    break;
                }
            }
        }
    }
    web_base(&url?)
}

/// A remote URL as its web page: `git@host:owner/repo.git`,
/// `ssh://git@host/owner/repo.git` and `https://host/owner/repo.git` all
/// read `https://host/owner/repo`.
pub(crate) fn web_base(remote: &str) -> Option<String> {
    let remote = remote.trim().trim_end_matches('/');
    let remote = remote.strip_suffix(".git").unwrap_or(remote);
    let (host, path) = if let Some(rest) = remote
        .strip_prefix("https://")
        .or_else(|| remote.strip_prefix("http://"))
    {
        rest.split_once('/')?
    } else if let Some(rest) = remote.strip_prefix("ssh://") {
        let rest = rest.split_once('@').map_or(rest, |(_, rest)| rest);
        let (host, path) = rest.split_once('/')?;
        (host.split(':').next().unwrap_or(host), path)
    } else {
        let rest = remote.split_once('@').map_or(remote, |(_, rest)| rest);
        rest.split_once(':')?
    };
    let host = host.rsplit('@').next().unwrap_or(host);
    (!host.is_empty() && path.contains('/')).then(|| format!("https://{host}/{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_are_encoded_and_source_locations_do_not_reach_the_os() {
        let dir = std::env::temp_dir();
        for (name, suffix) in [
            ("report.md", ":12:3"),
            ("report.md", "#L12-L15"),
            ("report one.md", ":12"),
            ("résumé #1.md", ""),
        ] {
            let path = dir.join(name);
            let expected = url::Url::from_file_path(&path).unwrap().to_string();
            for input in [
                format!("{}{suffix}", path.display()),
                format!("{expected}{suffix}"),
            ] {
                assert_eq!(
                    FileLink::resolve(&input, None).unwrap().url().as_deref(),
                    Some(expected.as_str()),
                    "{input}"
                );
            }
        }
        assert_eq!(
            FileLink::resolve("report%20one.md", Some(&dir))
                .unwrap()
                .path,
            dir.join("report one.md")
        );
    }
    #[test]
    fn relative_paths_use_the_threads_checkout() {
        let checkout = std::env::temp_dir().join("project/.worktrees/task");
        let cwd = checkout.as_path();
        assert_eq!(
            FileLink::resolve("docs/report.md", Some(cwd)).unwrap().path,
            cwd.join("docs/report.md")
        );
        assert!(FileLink::resolve("report.md", None).is_none());
        for url in [
            "https://example.com/a.pdf",
            "https://example.com:123",
            "mailto:a@example.com",
            "mailto:123",
            "#heading",
        ] {
            assert!(FileLink::resolve(url, Some(cwd)).is_none(), "{url}");
        }
    }
}
