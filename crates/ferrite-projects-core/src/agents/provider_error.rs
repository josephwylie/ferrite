//! Provider errors: which ones pass by themselves, and which need the human. A turn can end because the model's
//! provider failed, not because the agent finished. Some pass by themselves (capacity, overload, a rate limit, a dropped
//! connection): the board waits and tries again. The others need the human (a login, a model that does not exist, a
//! usage limit, a refused request) and are never retried. An error nobody recognises counts as the second kind. The
//! program's own code for the error decides where it has one (Codex `codex_error_info`, Claude `error` and
//! `apiErrorStatus`); the words decide for the rest.
use std::sync::LazyLock;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProviderError { pub passing: bool, pub reason: String, pub auth: bool }
fn code_passing(c: &str) -> Option<&'static str> {
    Some(match c {
        "server_overloaded" | "overloaded" | "overloaded_error" => "provider is overloaded",
        "model_cap" => "model is at capacity",
        "http_connection_failed" | "response_stream_connection_failed" | "response_stream_disconnected" | "response_too_many_failed_attempts" => "connection dropped",
        "internal_server_error" | "server_error" => "provider had a server error",
        _ => return None,
    })
}
const CODE_LASTING: [&str; 10] = ["unauthorized", "bad_request", "context_window_exceeded", "usage_limit_exceeded", "sandbox_error", "authentication_failed", "billing_error", "invalid_request", "model_not_found", "max_output_tokens"];
static WORDS_AUTH: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)\b(?:status|error|HTTP)\b[^\n]{0,12}\b40[13]\b|\b401 Unauthorized\b").unwrap());
static WORDS_LASTING: LazyLock<fancy_regex::Regex> = LazyLock::new(|| fancy_regex::Regex::new(r"(?i)usage limit|(?<!rate.)(?<!rate)limit reached|hit your [\w ]*limit|\bquota\b|billing|credit balance|payment|not supported|unsupported|does not exist|may not exist|not found|unknown (?:provider|model)|invalid|context (?:window|length)|flagged|content policy").unwrap());
static WORDS_PASSING: LazyLock<Vec<(regex::Regex, &'static str)>> = LazyLock::new(|| vec![
    (regex::Regex::new(r"(?i)at capacity").unwrap(), "model is at capacity"),
    (regex::Regex::new(r"(?i)rate.?limit|too many requests|\b429\b").unwrap(), "model is rate limited"),
    (regex::Regex::new(r"(?i)overloaded|\b529\b").unwrap(), "provider is overloaded"),
    (regex::Regex::new(r"(?i)socket|connection|network|\bE(?:CONN\w+|TIMEDOUT|AI_AGAIN|PIPE)\b|timed? ?out|fetch failed|stream (?:ended|closed|disconnected|error)|disconnect").unwrap(), "connection dropped"),
    (regex::Regex::new(r"(?i)\b(?:API error|status|HTTP)\b[^\n]{0,12}\b5\d\d\b|internal server error|bad gateway|service unavailable|upstream|server error|you can retry your request|try again later").unwrap(), "provider had a server error"),
]);
pub fn provider_error(words: &str, code: &str, status: i64) -> ProviderError {
    let c = code.to_lowercase();
    let said = WORDS_PASSING.iter().find(|(re, _)| re.is_match(words)).map(|(_, r)| r.to_string()).unwrap_or_default();
    let lasting = ProviderError::default();
    if c == "unauthorized" || c == "authentication_failed" || status == 401 || status == 403 || WORDS_AUTH.is_match(words) { return ProviderError { auth: true, ..lasting }; }
    if CODE_LASTING.contains(&c.as_str()) || ((400..500).contains(&status) && status != 408 && status != 429) || WORDS_LASTING.is_match(words).unwrap_or(false) { return lasting; }
    let pass = |r: String| ProviderError { passing: true, reason: r, auth: false };
    if let Some(r) = code_passing(&c) { return pass(if said.is_empty() { r.into() } else { said }); }
    if status == 429 || c == "rate_limit" { return pass("model is rate limited".into()); }
    if status == 408 || status >= 500 { return pass(if !said.is_empty() { said } else if status == 529 { "provider is overloaded".into() } else { "provider had a server error".into() }); }
    if said.is_empty() { lasting } else { pass(said) }
}
