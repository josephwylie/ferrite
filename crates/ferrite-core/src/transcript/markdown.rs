//! An answer's Markdown, folded a section at a time: a paragraph, heading,
//! bullet or fenced block, each closed by the first line that cannot join
//! it. The section still open folds as it streams, each delta costing what
//! it adds rather than a re-read of the section so far.

use super::{Block, BlockId, Body, Span, Style};

/// The answer section still streaming: its raw Markdown, the Block it folds
/// into, and how far both have been read. The one writer of that Block.
#[derive(Default)]
pub(super) struct OpenSection {
    block: Option<BlockId>,
    source: String,
    scan: Scan,
    /// The prose already folded for good, while `block` keeps it.
    settled: Option<Settled>,
}

/// What a delta did to the Blocks: those it changed, and those whose section
/// it closed, which no longer change.
#[derive(Default)]
pub(super) struct Grown {
    pub(super) dirty: Vec<BlockId>,
    pub(super) closed: Vec<BlockId>,
}

/// How far the look for the open section's end has got: the start of its
/// unfinished line, and how far into the source it has found no newline.
#[derive(Default)]
struct Scan {
    line: usize,
    seen: usize,
}

/// A prose section's leading spans that no later text can restyle: what
/// the open Block's spans keep between deltas, and where in the section's
/// text the rest starts.
struct Settled {
    shape: Shape,
    /// Where the section's inline text starts in its source, past any
    /// indent and marker.
    start: usize,
    /// Bytes of the inline text settled.
    bytes: usize,
    /// Spans settled, the last of them Plain and cut short at `last` bytes:
    /// the rest of its run reads again with the tail.
    spans: usize,
    last: usize,
}

