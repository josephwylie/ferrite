//! A retained selection range follows native text paint independently of the live selection.

use super::*;
use std::{cell::RefCell, collections::BTreeMap, rc::Weak};

type Changed = Rc<dyn Fn(Option<Bounds<Pixels>>, &mut Window, &mut App)>;

#[derive(Clone)]
pub struct TextSelectionAnchor(Rc<Anchor>);

#[derive(Clone, Debug)]
pub struct TextSelectionSelector {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub prefix: String,
    pub suffix: String,
}

pub(super) struct Anchor {
    parts: Vec<Rc<RetainedPart>>,
    window: gpui::AnyWindowHandle,
    bounds: Cell<Option<Bounds<Pixels>>>,
    available: Cell<bool>,
    changed: RefCell<Option<Changed>>,
}

pub(crate) struct RetainedPart {
    participant: EntityId,
    available: Cell<bool>,
    member: Option<DocumentMemberBinding>,
    _pin: Option<AnyEntity>,
    pub(crate) range: Cell<TextSelectionDocumentRange>,
    text: RefCell<String>,
    quote: String,
    prefix: String,
    suffix: String,
    sources: RefCell<BTreeMap<usize, SharedString>>,
    rects: RefCell<BTreeMap<(usize, usize), Bounds<Pixels>>>,
}

impl std::fmt::Debug for TextSelectionAnchor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TextSelectionAnchor")
            .field("bounds", &self.bounds())
            .finish()
    }
}

impl TextSelectionAnchor {
    pub fn selector(&self) -> Option<(SharedString, TextSelectionSelector)> {
        if self.0.parts.len() != 1 {
            return None;
        }
        let part = &self.0.parts[0];
        let key = part.member.as_ref()?.key.clone();
        let text = part.text.borrow();
        let range = offsets(&part.sources.borrow(), part.range.get());
        Some((
            key,
            TextSelectionSelector {
                text: part.quote.clone(),
                start: normalize(&text[..range.start]).0.encode_utf16().count(),
                end: normalize(&text[..range.end]).0.encode_utf16().count(),
                prefix: part.prefix.clone(),
                suffix: part.suffix.clone(),
            },
        ))
    }

    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.0.bounds.get()
    }

    pub fn on_change(
        &self,
        changed: impl Fn(Option<Bounds<Pixels>>, &mut Window, &mut App) + 'static,
    ) {
        *self.0.changed.borrow_mut() = Some(Rc::new(changed));
    }
}

impl RetainedPart {
    pub(crate) fn begin_frame(&self) {
        self.sources.borrow_mut().clear();
        self.rects.borrow_mut().clear();
    }

    pub(crate) fn record(
        &self,
        ordinal: usize,
        offset: usize,
        source: SharedString,
        last: Option<Bounds<Pixels>>,
    ) {
        self.sources.borrow_mut().insert(ordinal, source);
        if let Some(last) = last {
            self.rects.borrow_mut().insert((ordinal, offset), last);
        }
    }

    fn repair(&self) -> Option<bool> {
        let sources = self.sources.borrow();
        let text = flatten(&sources);
        if text == *self.text.borrow() {
            return Some(false);
        }
        let selected = resolve(&text, &self.quote, &self.prefix, &self.suffix)?;
        let range = range_at(&sources, selected)?;
        *self.text.borrow_mut() = text;
        let changed = self.range.replace(range) != range;
        Some(changed)
    }
}

fn flatten(sources: &BTreeMap<usize, SharedString>) -> String {
    sources
        .values()
        .map(AsRef::as_ref)
        .collect::<Vec<_>>()
        .join("\n")
}

fn offsets(
    sources: &BTreeMap<usize, SharedString>,
    range: TextSelectionDocumentRange,
) -> Range<usize> {
    let mut at = 0;
    let mut start = 0;
    let mut end = flatten(sources).len();
    for (&ordinal, text) in sources {
        for (key, target) in [(range.start(), &mut start), (range.end(), &mut end)] {
            if let Some(key) = key.filter(|key| (key.value() >> 32) as usize == ordinal) {
                *target = at + (key.value() as u32 as usize).min(text.len());
            }
        }
        at += text.len() + 1;
    }
    start..end
}

fn range_at(
    sources: &BTreeMap<usize, SharedString>,
    selected: Range<usize>,
) -> Option<TextSelectionDocumentRange> {
    let key_at = |offset: usize| {
        let mut at = 0;
        for (&ordinal, text) in sources {
            if offset <= at + text.len() {
                return Some(TextSelectionContentKey::new(
                    ((ordinal as u64) << 32) | (offset - at) as u64,
                ));
            }
            at += text.len() + 1;
        }
        None
    };
    Some(TextSelectionDocumentRange::new(
        Some(key_at(selected.start)?),
        Some(key_at(selected.end)?),
    ))
}

