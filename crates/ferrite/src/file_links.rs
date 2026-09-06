//! Resolve transcript destinations once, then hand the OS a real file URL.
use std::path::{Path, PathBuf};

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