impl OpenSection {
    /// The open section's Block, once it has content.
    pub(super) fn block(&self) -> Option<BlockId> {
        self.block
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Fold `text` into `blocks`: first every section a line has closed,
    /// then the one still open, which `mint` gives a Block on its first
    /// content. A section that closed unchanged is in `closed` but not
    /// `dirty`, unless its body or retained Markdown moved.
    pub(super) fn grow(
        &mut self,
        text: &str,
        blocks: &mut Vec<Block>,
        mut mint: impl FnMut() -> BlockId,
    ) -> Grown {
        let mut grown = Grown::default();
        if text.is_empty() {
            return grown;
        }
        self.source.push_str(text);
        while let Some(section) = self.take_complete() {
            if let Some(body) = parse_section(&section) {
                let changed = write(&mut self.block, blocks, body, &section, &mut mint);
                grown.dirty.extend(changed);
                grown.closed.extend(changed.or(self.block));
            } else if let Some(block) = blocks.last_mut() {
                // Blank lines carry Markdown structure even though they do
                // not have a visual block of their own.
                if let Some(markdown) = &mut block.markdown {
                    markdown.push_str(&section);
                    grown.dirty.push(block.id);
                }
            }
            self.block = None;
        }
        if let Some(block) = open(self.block, blocks) {
            let id = block.id;
            if self.grow_into(block) {
                grown.dirty.push(id);
                return grown;
            }
        }
        if let Some(body) = parse_section(&self.source) {
            grown.dirty.extend(write(
                &mut self.block,
                blocks,
                body,
                &self.source,
                &mut mint,
            ));
            self.unsettle();
        }
        grown
    }

    /// Take the leading section once a line closes it. Its Block, if it has
    /// one yet, stays `block` until the caller settles it.
    fn take_complete(&mut self) -> Option<String> {
        let used = complete_section(&self.source, &mut self.scan)?;
        let section = self.source[..used].to_string();
        self.source.replace_range(..used, "");
        self.scan = Scan::default();
        self.settled = None;
        Some(section)
    }

    /// Settle none of the open section's prose, after it was folded whole:
    /// the next `grow_into` reads it from its start.
    fn unsettle(&mut self) {
        self.settled = prose(&self.source).map(|(shape, text)| Settled {
            shape,
            start: offset(&self.source, text),
            bytes: 0,
            spans: 0,
            last: 0,
        });
    }

    /// Fold the text pushed since `block` last matched the section into it,
    /// re-reading only the prose a new delta can still restyle. False, with
    /// `block` untouched, when the section has to fold whole: it is code,
    /// or its shape moved under it (`-` became a bullet).
    fn grow_into(&mut self, block: &mut Block) -> bool {
        let Some(settled) = &mut self.settled else {
            return false;
        };
        if self.source.starts_with("```") {
            return false;
        }
        let Some((shape, text)) = prose(&self.source) else {
            return false;
        };
        if shape != settled.shape || offset(&self.source, text) != settled.start {
            return false;
        }
        let spans = match (&mut block.body, shape) {
            (Body::Paragraph { spans }, Shape::Paragraph)
            | (Body::Bullet { spans }, Shape::Bullet) => spans,
            (Body::Heading { level, spans }, Shape::Heading(shaped)) if *level == shaped => spans,
            _ => return false,
        };
        let Some(markdown) = &mut block.markdown else {
            return false;
        };
        markdown.push_str(&self.source[markdown.len()..]);

        let tail = Inline::read(&text[settled.bytes..]);
        spans.truncate(settled.spans);
        let merged = settled.spans > 0;
        let mut read = tail.spans.iter();
        if merged {
            let last = &mut spans[settled.spans - 1].text;
            last.truncate(settled.last);
            if let Some((style, run)) = read.next() {
                debug_assert_eq!(*style, Style::Plain, "a settled cut lies inside plain text");
                last.push_str(run);
            }
        }
        spans.extend(read.map(|&(style, text)| Span {
            text: text.to_string(),
            style,
        }));

        if let Some((index, cut)) = tail.cut() {
            let base = settled.spans - usize::from(merged);
            settled.last = if merged && index == 0 {
                settled.last + cut
            } else {
                cut - offset(tail.text, tail.spans[index].1)
            };
            settled.spans = base + index + 1;
            settled.bytes += cut;
        }
        true
    }
}

/// The open Block: the transcript's tail, since anything pushed after it
/// closes the section. None once it has fallen away.
fn open(id: Option<BlockId>, blocks: &mut [Block]) -> Option<&mut Block> {
    blocks.last_mut().filter(|block| Some(block.id) == id)
}

/// Write a folded body into the open Block, creating it on first content.
/// Reports changes to either the folded body or the original Markdown,
/// since whitespace can change rich-text structure without changing plain
/// text.
fn write(
    open_id: &mut Option<BlockId>,
    blocks: &mut Vec<Block>,
    body: Body,
    source: &str,
    mint: &mut impl FnMut() -> BlockId,
) -> Option<BlockId> {
    if open_id.is_some() {
        let block = open(*open_id, blocks)?;
        if block.body == body && block.markdown.as_deref() == Some(source) {
            return None;
        }
        block.body = body;
        block.markdown = Some(source.to_string());
        return Some(block.id);
    }
    let id = mint();
    let markdown_run = blocks
        .last()
        .and_then(|block| block.markdown_run)
        .unwrap_or(id);
    blocks.push(Block {
        id,
        body,
        markdown: Some(source.to_string()),
        markdown_run: Some(markdown_run),
        sent_at: None,
    });
    *open_id = Some(id);
    Some(id)
}

/// What a prose section folds into, once its spans are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Paragraph,
    Heading(u8),
    Bullet,
}

impl Shape {
    fn body(self, spans: Vec<Span>) -> Body {
        match self {
            Shape::Paragraph => Body::Paragraph { spans },
            Shape::Heading(level) => Body::Heading { level, spans },
            Shape::Bullet => Body::Bullet { spans },
        }
    }
}

/// Where `part`, a slice of `whole`, starts in it.
fn offset(whole: &str, part: &str) -> usize {
    part.as_ptr() as usize - whole.as_ptr() as usize
}