fn normalize(text: &str) -> (String, Vec<(Range<usize>, Range<usize>)>) {
    let mut normalized = String::new();
    let mut spans: Vec<(Range<usize>, Range<usize>)> = Vec::new();
    let mut whitespace = false;
    for (at, ch) in text.char_indices() {
        let blank = ch.is_whitespace();
        if blank && whitespace {
            if let Some((_, raw)) = spans.last_mut() {
                raw.end = at + ch.len_utf8();
            }
            continue;
        }
        let start = normalized.len();
        normalized.push(if blank { ' ' } else { ch });
        spans.push((start..normalized.len(), at..at + ch.len_utf8()));
        whitespace = blank;
    }
    (normalized, spans)
}

fn context(text: &str, before: bool) -> String {
    let text = normalize(text).0;
    let chars = if before {
        text.chars().rev().collect::<Vec<_>>()
    } else {
        text.chars().collect()
    };
    let mut units = 0;
    let mut taken = Vec::new();
    for ch in chars {
        if units + ch.len_utf16() > 32 {
            break;
        }
        units += ch.len_utf16();
        taken.push(ch);
    }
    if before {
        taken.reverse();
    }
    taken.into_iter().collect()
}

fn resolve(text: &str, quote: &str, prefix: &str, suffix: &str) -> Option<Range<usize>> {
    let (text, spans) = normalize(text);
    let quote = normalize(quote).0;
    let prefix = normalize(prefix).0;
    let suffix = normalize(suffix).0;
    let matches = text
        .char_indices()
        .filter_map(|(at, _)| {
            text[at..]
                .starts_with(&quote)
                .then_some(at..at + quote.len())
        })
        .collect::<Vec<_>>();
    let mut contextual = matches.iter().filter(|range| {
        text[..range.start].ends_with(&prefix) && text[range.end..].starts_with(&suffix)
    });
    let found = match (contextual.next(), contextual.next()) {
        (Some(found), None) => found,
        (None, None) if matches.len() == 1 => &matches[0],
        _ => return None,
    };
    let start = spans
        .iter()
        .find(|(normalized, _)| normalized.start == found.start)?
        .1
        .start;
    let end = spans
        .iter()
        .find(|(normalized, _)| normalized.end == found.end)?
        .1
        .end;
    Some(start..end)
}

