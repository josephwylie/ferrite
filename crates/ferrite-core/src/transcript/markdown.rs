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
    /// What `block` keeps folded for good between deltas.
    folded: Option<Folded>,
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

/// What the open Block keeps folded for good between deltas, read from
/// where the last delta left off.
enum Folded {
    Prose(Settled),
    Code(Lines),
}

/// A fenced block's lines complete so far: how far into the section's source
/// they run, how much of the Block's code they make, and how many there are.
/// The unfinished line after them reads again with the next delta.
struct Lines {
    read: usize,
    code: usize,
    count: usize,
}

/// A prose section's spans as folded so far: where its text is cut, and
/// the markers before the cut still waiting on a closer. Its spans before
/// the cut stay as they are until one of those markers closes.
struct Settled {
    shape: Shape,
    /// Where the section's inline text starts in its source, past any
    /// indent and marker.
    start: usize,
    cut: Cut,
    /// The first waiting marker of each kind, earliest first.
    waiting: Vec<Wait>,
}

/// A point the section's inline text is folded up to: the text after it
/// reads again with the next delta, as the rest of the span it falls in.
#[derive(Clone, Copy, Default)]
struct Cut {
    /// Bytes of the inline text before it.
    bytes: usize,
    /// Spans before it, the last of them cut short at `last` bytes.
    spans: usize,
    last: usize,
    /// Whether it falls in code a lone backtick opened, which runs on.
    code: bool,
}

/// A marker the cut has passed while it still waits on its closer: the
/// cut from before it, and how far the text after it has been looked
/// through for a closer.
struct Wait {
    marker: Marker,
    at: usize,
    before: Cut,
    looked: usize,
    /// For a `[`: where its `](` is, once found.
    pair: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Marker {
    Tick,
    Bold,
    Link,
}

impl Wait {
    /// Whether `text` now closes the marker, looking only past where the
    /// last look ended.
    fn closes(&mut self, text: &str) -> bool {
        // A two-byte closer may straddle the last look's end.
        let from = |least: usize, looked: usize| least.max(looked.saturating_sub(1));
        let closed = match self.marker {
            Marker::Tick => text[self.looked.max(self.at + 1)..].contains('`'),
            Marker::Bold => find(text, from(self.at + 2, self.looked), "**").is_some(),
            Marker::Link => {
                if self.pair.is_none() {
                    self.pair = find(text, from(self.at + 1, self.looked), "](");
                }
                self.pair
                    .is_some_and(|pair| text[self.looked.max(pair + 2)..].contains(')'))
            }
        };
        self.looked = text.len();
        closed
    }
}

/// Where `needle` first is in `text` at or past `from`.
fn find(text: &str, from: usize, needle: &str) -> Option<usize> {
    let from = from.min(text.len());
    text.as_bytes()[from..]
        .windows(needle.len())
        .position(|window| window == needle.as_bytes())
        .map(|found| from + found)
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
        self.folded = None;
        Some(section)
    }

    /// Settle none of the open section, after it was folded whole: the
    /// next `grow_into` reads it from its start. A fenced block reads from
    /// its first line's end, once that has come.
    fn unsettle(&mut self) {
        self.folded = if self.source.starts_with("```") {
            self.source.find('\n').map(|end| {
                Folded::Code(Lines {
                    read: end + 1,
                    code: 0,
                    count: 0,
                })
            })
        } else {
            prose(&self.source).map(|(shape, text)| {
                Folded::Prose(Settled {
                    shape,
                    start: offset(&self.source, text),
                    cut: Cut::default(),
                    waiting: Vec::new(),
                })
            })
        };
    }

    /// Fold the text pushed since `block` last matched the section into it,
    /// re-reading only what a new delta can still change: a fenced block's
    /// unfinished line, or the prose after the last settled cut. False, with
    /// `block` untouched, when the section has to fold whole: its fence's
    /// first line is still coming, or its shape moved under it (`-` became
    /// a bullet).
    fn grow_into(&mut self, block: &mut Block) -> bool {
        let Some(markdown) = &mut block.markdown else {
            return false;
        };
        let grown = match &mut self.folded {
            Some(Folded::Code(lines)) => lines.grow(&self.source, &mut block.body),
            Some(Folded::Prose(settled)) => settled.grow(&self.source, &mut block.body),
            None => false,
        };
        if grown {
            markdown.push_str(&self.source[markdown.len()..]);
        }
        grown
    }
}

