//! Operator preferences: one JSON file under the store directory, durable
//! across launches.
//!
//! Settings are a menu of defaults, never history: the Thread headers keep
//! each Thread's own provider and model as the durable truth, so a settings
//! file lost or damaged by hand changes what the *next* Thread starts with
//! and nothing that already exists. That is why `load` never fails and
//! never writes — a bad file is worth more to the operator whole than
//! replaced by defaults they did not choose.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::store::Provider;

/// The operator's preferences. Every field has a default, and the file is
/// read with `#[serde(default)]`: a field missing from the file takes its
/// default, and a field this build does not know is ignored, so a settings
/// file written by a newer Ferrite still loads here (forward compatible).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The Provider a new Thread starts on. Default: Claude.
    pub default_provider: Provider,
    /// The Claude model to ask for. `None` is the CLI's own default;
    /// otherwise a value the CLI accepts ("sonnet", "claude-fable-5-1").
    pub claude_model: Option<String>,
    /// The Codex model to ask for. `None` is the codex config default.
    pub codex_model: Option<String>,
    /// The reasoning effort a new Claude Thread starts with (`"low"` …
    /// `"max"`). `None` is the CLI's own default.
    pub claude_effort: Option<String>,
    /// The reasoning effort a new Codex Thread starts with (`"low"` …
    /// `"ultra"` where the model takes it). `None` is the codex config
    /// default.
    pub codex_effort: Option<String>,
    /// Claude's permission mode. `None` is the CLI default; otherwise
    /// "default" | "acceptEdits" | "plan" | "bypassPermissions".
    pub claude_permission_mode: Option<String>,
    /// Codex's approval policy: "on-request" (default) | "untrusted" | "never".
    pub codex_approval_policy: String,
    /// Codex's sandbox. `None` is the codex default; otherwise
    /// "read-only" | "workspace-write" | "danger-full-access".
    pub codex_sandbox: Option<String>,
    /// Whether the navigation rail is collapsed. Default: false.
    pub nav_collapsed: bool,
    /// Whether deleting a Thread asks first. Default: true.
    pub confirm_delete: bool,
    /// Whether an untitled Thread is named from its first prompt.
    /// Default: true.
    pub auto_title: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            default_provider: Provider::Claude,
            claude_model: None,
            codex_model: None,
            claude_effort: None,
            codex_effort: None,
            claude_permission_mode: None,
            codex_approval_policy: "on-request".to_string(),
            codex_sandbox: None,
            nav_collapsed: false,
            confirm_delete: true,
            auto_title: true,
        }
    }
}

impl Settings {
    /// The file's name under the store directory.
    pub const FILE: &str = "settings.json";

