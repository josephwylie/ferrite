//! Native suggestions belong to one live Claude connection. The reader and
//! sender share this small inbox; sending or changing settings invalidates it.
use serde_json::Value;

#[derive(Default)]
pub(super) struct Inbox {
    pub enabled: bool,
    eligible: bool,
    session: Option<String>,
    pending: Option<String>,
}

impl Inbox {
    pub fn sent(&mut self) {
        self.eligible = false;
        self.pending = None;
    }

    pub fn configure(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.sent();
    }

    pub fn observe(&mut self, value: &Value) {
        if value
            .get("parent_tool_use_id")
            .is_some_and(|id| !id.is_null())
        {
            return;
        }
        match value["type"].as_str() {
            Some("system") if value["subtype"] == "init" => {
                self.session = value["session_id"].as_str().map(str::to_owned);
                self.sent();
            }
            Some("result") if self.same_session(value) => {
                self.eligible =
                    self.enabled && value["is_error"] == false && value["subtype"] == "success";
            }
            Some("prompt_suggestion") if self.eligible && self.same_session(value) => {
                self.pending = value["suggestion"]
                    .as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_owned);
            }
            _ => {}
        }
    }

    fn same_session(&self, value: &Value) -> bool {
        self.session
            .as_deref()
            .is_some_and(|id| value["session_id"] == id)
    }

    pub fn take(&mut self) -> Option<String> {
        self.pending.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn result() -> Value {
        json!({"type":"result","session_id":"main","subtype":"success","is_error":false})
    }
    fn suggestion() -> Value {
        json!({"type":"prompt_suggestion","session_id":"main","suggestion":"write the tests"})
    }

    #[test]
    fn native_suggestions_follow_success_and_never_leak_across_sends_or_settings() {
        let mut inbox = Inbox::default();
        inbox.configure(true);
        inbox.observe(&json!({"type":"system","subtype":"init","session_id":"main"}));
        inbox.observe(&suggestion());
        assert_eq!(inbox.take(), None);
        inbox.observe(&result());
        inbox.observe(&suggestion());
        assert_eq!(inbox.take().as_deref(), Some("write the tests"));
        assert_eq!(inbox.take(), None);
        inbox.sent();
        inbox.observe(&suggestion());
        assert_eq!(inbox.take(), None);
        inbox.observe(&result());
        inbox.configure(false);
        inbox.configure(true);
        inbox.observe(&suggestion());
        assert_eq!(inbox.take(), None);
        inbox.observe(&result());
        let mut child = suggestion();
        child["session_id"] = json!("child");
        inbox.observe(&child);
        assert_eq!(inbox.take(), None);
        let mut child = suggestion();
        child["parent_tool_use_id"] = json!("tool-1");
        inbox.observe(&child);
        assert_eq!(inbox.take(), None);
        inbox.observe(&json!({"type":"result","session_id":"main","subtype":"error_during_execution","is_error":true}));
        inbox.observe(&suggestion());
        assert_eq!(inbox.take(), None);
    }
}