impl TextSelection {
    pub fn retain_range(window: &mut Window, cx: &mut App) -> Option<TextSelectionAnchor> {
        let state = live_text_selection_state(window, cx)?;
        WindowSelectionState::resolve_content_keys(&state, cx);
        let candidates = state
            .read(cx)
            .participants
            .iter()
            .filter_map(|(id, registered)| {
                let participant = registered.participant.upgrade()?;
                let selected = participant.read(cx);
                let range = selected.snapshot()?.document_range()?;
                let sources = selected.anchor_sources.as_ref()?.clone();
                Some((
                    *id,
                    participant.clone(),
                    registered.document_member.clone(),
                    registered.registration.document_order,
                    range,
                    sources,
                ))
            })
            .collect::<Vec<_>>();
        let mut parts = Vec::new();
        for (id, participant, member, order, range, sources) in candidates {
            let sources = sources(cx);
            let text = flatten(&sources);
            let selected = offsets(&sources, range);
            let Some(quote) = text.get(selected.clone()).filter(|quote| !quote.is_empty()) else {
                continue;
            };
            let part = Rc::new(RetainedPart {
                participant: id,
                available: Cell::new(true),
                _pin: member
                    .as_ref()
                    .and_then(|member| member.pin_native_state(cx)),
                member,
                range: Cell::new(range),
                quote: quote.to_string(),
                prefix: context(&text[..selected.start], true),
                suffix: context(&text[selected.end..], false),
                text: RefCell::new(text),
                sources: RefCell::new(sources),
                rects: RefCell::new(BTreeMap::new()),
            });
            participant.update(cx, |state, _| {
                state.retained.retain(|part| part.strong_count() > 0);
                state.retained.push(Rc::downgrade(&part));
            });
            parts.push((order, part));
        }
        parts.sort_by_key(|(order, _)| *order);
        if parts.is_empty() {
            return None;
        }
        let initial = TextSelection::painted_bounds(window, cx)
            .into_iter()
            .max_by(|a, b| {
                a.bottom()
                    .partial_cmp(&b.bottom())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        let anchor = TextSelectionAnchor(Rc::new(Anchor {
            parts: parts.into_iter().map(|(_, part)| part).collect(),
            window: window.window_handle(),
            bounds: Cell::new(initial),
            available: Cell::new(true),
            changed: RefCell::new(None),
        }));
        state.update(cx, |state, _| state.retained.push(Rc::downgrade(&anchor.0)));
        Some(anchor)
    }
}

impl TextSelectionDocument {
    pub fn anchor_text(
        &self,
        key: &str,
        selector: &TextSelectionSelector,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<TextSelectionAnchor> {
        let (participant, member, sources) = {
            let document = self.0.read(cx);
            let index = *document.member_indices.get(key)?;
            let member = &document.members.get(index)?.1;
            let participant = member.participant.as_ref()?.upgrade()?;
            let selected = participant.read(cx);
            (
                participant.clone(),
                selected.document_member.clone(),
                selected.anchor_sources.clone()?,
            )
        };
        let state = live_text_selection_state(window, cx)?;
        if !state
            .read(cx)
            .participants
            .contains_key(&participant.entity_id())
        {
            return None;
        }
        let sources = sources(cx);
        let text = flatten(&sources);
        let selected = resolve(&text, &selector.text, &selector.prefix, &selector.suffix)?;
        let range = range_at(&sources, selected)?;
        let part = Rc::new(RetainedPart {
            participant: participant.entity_id(),
            available: Cell::new(true),
            _pin: member
                .as_ref()
                .and_then(|member| member.pin_native_state(cx)),
            member,
            range: Cell::new(range),
            text: RefCell::new(text),
            quote: selector.text.clone(),
            prefix: selector.prefix.clone(),
            suffix: selector.suffix.clone(),
            sources: RefCell::new(sources),
            rects: RefCell::new(BTreeMap::new()),
        });
        participant.update(cx, |state, _| {
            state.retained.retain(|part| part.strong_count() > 0);
            state.retained.push(Rc::downgrade(&part));
        });
        let anchor = TextSelectionAnchor(Rc::new(Anchor {
            parts: vec![part],
            window: window.window_handle(),
            bounds: Cell::new(None),
            available: Cell::new(true),
            changed: RefCell::new(None),
        }));
        state.update(cx, |state, _| state.retained.push(Rc::downgrade(&anchor.0)));
        window.refresh();
        Some(anchor)
    }
}

impl TextSelectionHandle {
    pub(crate) fn retained_ranges(&self, cx: &App) -> Vec<Rc<RetainedPart>> {
        self.0
            .read(cx)
            .retained
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|part| part.available.get())
            .collect()
    }

    pub(crate) fn anchor_sources_with(
        &self,
        sources: impl Fn(&App) -> BTreeMap<usize, SharedString> + 'static,
        cx: &mut App,
    ) {
        self.0
            .update(cx, |state, _| state.anchor_sources = Some(Rc::new(sources)));
    }
}

impl WindowSelectionState {
    pub(super) fn finish_anchors(&mut self, cx: &mut App) {
        self.retained.retain(|anchor| anchor.strong_count() > 0);
        for anchor in self.retained.iter().filter_map(Weak::upgrade) {
            if !anchor.available.get() {
                continue;
            }
            let mut valid = true;
            let mut repaired = false;
            let mut last = None;
            for part in &anchor.parts {
                let registered = self.participants.get(&part.participant);
                let mounted = registered
                    .is_some_and(|registered| registered.generation == self.frame_generation);
                let member_present = part.member.as_ref().is_none_or(|member| {
                    member.document.upgrade().is_some_and(|document| {
                        document.read(cx).member_indices.contains_key(&member.key)
                    })
                });
                if !mounted || !member_present {
                    valid = false;
                    break;
                }
                match part.repair() {
                    Some(changed) => repaired |= changed,
                    None => {
                        valid = false;
                        break;
                    }
                }
                let bounds = registered.unwrap().registration.bounds;
                if let Some(rect) = part.rects.borrow().last_key_value().map(|(_, rect)| *rect) {
                    let left = rect.left().max(bounds.left()).min(bounds.right());
                    let right = rect.right().min(bounds.right()).max(left);
                    last = Some(Bounds::from_corners(
                        point(left, rect.top()),
                        point(right, rect.bottom()),
                    ));
                }
            }
            if repaired && valid {
                cx.refresh_windows();
                continue;
            }
            let was_available = anchor.available.replace(valid);
            if !valid {
                for part in &anchor.parts {
                    part.available.set(false);
                }
                last = None;
            }
            if valid && last.is_none() {
                continue;
            }
            if anchor.bounds.replace(last) != last || (!valid && was_available) {
                if let Some(changed) = anchor.changed.borrow().clone() {
                    let window = anchor.window;
                    cx.defer(move |cx| {
                        let _ = window.update(cx, |_, window, cx| changed(last, window, cx));
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_repair_uses_context_and_rejects_ambiguous_duplicates() {
        assert_eq!(
            resolve("first same second same end", "same", "second ", " end"),
            Some(18..22)
        );
        assert_eq!(resolve("same same", "same", "", ""), None);
        assert_eq!(
            resolve("inserted unique text", "unique", "old ", " context"),
            Some(9..15)
        );
        assert_eq!(resolve("changed", "gone", "", ""), None);
        assert_eq!(resolve("aaa", "aa", "", ""), None);
        assert_eq!(
            resolve(
                "before\n  selected\t text after",
                "selected text",
                "before ",
                " after"
            ),
            Some(9..23)
        );
    }

    #[test]
    fn multiline_ranges_keep_native_ordinals_and_utf8_offsets() {
        let sources = BTreeMap::from([(0, "aé repeated".into()), (1, "last repeated".into())]);
        let range = range_at(&sources, 3..19).unwrap();
        assert_eq!(offsets(&sources, range), 3..19);
        assert_eq!(range.end().unwrap().value() >> 32, 1);
    }
}