    /// Read `<dir>/settings.json`. A missing file is the defaults. An
    /// unreadable or corrupt file is the defaults too, and the bad file is
    /// left in place untouched: `load` never writes, so nothing overwrites
    /// it until the operator changes a setting and the caller `save`s —
    /// callers save on change, never on load, which is what keeps a
    /// damaged file around long enough to be looked at or repaired by hand.
    pub fn load(dir: &Path) -> Settings {
        match fs::read(dir.join(Self::FILE)) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Settings::default(),
        }
    }

    /// Write `<dir>/settings.json` as pretty JSON, creating `dir` if
    /// needed. Written beside and renamed over, so a crash mid-write leaves
    /// the old file whole — the store's own rewrite discipline.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let path = dir.join(Self::FILE);
        let tmp = path.with_extension("json.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(io::Error::other)?,
        )?;
        fs::rename(&tmp, &path)
    }

    /// The model choice for `provider`; `None` is that provider's own
    /// default.
    pub fn model_for(&self, provider: Provider) -> Option<&str> {
        match provider {
            Provider::Claude => self.claude_model.as_deref(),
            Provider::Codex => self.codex_model.as_deref(),
        }
    }

    pub fn set_model_for(&mut self, provider: Provider, model: Option<String>) {
        match provider {
            Provider::Claude => self.claude_model = model,
            Provider::Codex => self.codex_model = model,
        }
    }

    /// The reasoning effort a new Thread on `provider` starts with; `None`
    /// is that provider's own default.
    pub fn effort_for(&self, provider: Provider) -> Option<&str> {
        match provider {
            Provider::Claude => self.claude_effort.as_deref(),
            Provider::Codex => self.codex_effort.as_deref(),
        }
    }

    pub fn set_effort_for(&mut self, provider: Provider, effort: Option<String>) {
        match provider {
            Provider::Claude => self.claude_effort = effort,
            Provider::Codex => self.codex_effort = effort,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A fresh scratch directory for one test's settings file.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-settings-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Every field set away from its default, so a round trip that drops
    /// any one of them shows.
    fn everything_changed() -> Settings {
        Settings {
            default_provider: Provider::Codex,
            claude_model: Some("claude-fable-5-1".to_string()),
            codex_model: Some("gpt-5.4".to_string()),
            claude_effort: Some("max".to_string()),
            codex_effort: Some("high".to_string()),
            claude_permission_mode: Some("acceptEdits".to_string()),
            codex_approval_policy: "never".to_string(),
            codex_sandbox: Some("workspace-write".to_string()),
            nav_collapsed: true,
            confirm_delete: false,
            auto_title: false,
        }
    }

    #[test]
    fn saved_settings_load_back_unchanged() {
        let dir = scratch("round-trip").join("store");
        let settings = everything_changed();

        settings.save(&dir).unwrap();

        assert_eq!(Settings::load(&dir), settings);
        // Pretty JSON, and only the file — the tmp was renamed over.
        let text = fs::read_to_string(dir.join(Settings::FILE)).unwrap();
        assert!(
            text.contains("\n  \"default_provider\": \"codex\""),
            "{text}"
        );
        assert!(!dir.join("settings.json.tmp").exists());
    }

    #[test]
    fn a_missing_file_or_directory_is_the_defaults() {
        let dir = scratch("missing");

        assert_eq!(Settings::load(&dir), Settings::default());
        assert_eq!(Settings::load(&dir.join("nowhere")), Settings::default());
        assert!(!dir.join(Settings::FILE).exists(), "load never writes");
    }

    #[test]
    fn the_defaults_are_the_documented_ones() {
        let settings = Settings::default();

        assert_eq!(settings.default_provider, Provider::Claude);
        assert_eq!(settings.claude_model, None);
        assert_eq!(settings.codex_model, None);
        assert_eq!(settings.claude_effort, None);
        assert_eq!(settings.codex_effort, None);
        assert_eq!(settings.claude_permission_mode, None);
        assert_eq!(settings.codex_approval_policy, "on-request");
        assert_eq!(settings.codex_sandbox, None);
        assert!(!settings.nav_collapsed);
        assert!(settings.confirm_delete);
        assert!(settings.auto_title);
    }

    /// A corrupt file loads as the defaults and stays exactly as it was:
    /// nothing rewrites it until the operator changes something.
    #[test]
    fn a_corrupt_file_is_the_defaults_and_is_left_untouched() {
        let dir = scratch("corrupt");
        let path = dir.join(Settings::FILE);
        let garbage = b"{\"default_provider\": \"codex\", ";
        fs::write(&path, garbage).unwrap();

        assert_eq!(Settings::load(&dir), Settings::default());

        assert_eq!(fs::read(&path).unwrap(), garbage);
        assert!(!dir.join("settings.json.tmp").exists());
        // A wrong type is corrupt too: no half-read where one field lands
        // and the rest silently default.
        fs::write(
            &path,
            br#"{"nav_collapsed": "yes", "confirm_delete": false}"#,
        )
        .unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());
    }

    /// An unreadable file — here a directory squatting at its path — is the
    /// defaults, not a failure.
    #[test]
    fn an_unreadable_file_is_the_defaults() {
        let dir = scratch("unreadable");
        fs::create_dir_all(dir.join(Settings::FILE)).unwrap();

        assert_eq!(Settings::load(&dir), Settings::default());
    }

    /// A field this build does not know is ignored, so a file from a newer
    /// Ferrite still loads.
    #[test]
    fn an_unknown_field_is_ignored() {
        let dir = scratch("unknown-field");
        fs::write(
            dir.join(Settings::FILE),
            br#"{"default_provider": "codex", "future_knob": {"deep": [1, 2]}}"#,
        )
        .unwrap();

        let loaded = Settings::load(&dir);

        assert_eq!(loaded.default_provider, Provider::Codex);
        assert_eq!(
            loaded,
            Settings {
                default_provider: Provider::Codex,
                ..Settings::default()
            }
        );
    }

    /// A file naming only some fields takes the defaults for the rest.
    #[test]
    fn a_partial_file_fills_in_the_defaults() {
        let dir = scratch("partial");
        fs::write(
            dir.join(Settings::FILE),
            br#"{"nav_collapsed": true, "codex_approval_policy": "untrusted"}"#,
        )
        .unwrap();

        let loaded = Settings::load(&dir);

        assert_eq!(
            loaded,
            Settings {
                nav_collapsed: true,
                codex_approval_policy: "untrusted".to_string(),
                ..Settings::default()
            }
        );
        // The file is untouched by the load — the defaults were not written
        // back into it.
        assert_eq!(
            fs::read(dir.join(Settings::FILE)).unwrap(),
            br#"{"nav_collapsed": true, "codex_approval_policy": "untrusted"}"#
        );
    }

    #[test]
    fn save_creates_the_directory_and_replaces_the_file_atomically() {
        let dir = scratch("create-dir").join("deeper").join("store");
        assert!(!dir.exists());

        Settings::default().save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());

        let changed = everything_changed();
        changed.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), changed);
        assert!(!dir.join("settings.json.tmp").exists());
    }

    #[test]
    fn model_is_kept_per_provider() {
        let mut settings = Settings::default();
        assert_eq!(settings.model_for(Provider::Claude), None);
        assert_eq!(settings.model_for(Provider::Codex), None);

        settings.set_model_for(Provider::Claude, Some("sonnet".to_string()));
        assert_eq!(settings.model_for(Provider::Claude), Some("sonnet"));
        assert_eq!(settings.model_for(Provider::Codex), None);
        assert_eq!(settings.claude_model.as_deref(), Some("sonnet"));

        settings.set_model_for(Provider::Codex, Some("gpt-5.4".to_string()));
        settings.set_model_for(Provider::Claude, None);
        assert_eq!(settings.model_for(Provider::Claude), None);
        assert_eq!(settings.model_for(Provider::Codex), Some("gpt-5.4"));
    }

    #[test]
    fn effort_is_kept_per_provider() {
        let mut settings = Settings::default();
        assert_eq!(settings.effort_for(Provider::Claude), None);
        assert_eq!(settings.effort_for(Provider::Codex), None);

        settings.set_effort_for(Provider::Claude, Some("high".to_string()));
        assert_eq!(settings.effort_for(Provider::Claude), Some("high"));
        assert_eq!(settings.effort_for(Provider::Codex), None);
        assert_eq!(settings.claude_effort.as_deref(), Some("high"));

        settings.set_effort_for(Provider::Codex, Some("ultra".to_string()));
        settings.set_effort_for(Provider::Claude, None);
        assert_eq!(settings.effort_for(Provider::Claude), None);
        assert_eq!(settings.effort_for(Provider::Codex), Some("ultra"));
    }
}
