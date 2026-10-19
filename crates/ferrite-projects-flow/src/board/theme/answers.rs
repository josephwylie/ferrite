//! What the terminal answered when asked for its colours: its text colour (OSC 10), its background (OSC 11), each by
//! its own number, whether it is light (by that background), and whether its answer to the question for its kind
//! (DA1, asked last) is in, so nothing is waited for longer.
use super::Seen;

/// The answers in `buf` so far: what was told, and whether the last answer is in.
pub fn read_answers(buf: &str) -> (Seen, bool) {
    let said = |n: u8| -> Option<u32> {
        let re = regex::Regex::new(&format!(r"\]{n};rgb:([0-9a-fA-F]+)/([0-9a-fA-F]+)/([0-9a-fA-F]+)")).unwrap();
        let c = re.captures(buf)?;
        let byte = |x: &str| { let two: String = x.chars().take(2).collect(); let two = if two.len() == 1 { format!("{two}{two}") } else { two }; u32::from_str_radix(&two, 16).unwrap_or(0) };
        Some(byte(&c[1]) << 16 | byte(&c[2]) << 8 | byte(&c[3]))
    };
    let (fg, bg) = (said(10), said(11));
    let light = bg.map(|bg| { let (r, g, b) = ((bg >> 16) as f64, ((bg >> 8) & 255) as f64, (bg & 255) as f64); 0.2126 * r + 0.7152 * g + 0.0722 * b > 140.0 });
    (Seen { light, fg, bg }, regex::Regex::new(r"\x1b\[\?[\d;]*c").unwrap().is_match(buf))
}
