// The bundled faces' tables, read for the type rules' tests: which glyphs
// a face covers, and the render code that must draw only those.

use crate::theme::FALLBACK_GLYPHS;

/// Geist Mono, the code face (theme rule 1).
pub const GEIST_MONO: &[u8] = include_bytes!("../assets/fonts/GeistMono.ttf");

/// A TrueType table's byte range, by tag.
pub fn table<'a>(font: &'a [u8], tag: &[u8; 4]) -> &'a [u8] {
    let u16_at = |at: usize| u16::from_be_bytes([font[at], font[at + 1]]) as usize;
    let u32_at = |at: usize| {
        u32::from_be_bytes([font[at], font[at + 1], font[at + 2], font[at + 3]]) as usize
    };
    (0..u16_at(4))
        .map(|i| 12 + 16 * i)
        .find(|record| &font[*record..*record + 4] == tag)
        .map(|record| &font[u32_at(record + 8)..][..u32_at(record + 12)])
        .unwrap_or_else(|| panic!("no {} table", String::from_utf8_lossy(tag)))
}

pub fn be16(data: &[u8], at: usize) -> u32 {
    u16::from_be_bytes([data[at], data[at + 1]]) as u32
}

pub fn be32(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// Whether the face's Unicode cmap (format 12, else format 4) maps `c`
/// to a real glyph.
pub fn covers(font: &[u8], c: char) -> bool {
    let cmap = table(font, b"cmap");
    let code = c as u32;
    let subtables: Vec<&[u8]> = (0..be16(cmap, 2) as usize)
        .map(|i| &cmap[be32(cmap, 4 + 8 * i + 4) as usize..])
        .collect();
    if let Some(sub) = subtables.iter().find(|sub| be16(sub, 0) == 12) {
        return (0..be32(sub, 12) as usize).any(|group| {
            let at = 16 + 12 * group;
            (be32(sub, at)..=be32(sub, at + 4)).contains(&code) && {
                be32(sub, at + 8) + code - be32(sub, at) != 0
            }
        });
    }
    let sub = subtables
        .iter()
        .find(|sub| be16(sub, 0) == 4)
        .expect("a Unicode cmap");
    let segments = be16(sub, 6) as usize / 2;
    let (ends, starts) = (14, 16 + 2 * segments);
    let (deltas, ranges) = (starts + 2 * segments, starts + 4 * segments);
    (0..segments).any(|seg| {
        let (start, end) = (be16(sub, starts + 2 * seg), be16(sub, ends + 2 * seg));
        if !(start..=end).contains(&code) {
            return false;
        }
        let delta = be16(sub, deltas + 2 * seg);
        let range = be16(sub, ranges + 2 * seg) as usize;
        let glyph = if range == 0 {
            code
        } else {
            let at = ranges + 2 * seg + range + 2 * (code - start) as usize;
            be16(sub, at)
        };
        glyph != 0 && (glyph + delta) & 0xffff != 0
    })
}

/// Rule 10 over render code: every non-ASCII glyph a literal puts on
/// screen is Geist Mono's own or one the fallback face sets. `sources` are
/// a crate's render modules, `(name, source)`; their test modules and
/// comments are skipped. Each crate lists its own.
pub fn assert_render_code_draws_only_covered_glyphs(sources: &[(&str, &str)]) {
    let mut missing = Vec::new();
    for (file, source) in sources {
        let end = ["\n#[cfg(test)]\nmod ", "\n#[cfg(test)]\npub mod "]
            .iter()
            .filter_map(|module| source.find(module))
            .min()
            .unwrap_or(source.len());
        let body = &source[..end];
        for (at, line) in body.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            let code = code.split(" // ").next().unwrap_or(code);
            for c in code.chars().filter(|c| !c.is_ascii()) {
                if !covers(GEIST_MONO, c) && !FALLBACK_GLYPHS.contains(&c) {
                    missing.push(format!(
                        "{file}:{} {c} (U+{:04X}) not in Geist Mono",
                        at + 1,
                        c as u32
                    ));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "glyphs the face lacks:\n{}",
        missing.join("\n")
    );
}