impl Lines {
    /// Append the fenced block's lines completed since the last read, then
    /// its unfinished line, as `parse_code` reads them: a line's `\r\n`
    /// is its end, and a closing fence still coming is no code.
    fn grow(&mut self, source: &str, body: &mut Body) -> bool {
        let Body::Code { source: code, .. } = body else {
            return false;
        };
        code.truncate(self.code);
        for line in source[self.read..].split_inclusive('\n') {
            let complete = line
                .strip_suffix('\n')
                .map(|line| line.strip_suffix('\r').unwrap_or(line));
            let text = complete.unwrap_or(line);
            if complete.is_none() && (text.is_empty() || fence(text).is_some()) {
                break;
            }
            if self.count > 0 {
                code.push('\n');
            }
            code.push_str(text);
            if complete.is_some() {
                self.read += line.len();
                self.code = code.len();
                self.count += 1;
            }
        }
        true
    }
}

impl Settled {
    /// Fold the prose after the settled cut into `body`'s spans, and move
    /// the cut as far as the new text lets it.
    fn grow(&mut self, source: &str, body: &mut Body) -> bool {
        if source.starts_with("```") {
            return false;
        }
        let Some((shape, text)) = prose(source) else {
            return false;
        };
        if shape != self.shape || offset(source, text) != self.start {
            return false;
        }
        let spans = match (body, shape) {
            (Body::Paragraph { spans }, Shape::Paragraph)
            | (Body::Bullet { spans }, Shape::Bullet) => spans,
            (Body::Heading { level, spans }, Shape::Heading(shaped)) if *level == shaped => spans,
            _ => return false,
        };

        // A marker the new text closes restyles what followed it: read
        // again from the cut before it.
        if let Some(closed) = self.waiting.iter_mut().position(|wait| wait.closes(text)) {
            self.cut = self.waiting[closed].before;
            self.waiting.truncate(closed);
        }

        // In code a lone backtick opened, the tail reads on from that
        // backtick.
        let from = self.cut;
        let reopened;
        let (read, shift) = if from.code {
            reopened = format!("`{}", &text[from.bytes..]);
            (reopened.as_str(), 1)
        } else {
            (&text[from.bytes..], 0)
        };
        let tail = Inline::read(read);

        spans.truncate(from.spans);
        let merged = from.spans > 0;
        let mut runs = tail.spans.iter();
        if merged {
            let last = &mut spans[from.spans - 1];
            last.text.truncate(from.last);
            if let Some((style, run)) = runs.next() {
                debug_assert_eq!(*style, last.style, "a cut reads on as its own span");
                last.text.push_str(run);
            }
        }
        spans.extend(runs.map(|&(style, text)| Span {
            text: text.to_string(),
            style,
        }));

        // Settle past every marker still waiting, keeping the cut before
        // the first of each kind for when it closes.
        let mut found = [
            (Marker::Tick, tail.tick.filter(|&at| at >= shift)),
            (Marker::Bold, tail.bold),
            (Marker::Link, tail.link),
        ];
        found.sort_by_key(|&(_, at)| at);
        let advance = |cut: Option<(usize, usize, bool)>| match cut {
            None => from,
            Some((index, at, code)) => Cut {
                bytes: from.bytes + at - shift,
                spans: from.spans - usize::from(merged) + index + 1,
                last: if merged && index == 0 {
                    from.last + at - shift
                } else {
                    at - offset(tail.text, tail.spans[index].1)
                },
                code,
            },
        };
        // A cut on a waiting marker would read it again as the span it
        // was cut from: cut before it.
        let cut_before = |mut limit: usize| loop {
            let cut = tail.cut(limit);
            match cut {
                Some((_, at, _)) if found.iter().any(|&(_, wait)| wait == Some(at)) => limit = at,
                _ => break cut,
            }
        };
        let end = cut_before(tail.text.len());
        for (marker, at) in found {
            let Some(at) = at else { continue };
            if end.is_none_or(|(_, cut, _)| cut <= at)
                || self.waiting.iter().any(|wait| wait.marker == marker)
            {
                continue;
            }
            self.waiting.push(Wait {
                marker,
                at: from.bytes + at - shift,
                before: advance(cut_before(at)),
                looked: text.len(),
                // Its `](` may be here already, its `)` still coming.
                pair: (marker == Marker::Link)
                    .then(|| find(text, from.bytes + at - shift + 1, "]("))
                    .flatten(),
            });
        }
        self.cut = advance(end);
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

/// A line's spans, as slices of it, and the first marker of each kind in
/// it still waiting on text to come: a backtick or `**` with no closer
/// yet, or a `[` its link's `](…)` has not followed.
struct Inline<'a> {
    text: &'a str,
    spans: Vec<(Style, &'a str)>,
    tick: Option<usize>,
    bold: Option<usize>,
    link: Option<usize>,
    /// The Code span a lone backtick opened that runs to the line's end.
    open_code: Option<usize>,
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
            tick: None,
            bold: None,
            link: None,
            open_code: None,
        };
        let mut plain = 0;
        let mut at = 0;
        while at < text.len() {
            let rest = &text[at..];
            if let Some(code) = rest.strip_prefix('`') {
                // Step over a closed code span; an unclosed backtick opens
                // none here (`code_split` reads it).
                at += match code.find('`') {
                    Some(end) => end + 2,
                    None => {
                        inline.tick.get_or_insert(at);
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
            if inline.bold.is_none() && rest.starts_with("**") && !rest[2..].contains("**") {
                inline.bold = Some(at);
            }
            at += skip(rest, |b| matches!(b, b'`' | b'*'));
        }
        inline.code_split(&text[plain..], Style::Plain);
        inline
    }

    /// Cut the backticked code out of one run, pairing backticks in turn:
    /// the text between a pair is Code, and a lone one makes the rest of
    /// the run Code. The rest wears `style`, and a plain piece still folds
    /// its links.
    fn code_split(&mut self, text: &'a str, style: Style) {
        let mut piece = 0;
        let mut at = 0;
        while let Some(found) = text[at..].find('`') {
            let tick = at + found;
            let rest = &text[tick..];
            let end = rest[1..].find('`').map_or(text.len(), |end| tick + 1 + end);
            self.styled_piece(&text[piece..tick], style);
            if end > tick + 1 {
                if end == text.len() && self.reaches_end(text) {
                    self.open_code = Some(self.spans.len());
                }
                self.spans.push((Style::Code, &text[tick + 1..end]));
            }
            at = text.len().min(end + 1);
            piece = at;
        }
        self.styled_piece(&text[piece..], style);
    }

    fn styled_piece(&mut self, text: &'a str, style: Style) {
        if style == Style::Plain {
            self.styled(text);
        } else if !text.is_empty() {
            self.spans.push((style, text));
        }
    }

    /// Fold one plain run into Plain/Bold/Link spans. An unclosed marker
    /// stays literal text — prose is never eaten on a guess.
    fn styled(&mut self, text: &'a str) {
        // A `[` in a run the line's end leaves open may yet find its `](…)`
        // — unless its `]` is the first: `[](` never links.
        let open = self.reaches_end(text);
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
            if open && self.link.is_none() && rest.starts_with('[') && !rest[1..].starts_with("](")
            {
                self.link = Some(offset(self.text, rest));
            }
            at += skip(rest, |b| matches!(b, b'*' | b'['));
        }
        self.plain(&text[plain..]);
    }

    fn reaches_end(&self, run: &str) -> bool {
        offset(self.text, run) + run.len() == self.text.len()
    }

    fn plain(&mut self, text: &'a str) {
        if !text.is_empty() {
            self.spans.push((Style::Plain, text));
        }
    }

    /// The latest point before `limit` the line can be cut at, as the index
    /// of the span it falls in, its byte, and whether it falls in open
    /// code: a character of a Plain span, or of the code a lone backtick
    /// opened. Whatever follows, and unless a marker before it closes, the
    /// text before it spans the same, and the text from it reads as the
    /// rest of that span. A `*` at the end may be half of a `**` to come.
    fn cut(&self, limit: usize) -> Option<(usize, usize, bool)> {
        let limit = limit.min(self.text.len() - usize::from(self.text.ends_with('*')));
        self.spans
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(index, &(style, run))| {
                let code = self.open_code == Some(index);
                (style == Style::Plain || code).then_some((index, run, code))
            })
            .find_map(|(index, run, code)| {
                let start = offset(self.text, run);
                let end = limit.min(start + run.len());
                let (last, _) = self.text.get(start..end)?.char_indices().next_back()?;
                Some((index, start + last, code))
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
