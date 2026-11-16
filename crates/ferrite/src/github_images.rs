//! A private repository's images (a PR body's screenshots) answer 404 to
//! an anonymous fetch. Their addresses are read as raw file fetches
//! signed with the operator's `gh` login; every other address passes
//! through untouched.

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, OnceLock},
};

use gpui::http_client::{
    http::{header::AUTHORIZATION, HeaderValue},
    AsyncBody, HttpClient, Request, Response, Url,
};

type Sent = Pin<Box<dyn Future<Output = gpui::http_client::Result<Response<AsyncBody>>> + Send>>;

pub(crate) struct GitHubImages {
    inner: Arc<dyn HttpClient>,
    token: Arc<OnceLock<Option<HeaderValue>>>,
}

impl GitHubImages {
    pub(crate) fn new(inner: Arc<dyn HttpClient>) -> Self {
        Self {
            inner,
            token: Arc::default(),
        }
    }
}

impl HttpClient for GitHubImages {
    fn user_agent(&self) -> Option<&HeaderValue> {
        self.inner.user_agent()
    }

    fn proxy(&self) -> Option<&Url> {
        self.inner.proxy()
    }

    fn send(&self, mut request: Request<AsyncBody>) -> Sent {
        let Some(raw) = raw_address(&request.uri().to_string()) else {
            return self.inner.send(request);
        };
        let inner = self.inner.clone();
        let token = self.token.clone();
        Box::pin(async move {
            *request.uri_mut() = raw.parse()?;
            if let Some(token) = token.get_or_init(gh_token) {
                request.headers_mut().insert(AUTHORIZATION, token.clone());
            }
            inner.send(request).await
        })
    }
}

/// The raw file fetch behind a repository file's address: a
/// `raw.githubusercontent.com` one as it is, a `github.com` `blob/…?raw=true`
/// or `raw/…` one rewritten to it.
fn raw_address(address: &str) -> Option<String> {
    const RAW: &str = "https://raw.githubusercontent.com/";
    if address.starts_with(RAW) {
        return Some(address.to_string());
    }
    let rest = address.strip_prefix("https://github.com/")?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut parts = path.splitn(4, '/');
    let (owner, repo, kind, file) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let raw = match kind {
        "raw" => true,
        "blob" => query.split('&').any(|pair| pair == "raw=true"),
        _ => false,
    };
    (raw && !file.is_empty()).then(|| format!("{RAW}{owner}/{repo}/{file}"))
}

fn gh_token() -> Option<HeaderValue> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token", "--hostname", "github.com"])
        .output()
        .ok()?;
    let token = String::from_utf8(output.stdout).ok()?;
    let token = token.trim();
    (output.status.success() && !token.is_empty())
        .then(|| HeaderValue::from_str(&format!("token {token}")).ok())
        .flatten()
        .map(|mut value| {
            value.set_sensitive(true);
            value
        })
}

#[cfg(test)]
mod tests {
    use super::raw_address;

    #[test]
    fn a_repository_files_address_reads_as_its_raw_fetch() {
        let raw = "https://raw.githubusercontent.com/o/r/abc/shots/a.png";
        assert_eq!(
            raw_address("https://github.com/o/r/blob/abc/shots/a.png?raw=true").as_deref(),
            Some(raw)
        );
        assert_eq!(
            raw_address("https://github.com/o/r/raw/abc/shots/a.png").as_deref(),
            Some(raw)
        );
        assert_eq!(raw_address(raw).as_deref(), Some(raw));
    }

    #[test]
    fn other_addresses_pass_through() {
        for address in [
            "https://github.com/o/r/blob/abc/shots/a.png",
            "https://github.com/o/r/pull/3",
            "https://github.com/user-attachments/assets/1234",
            "https://example.com/o/r/raw/abc/a.png",
            "https://github.com/o/r/raw/",
        ] {
            assert_eq!(raw_address(address), None, "{address}");
        }
    }
}
