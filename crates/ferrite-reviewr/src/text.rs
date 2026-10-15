//! Lines and line endings, as git counts them and as Windows writes them.

/// `text`'s lines with their endings, split on `\n` alone, as git counts them.
pub(crate) fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// A line without its ending, and whether that ending carried a CR.
pub(crate) fn line_body(line: &str) -> (&str, bool) {
    let line = line.strip_suffix('\n').unwrap_or(line);
    match line.strip_suffix('\r') {
        Some(body) => (body, true),
        None => (line, false),
    }
}

/// `\n` as CRLF, an existing CRLF kept, a lone CR passed through.
pub(crate) fn crlf_line_breaks(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_line_breaks_are_crlf_and_never_doubled() {
        let rows = [
            ("a.rs:2\n+b\nok", "a.rs:2\r\n+b\r\nok"),
            ("a\n\nb\n", "a\r\n\r\nb\r\n"),
            ("a\r\nb", "a\r\nb"),
            ("a\rb", "a\rb"),
        ];
        for (text, want) in rows {
            assert_eq!(super::crlf_line_breaks(text), want, "{text:?}");
        }
    }
}