/// How many bytes of `source` form a section that can no longer grow — or
/// None while the leading section is still open to more text. `scan`
/// carries where the last look left off, so text already read for a
/// newline is not read again.
fn complete_section(source: &str, scan: &mut Scan) -> Option<usize> {
    // Past its first line, a section still open is a fenced block or a
    // paragraph: a first line that is blank, a heading or a bullet closes
    // its section on its own.
    let fenced = source.starts_with("```");
    loop {
        let Some(found) = source[scan.seen..].find('\n') else {
            scan.seen = source.len();
            return None;
        };
        let end = scan.seen + found;
        let line = &source[scan.line..end];
        if scan.line == 0 {
            if !fenced
                && (line.trim().is_empty() || heading(line).is_some() || bullet(line).is_some())
            {
                return Some(end + 1);
            }
        } else if fenced {
            // A fenced block runs to its closing fence, blank lines included.
            if fence(line).is_some() {
                return Some(end + 1);
            }
        } else if line.trim().is_empty()
            || heading(line).is_some()
            || bullet(line).is_some()
            || fence(line).is_some()
        {
            // A paragraph runs until a line that cannot join it.
            return Some(scan.line);
        }
        scan.line = end + 1;
        scan.seen = end + 1;
    }
}

/// Fold one section of markdown into a Block body. Blank sections have none.
fn parse_section(source: &str) -> Option<Body> {
    if let Some(body) = parse_code(source) {
        return Some(body);
    }
    let (shape, text) = prose(source)?;
    Some(shape.body(spans(text)))
}

/// A prose section's shape and the inline text its spans are read from.
fn prose(source: &str) -> Option<(Shape, &str)> {
    let source = source.trim();
    if source.is_empty() {
        return None;
    }
    if let Some((level, rest)) = heading(source) {
        return Some((Shape::Heading(level), rest));
    }
    if let Some(item) = bullet(source) {
        return Some((Shape::Bullet, item));
    }
    Some((Shape::Paragraph, source))
}

/// A fenced block, complete or still streaming. Its source keeps every inner
/// line untouched — indentation is code, not decoration.
fn parse_code(source: &str) -> Option<Body> {
    let mut lines = source.lines();
    let info = fence(lines.next()?)?;
    let mut body = Vec::new();
    for line in lines {
        if fence(line).is_some() {
            break;
        }
        body.push(line);
    }
    Some(Body::Code {
        language: (!info.is_empty()).then(|| info.to_string()),
        source: body.join("\n"),
        tokens: None,
    })
}

/// ```` ```rust ```` -> "rust"; a bare ```` ``` ```` -> "".
fn fence(line: &str) -> Option<&str> {
    Some(line.trim_end().strip_prefix("```")?.trim())
}

/// `- one` or `* one` -> "one".
fn bullet(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))?;
    Some(rest.trim())
}

/// `## Plan` -> (2, "Plan").
fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &line[hashes..];
    let trimmed = rest.strip_prefix(' ')?;
    Some((hashes as u8, trimmed.trim()))
}

/// Fold a line into spans.
pub(super) fn spans(text: &str) -> Vec<Span> {
    Inline::read(text)
        .spans
        .into_iter()
        .map(|(style, text)| Span {
            text: text.to_string(),
            style,
        })
        .collect()
}

/// A line's spans, as slices of it, and the first marker in it still
/// waiting on text to come: a backtick or `**` with no closer yet, or a
/// `[` its link's `](…)` has not followed.
struct Inline<'a> {
    text: &'a str,
    spans: Vec<(Style, &'a str)>,
    waiting: usize,
}

impl<'a> Inline<'a> {
    /// `**bold**` is found first, over the whole line — a bold run may hold
    /// inline code (`**drop the `MARK` lines**`), and splitting on backticks
    /// first left its markers literal. Backticked code is then cut out of
    /// each run, bold or plain; the plain runs split once more on
    /// `[text](url)`. A code span's own `**` never opens bold: the scan
    /// steps over code whole.
    fn read(text: &'a str) -> Self {
        let mut inline = Self {
            text,
            spans: Vec::new(),
            waiting: text.len(),
        };
        let mut plain = 0;
        let mut at = 0;
        while at < text.len() {
            let rest = &text[at..];
            if let Some(code) = rest.strip_prefix('`') {
                // Step over a closed code span; an unclosed backtick is text.
                at += match code.find('`') {
                    Some(end) => end + 2,
                    None => {
                        inline.wait(at);
                        1
                    }
                };
                continue;
            }
            if let Some((body, used)) = bold_at(rest) {
                inline.code_split(&text[plain..at], Style::Plain);
                inline.code_split(body, Style::Bold);
                at += used;
                plain = at;
                continue;
            }
            // `**` with no `**` after it yet: the text to come may close it.
            // (`****` is settled: its closer is already its first `**`.)
            if at < inline.waiting && rest.starts_with("**") && !rest[2..].contains("**") {
                inline.wait(at);
            }
            at += skip(rest, |b| matches!(b, b'`' | b'*'));
        }
        // A `*` at the end may be half of a `**` still to come.
        if text.ends_with('*') {
            inline.wait(text.len() - 1);
        }
        inline.code_split(&text[plain..], Style::Plain);
        inline
    }

