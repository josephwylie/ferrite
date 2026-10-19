//! The HTML document a view loads, and the small protocol between the page and
//! the browser process.
//!
//! The host's theme and a tiny host script go *before* the agent's HTML, as the
//! head of the document. The page's own `<!doctype>`, `<html>` and `<head>`
//! tags (if any) then merge into it per the HTML parsing rules, so both full
//! documents and bare fragments work.
//!
//! # How the page reports its height
//!
//! The host script writes `console.debug("\u0001ferrite:height:<px>")`, which
//! arrives in the browser process as `CefDisplayHandler::OnConsoleMessage` on
//! the UI thread; the handler swallows it. Alternatives considered:
//! - `CefMessageRouter` / process messages need a `CefRenderProcessHandler` and
//!   a V8 binding in the renderer, i.e. code running in the sandboxed helper —
//!   more moving parts for one number.
//! - `document.title` changes would clobber the page's title.
//!
//! The console is page-writable, so a page can spoof the message. It can only
//! misreport its *own* height, which the host clamps; nothing else listens.

use crate::web::Page;

/// Prefix of the console line carrying the content height.
const HEIGHT_PREFIX: &str = "\u{1}ferrite:height:";

/// The `id` of the `<style>` holding the theme, replaced by [`theme_script`].
const THEME_STYLE_ID: &str = "__ferrite_theme";

/// Content-Security-Policy sent with the document. The request handler already
/// refuses every request except this document (and `data:`/`blob:`); this is the
/// second lock: inline script and style only, nothing fetched.
pub(super) const CSP: &str = "default-src 'none'; \
    script-src 'unsafe-inline' 'unsafe-eval'; style-src 'unsafe-inline'; \
    img-src data: blob:; font-src data:; media-src data: blob:; \
    connect-src 'none'; frame-src 'none'; worker-src blob:; \
    form-action 'none'; base-uri 'none'";

/// Measures the content height and reports it whenever it changes.
///
/// It reports `scrollHeight` only when content actually overflows the view;
/// otherwise the height of the `<html>` box. Plain `scrollHeight` is never less
/// than the view's own height, and the view's height is set *from* this value,
/// so it could grow but never shrink.
const HOST_SCRIPT: &str = r#"(() => {
  const PREFIX = "\u0001ferrite:height:";
  let last = -1, queued = false;
  const measure = () => {
    queued = false;
    const root = document.documentElement;
    const box = Math.ceil(root.getBoundingClientRect().height);
    const overflow = root.scrollHeight > innerHeight ? root.scrollHeight : 0;
    const height = Math.max(box, overflow);
    if (height !== last) { last = height; console.debug(PREFIX + height); }
  };
  const schedule = () => { if (!queued) { queued = true; requestAnimationFrame(measure); } };
  const observer = new ResizeObserver(schedule);
  observer.observe(document.documentElement);
  document.addEventListener("DOMContentLoaded", () => {
    if (document.body) observer.observe(document.body);
    schedule();
  });
  addEventListener("load", schedule);
})();"#;

/// The full document for `page`.
pub(super) fn assemble(page: &Page) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <style id=\"__ferrite_host\">html,body{{overflow:hidden}}</style>\
         <style id=\"{THEME_STYLE_ID}\">{theme}</style>\
         <script>{HOST_SCRIPT}</script>\n{html}",
        theme = page.theme_css,
        html = page.html,
    )
}

/// JavaScript that swaps the theme in the live page.
pub(super) fn theme_script(theme_css: &str) -> String {
    format!(
        "(() => {{ const s = document.getElementById(\"{THEME_STYLE_ID}\"); \
         if (s) s.textContent = {}; }})();",
        js_string(theme_css)
    )
}

/// The height carried by a console line, if it is one of ours.
pub(super) fn parse_height(console_message: &str) -> Option<u32> {
    console_message.strip_prefix(HEIGHT_PREFIX)?.trim().parse().ok()
}

/// `text` as a double-quoted JavaScript string literal.
fn js_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '<' => out.push_str("\\u003c"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_comes_before_the_page() {
        let doc = assemble(&Page { html: "<p>hi</p>".into(), theme_css: ":root{--a:1}".into() });
        let theme = doc.find(":root{--a:1}").unwrap();
        let body = doc.find("<p>hi</p>").unwrap();
        assert!(theme < body);
        assert!(doc.contains("overflow:hidden"));
    }

    #[test]
    fn height_lines_parse_and_others_do_not() {
        assert_eq!(parse_height("\u{1}ferrite:height:512"), Some(512));
        assert_eq!(parse_height("ferrite:height:512"), None);
        assert_eq!(parse_height("hello"), None);
    }

    #[test]
    fn theme_script_escapes() {
        let js = theme_script("a\"b\\c\n</style>");
        assert!(js.contains(r#""a\"b\\c\n\u003c/style>""#), "{js}");
    }
}