    fn wait(&mut self, at: usize) {
        self.waiting = self.waiting.min(at);
    }

    /// Cut the backticked code out of one run: the odd pieces are Code, the
    /// even ones wear `style` — and a plain piece still folds its links.
    fn code_split(&mut self, text: &'a str, style: Style) {
        for (i, run) in text.split('`').enumerate() {
            if run.is_empty() {
                continue;
            }
            if i % 2 == 1 {
                self.spans.push((Style::Code, run));
            } else if style == Style::Plain {
                self.styled(run);
            } else {
                self.spans.push((style, run));
            }
        }
    }

    /// Fold one plain run into Plain/Bold/Link spans. An unclosed marker
    /// stays literal text — prose is never eaten on a guess.
    fn styled(&mut self, text: &'a str) {
        // A `[` in a run the line's end leaves open may yet find its `](…)`.
        let open = offset(self.text, text) + text.len() == self.text.len();
        let mut plain = 0;
        let mut at = 0;
        while at < text.len() {
            let rest = &text[at..];
            let styled = bold_at(rest)
                .map(|(body, used)| (body, used, Style::Bold))
                .or_else(|| link_at(rest).map(|(body, used)| (body, used, Style::Link)));
            if let Some((body, used, style)) = styled {
                self.plain(&text[plain..at]);
                self.spans.push((style, body));
                at += used;
                plain = at;
                continue;
            }
            if open && rest.starts_with('[') {
                self.wait(offset(self.text, rest));
            }
            at += skip(rest, |b| matches!(b, b'*' | b'['));
        }
        self.plain(&text[plain..]);
    }

    fn plain(&mut self, text: &'a str) {
        if !text.is_empty() {
            self.spans.push((Style::Plain, text));
        }
    }

    /// The latest point the line can be cut at for good, as the index of
    /// the span it falls in and its byte: inside a Plain span, at a plain
    /// character before any marker still waiting. The text before it spans
    /// the same whatever follows, and the text after it reads as the rest
    /// of that span.
    fn cut(&self) -> Option<(usize, usize)> {
        self.spans
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, (style, _))| *style == Style::Plain)
            .find_map(|(index, &(_, run))| {
                let start = offset(self.text, run);
                let end = self.waiting.min(start + run.len());
                let (last, _) = self.text.get(start..end)?.char_indices().next_back()?;
                (last > 0).then_some((index, start + last))
            })
    }
}

/// How far the scan steps from the front of `rest`: past its first
/// character, then on to the next byte that can open a marker. Nothing
/// between is the scan's to decide.
fn skip(rest: &str, marker: impl Fn(u8) -> bool) -> usize {
    let first = rest.chars().next().map_or(1, char::len_utf8);
    let next = rest.as_bytes()[first..].iter().position(|&b| marker(b));
    first + next.unwrap_or(rest.len() - first)
}

/// `**bold** …` → ("bold", bytes consumed).
fn bold_at(rest: &str) -> Option<(&str, usize)> {
    let body = rest.strip_prefix("**")?;
    let end = body.find("**")?;
    (end > 0).then_some((&body[..end], end + 4))
}

/// `[text](url) …` → ("text", bytes consumed). Only the text survives.
fn link_at(rest: &str) -> Option<(&str, usize)> {
    let body = rest.strip_prefix('[')?;
    let close = body.find("](")?;
    let target = &body[close + 2..];
    let end = target.find(')')?;
    (close > 0).then_some((&body[..close], 1 + close + 2 + end + 1))
}
