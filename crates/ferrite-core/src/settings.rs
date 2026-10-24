//! Operator preferences: one JSON file under the store directory, durable
//! across launches.
//!
//! Settings are a menu of defaults, never history: the Thread headers keep
//! each Thread's own provider and model as the durable truth, so a settings
//! file lost or damaged by hand changes what the *next* Thread starts with
//! and nothing that already exists. `load` never writes, and says why a
//! file it could not use was refused: a bad file is worth more to the
//! operator whole than replaced by defaults they did not choose, and the
//! window tells them so rather than silently running on the defaults.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::sound::Sound;
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
    /// Whether the Solo view's right panel (Changes, PR, Context) shows
    /// where the window has room for it; ⌥⌘B and its own hide button
    /// flip it. Default: true.
    pub panel_shown: bool,
    /// How the navigation orders the rows inside each Project: creation
    /// order, or most recently used first. Changed from the ⌘K palette.
    pub thread_list_order: ThreadListOrder,
    /// Whether deleting a Thread asks first. Default: true.
    pub confirm_delete: bool,
    /// Whether an untitled Thread is named from its first prompt.
    /// Default: true.
    pub auto_title: bool,
    /// Whether Ferrite predicts a follow-up prompt in the empty Composer.
    /// Default: true.
    pub placeholder_suggestions: bool,
    /// The transcript's text size, in every Pane: the answer prose in px,
    /// one of `ReadingSize::STEPS`; every other transcript measure scales
    /// from it. Default: 13, the chrome's own size (an explicitly saved size
    /// is kept as it was).
    pub reading_size: ReadingSize,
    /// Whether a newer provider CLI is installed without asking, once no
    /// Session of that provider is running — new models arrive only
    /// through a newer CLI. Off, Ferrite still offers each update.
    /// Default: true.
    pub auto_update_clis: bool,
    /// Whether Ferrite looks for a newer release of itself every few
    /// hours and downloads it. Installing still waits for a restart, which
    /// Ferrite offers and never forces. Off, Settings checks on request.
    /// Default: true.
    pub check_for_updates: bool,
    /// Whether the Server starts at login (ADR 0025): a LaunchAgent on
    /// macOS, the Run key on Windows. Default: true.
    pub start_at_login: bool,
    /// Whether a transcript shows the model's thinking (Claude's thinking,
    /// Codex's reasoning summaries) as `∴` rows between the calls. The
    /// approved transcript has none, so they are kept off it and shown from
    /// the ⌘K palette's `show thinking`. Default: false.
    pub show_thinking: bool,
    /// Whether agents may show interactive visuals inline (Ferrite's own
    /// `show_visual` tool), and how readily: Off (not offered), When asked,
    /// or Automatic. Applies to Sessions started after it changes; a Codex
    /// thread keeps the tool it started with. Off also leaves ```mermaid
    /// fences as code rather than diagrams. Default: When asked.
    pub visuals: crate::visual::Level,
    /// The operator's own palette commands, in the order the palette
    /// lists them (`user_commands`). Default: none.
    pub commands: Vec<crate::user_commands::UserCommand>,
    /// The operator's keys over the defaults: an action's name to the
    /// keystroke that runs it instead (`"palette::Toggle": "cmd-p"`), or
    /// `null` to unbind it. Read at launch; the palette's `reset keys`
    /// clears it. Default: none.
    pub keys: BTreeMap<String, Option<String>>,
    /// Whether a Thread that finished or needs you plays a sound (ADR
    /// 0017). Default: true.
    pub sounds: bool,
    /// The file played when a Thread finishes, instead of the built-in
    /// chime. `None` is the built-in one.
    pub done_sound: Option<PathBuf>,
    /// The file played when a Thread needs you, instead of the built-in
    /// chime. `None` is the built-in one.
    pub needs_you_sound: Option<PathBuf>,
    /// Whether Claude's Threads play their sounds. Default: true.
    pub claude_sounds: bool,
    /// Whether Codex's Threads play their sounds. Default: true.
    pub codex_sounds: bool,
    /// Whether a Thread that finished or needs you posts a system
    /// notification while Ferrite's window is in the background.
    /// Default: true.
    pub system_notifications: bool,
    /// The use, in percent, at which a provider's 5-hour or weekly window
    /// warns: one toast as a window rises past each. Empty turns the
    /// warnings off. Default: 80 and 95.
    pub allowance_warnings: Vec<u8>,
    /// Other computers whose Servers the window can be on, over ssh (ADR
    /// 0026), in the nav's order. Default: none.
    pub machines: Vec<crate::machine::Machine>,
    /// The self-hosted forges a review reads pull requests from, besides
    /// github.com, gitlab.com and Azure DevOps (reviewr's `*_host` keys,
    /// ADR 0021). Default: none.
    pub forge_hosts: ForgeHosts,
}

/// One self-hosted host per forge, each a bare hostname (`ghe.corp.example`)
/// no built-in forge already claims.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ForgeHosts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gitlab: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub azure_devops: Option<String>,
}

impl ForgeHosts {
    /// What a review's pull request read resolves remotes against. Hosts
    /// reviewr refuses are all dropped, as it refuses a config naming one,
    /// and the log says why.
    pub fn review_config(&self) -> ferrite_reviewr::config::PluginConfig {
        ferrite_reviewr::config::PluginConfig::with_hosts(
            self.github.as_deref(),
            self.gitlab.as_deref(),
            self.azure_devops.as_deref(),
        )
        .unwrap_or_else(|error| {
            crate::log!("settings: forge_hosts refused: {error}");
            ferrite_reviewr::config::PluginConfig::default()
        })
    }
}

/// The transcript's text size in px, independent of provider and Thread:
/// a browser-style zoom over `STEPS`, 13 the standard (the chrome's grid).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReadingSize(u8);

impl ReadingSize {
    /// The sizes cmd-= and cmd-- step through: one pixel at a time where a
    /// pixel is a visible step, two above 16.
    pub const STEPS: [u8; 9] = [12, 13, 14, 15, 16, 18, 20, 22, 24];
    pub const STANDARD: ReadingSize = ReadingSize(13);
    pub const SMALLEST: ReadingSize = ReadingSize(Self::STEPS[0]);
    pub const LARGEST: ReadingSize = ReadingSize(Self::STEPS[Self::STEPS.len() - 1]);

    /// The step nearest `px` (a hand-edited file may say anything).
    pub fn nearest(px: u8) -> ReadingSize {
        let step = Self::STEPS
            .iter()
            .copied()
            .min_by_key(|step| (i16::from(*step) - i16::from(px)).abs())
            .unwrap_or(13);
        ReadingSize(step)
    }

    pub fn px(self) -> u8 {
        self.0
    }

    /// `delta` steps larger (positive) or smaller, stopping at either end.
    pub fn step(self, delta: i32) -> ReadingSize {
        let at = Self::STEPS
            .iter()
            .position(|step| *step == self.0)
            .unwrap_or(1) as i32;
        let to = (at + delta).clamp(0, Self::STEPS.len() as i32 - 1) as usize;
        ReadingSize(Self::STEPS[to])
    }
}

impl Default for ReadingSize {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// How the navigation orders the rows inside each Project section. The
/// sections themselves are always the Projects in creation order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThreadListOrder {
    /// Creation order: a row never moves because its Thread worked. The
    /// two retired readings (one activity stream, Project sections) load
    /// as this, the order that replaced them.
    #[default]
    #[serde(alias = "recent", alias = "by-project")]
    Created,
    /// Most recently used first (the palette's `sort: recent`).
    #[serde(rename = "last-used")]
    Recent,
    /// What needs the operator first — blocked, done, working, idle — and
    /// creation order within each (the palette's `sort: attention`).
    Attention,
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
            panel_shown: true,
            thread_list_order: ThreadListOrder::Created,
            confirm_delete: true,
            auto_title: true,
            placeholder_suggestions: true,
            reading_size: ReadingSize::STANDARD,
            auto_update_clis: true,
            check_for_updates: true,
            start_at_login: true,
            show_thinking: false,
            visuals: crate::visual::Level::WhenAsked,
            commands: Vec::new(),
            keys: BTreeMap::new(),
            sounds: true,
            done_sound: None,
            needs_you_sound: None,
            claude_sounds: true,
            codex_sounds: true,
            system_notifications: true,
            allowance_warnings: vec![80, 95],
            machines: Vec::new(),
            forge_hosts: ForgeHosts::default(),
        }
    }
}

impl Settings {
    /// The file's name under the store directory.
    pub const FILE: &str = "settings.json";

    /// Read `<dir>/settings.json`. A missing file is the defaults. A file
    /// that cannot be read or parsed is an error, and stays exactly as it
    /// is: `load` never writes, and the caller keeps the defaults in memory
    /// without saving over the file (the window says so), so a damaged file
    /// is around to be looked at or repaired by hand.
    pub fn load(dir: &Path) -> io::Result<Settings> {
        let bytes = match fs::read(dir.join(Self::FILE)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(error) => return Err(error),
        };
        let mut settings: Settings = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        settings.reading_size = ReadingSize::nearest(settings.reading_size.px());
        // A file from before the zoom names one of three sizes; it keeps
        // reading at the size it chose.
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if value.get("reading_size").is_none() {
                if let Some(old) = value.get("solo_reading_size").and_then(|v| v.as_str()) {
                    // The pixel size each name meant when it was saved (the
                    // old standard was 14): a chosen size is never migrated.
                    settings.reading_size = match old {
                        "comfortable" => ReadingSize(16),
                        "large" => ReadingSize(18),
                        _ => ReadingSize(14),
                    };
                }
            }
        }
        Ok(settings)
    }

    /// Write `<dir>/settings.json` as pretty JSON, creating `dir` if
    /// needed, so that a crash at any moment leaves the old file or the
    /// new one (`store::replace_file`); a symlinked file keeps its link.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        crate::store::replace_file(
            &dir.join(Self::FILE),
            &serde_json::to_vec_pretty(self).map_err(io::Error::other)?,
        )
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

    /// Whether a Thread on `provider` plays its sounds: sounds are on and
    /// that provider is not muted.
    pub fn sounds_for(&self, provider: Provider) -> bool {
        self.sounds && self.provider_sounds(provider)
    }

    /// Whether `provider` is unmuted, whatever the switch for all sounds
    /// says.
    pub fn provider_sounds(&self, provider: Provider) -> bool {
        match provider {
            Provider::Claude => self.claude_sounds,
            Provider::Codex => self.codex_sounds,
        }
    }

    pub fn set_provider_sounds(&mut self, provider: Provider, on: bool) {
        match provider {
            Provider::Claude => self.claude_sounds = on,
            Provider::Codex => self.codex_sounds = on,
        }
    }

    /// The operator's own file for `sound`; `None` is the built-in chime.
    pub fn sound_file(&self, sound: Sound) -> Option<&Path> {
        match sound {
            Sound::Done => self.done_sound.as_deref(),
            Sound::NeedsYou => self.needs_you_sound.as_deref(),
        }
    }

    pub fn set_sound_file(&mut self, sound: Sound, file: Option<PathBuf>) {
        match sound {
            Sound::Done => self.done_sound = file,
            Sound::NeedsYou => self.needs_you_sound = file,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch directory for one test's settings file.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-settings-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The names in `dir`, sorted.
    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
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
            panel_shown: false,
            thread_list_order: ThreadListOrder::Recent,
            confirm_delete: false,
            auto_title: false,
            placeholder_suggestions: false,
            reading_size: ReadingSize(18),
            auto_update_clis: false,
            check_for_updates: false,
            start_at_login: false,
            show_thinking: true,
            visuals: crate::visual::Level::Automatic,
            commands: vec![crate::user_commands::UserCommand {
                name: "run tests".to_string(),
                run: crate::user_commands::Run::Shell("cargo test".to_string()),
                output: crate::user_commands::Output::Reader,
            }],
            keys: BTreeMap::from([
                ("palette::Toggle".to_string(), Some("cmd-p".to_string())),
                ("cockpit::ToggleNav".to_string(), None),
            ]),
            sounds: false,
            done_sound: Some(PathBuf::from("/sounds/done.aiff")),
            needs_you_sound: Some(PathBuf::from("/sounds/needs-you.aiff")),
            claude_sounds: false,
            codex_sounds: false,
            system_notifications: false,
            allowance_warnings: vec![90],
            machines: vec![crate::machine::Machine {
                name: "build box".into(),
                target: "me@build.example.com".into(),
                enabled: false,
            }],
            forge_hosts: ForgeHosts {
                github: Some("ghe.corp.example".into()),
                gitlab: None,
                azure_devops: None,
            },
        }
    }

    #[test]
    fn saved_settings_load_back_unchanged() {
        let dir = scratch("round-trip").join("store");
        let settings = everything_changed();

        settings.save(&dir).unwrap();

        assert_eq!(Settings::load(&dir).unwrap(), settings);
        // Pretty JSON, and only the file — the tmp was renamed over.
        let text = fs::read_to_string(dir.join(Settings::FILE)).unwrap();
        assert!(
            text.contains("\n  \"default_provider\": \"codex\""),
            "{text}"
        );
        assert_eq!(entries(&dir), [Settings::FILE], "the temp was renamed over");
    }

    /// The retired orders (one activity stream, Project sections) load as
    /// creation order, which replaced both; `sort: recent` and `sort:
    /// attention` round-trip.
    #[test]
    fn retired_thread_orders_load_as_creation_order() {
        for (stored, order) in [
            ("\"recent\"", ThreadListOrder::Created),
            ("\"by-project\"", ThreadListOrder::Created),
            ("\"created\"", ThreadListOrder::Created),
            ("\"last-used\"", ThreadListOrder::Recent),
            ("\"attention\"", ThreadListOrder::Attention),
        ] {
            assert_eq!(
                serde_json::from_str::<ThreadListOrder>(stored).unwrap(),
                order,
                "{stored}"
            );
        }
        assert_eq!(
            serde_json::to_string(&ThreadListOrder::Recent).unwrap(),
            "\"last-used\""
        );
    }

    #[test]
    fn a_missing_file_or_directory_is_the_defaults() {
        let dir = scratch("missing");

        assert_eq!(Settings::load(&dir).unwrap(), Settings::default());
        assert_eq!(
            Settings::load(&dir.join("nowhere")).unwrap(),
            Settings::default()
        );
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
        assert!(settings.panel_shown);
        assert_eq!(settings.thread_list_order, ThreadListOrder::Created);
        assert!(settings.confirm_delete);
        assert!(settings.auto_title);
        assert!(settings.placeholder_suggestions);
        assert_eq!(settings.reading_size, ReadingSize::STANDARD);
        assert!(settings.sounds);
        assert_eq!(settings.sound_file(Sound::Done), None);
        assert_eq!(settings.sound_file(Sound::NeedsYou), None);
        assert!(settings.sounds_for(Provider::Claude) && settings.sounds_for(Provider::Codex));
        assert!(settings.system_notifications);
        assert_eq!(settings.allowance_warnings, [80, 95]);
    }

    /// A provider's mute silences only its own Threads, and turning sounds
    /// off silences every provider without forgetting the mutes.
    #[test]
    fn sounds_mute_per_provider_and_all_at_once() {
        let mut settings = Settings::default();
        settings.set_provider_sounds(Provider::Codex, false);
        assert!(settings.sounds_for(Provider::Claude));
        assert!(!settings.sounds_for(Provider::Codex));
        settings.sounds = false;
        assert!(!settings.sounds_for(Provider::Claude));
        settings.sounds = true;
        assert!(!settings.sounds_for(Provider::Codex), "the mute was kept");

        settings.set_sound_file(Sound::NeedsYou, Some(PathBuf::from("/tmp/ping.aiff")));
        assert_eq!(
            settings.sound_file(Sound::NeedsYou),
            Some(Path::new("/tmp/ping.aiff"))
        );
        assert_eq!(settings.sound_file(Sound::Done), None);
    }

    /// The zoom steps one size at a time and stops at either end; a file
    /// saying a size between steps reads as the nearest.
    #[test]
    fn the_reading_size_steps_and_stops_at_its_ends() {
        let standard = ReadingSize::STANDARD;
        assert_eq!(standard.px(), 13, "the standard is the chrome's 13");
        assert_eq!(standard.step(1).px(), 14);
        assert_eq!(standard.step(-1).px(), 12);
        assert_eq!(standard.step(3).px(), 16);
        // A saved size is read as it was saved, never migrated.
        assert_eq!(ReadingSize::nearest(14).px(), 14);
        assert_eq!(ReadingSize::LARGEST.step(1), ReadingSize::LARGEST);
        assert_eq!(ReadingSize::SMALLEST.step(-1), ReadingSize::SMALLEST);
        assert_eq!(ReadingSize::nearest(17).px(), 16);
        assert_eq!(ReadingSize::nearest(99), ReadingSize::LARGEST);
    }

    /// A settings file from before the zoom keeps its chosen size.
    #[test]
    fn an_old_reading_size_carries_over() {
        for (old, px) in [("standard", 14), ("comfortable", 16), ("large", 18)] {
            let dir = scratch(&format!("legacy-reading-{old}"));
            fs::write(
                dir.join(Settings::FILE),
                format!("{{\"solo_reading_size\": \"{old}\"}}"),
            )
            .unwrap();
            assert_eq!(Settings::load(&dir).unwrap().reading_size.px(), px, "{old}");
        }
    }

    /// A corrupt file is refused, saying why, and stays exactly as it was:
    /// the caller runs on the defaults without saving over it.
    #[test]
    fn a_corrupt_file_is_refused_and_left_untouched() {
        let dir = scratch("corrupt");
        let path = dir.join(Settings::FILE);
        let garbage = b"{\"default_provider\": \"codex\", ";
        fs::write(&path, garbage).unwrap();

        let error = Settings::load(&dir).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("EOF"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), garbage);
        assert_eq!(entries(&dir), [Settings::FILE], "load never writes");
        // A wrong type is corrupt too: no half-read where one field lands
        // and the rest silently default.
        fs::write(
            &path,
            br#"{"nav_collapsed": "yes", "confirm_delete": false}"#,
        )
        .unwrap();
        assert!(Settings::load(&dir).is_err());
    }

    /// A setting Ferrite no longer has (the retired usage meter style,
    /// smart filtering — the nav's Attention list is always on now) is
    /// ignored: the rest of the file still loads.
    #[test]
    fn a_retired_setting_is_ignored() {
        let dir = scratch("retired");
        fs::write(
            dir.join(Settings::FILE),
            br#"{"usage_meter_style": "rings", "smart_filtering": true, "confirm_delete": false}"#,
        )
        .unwrap();
        assert_eq!(
            Settings::load(&dir).unwrap(),
            Settings {
                confirm_delete: false,
                ..Settings::default()
            }
        );
    }

    /// An unreadable file — here a directory squatting at its path — is
    /// refused too, never mistaken for a missing one.
    #[test]
    fn an_unreadable_file_is_refused() {
        let dir = scratch("unreadable");
        fs::create_dir_all(dir.join(Settings::FILE)).unwrap();

        assert!(Settings::load(&dir).is_err());
    }

    /// A settings file symlinked elsewhere (a dotfiles repo) is saved
    /// through the link: the link stays, its target changes.
    #[cfg(unix)]
    #[test]
    fn save_writes_through_a_symlink() {
        let dir = scratch("symlink");
        let elsewhere = scratch("symlink-target").join("ferrite.json");
        fs::write(&elsewhere, b"{}").unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.join(Settings::FILE)).unwrap();

        let changed = everything_changed();
        changed.save(&dir).unwrap();

        let link = fs::symlink_metadata(dir.join(Settings::FILE)).unwrap();
        assert!(link.file_type().is_symlink(), "the link survives");
        assert_eq!(Settings::load(&dir).unwrap(), changed);
        assert_eq!(
            fs::read(&elsewhere).unwrap(),
            serde_json::to_vec_pretty(&changed).unwrap()
        );
    }

    /// A link whose target is gone is refused, not replaced by a plain
    /// file: the operator's link stays for them to mend.
    #[cfg(unix)]
    #[test]
    fn save_refuses_a_dangling_symlink() {
        let dir = scratch("dangling");
        let gone = dir.join("gone").join("ferrite.json");
        std::os::unix::fs::symlink(&gone, dir.join(Settings::FILE)).unwrap();

        assert!(everything_changed().save(&dir).is_err());

        let link = fs::symlink_metadata(dir.join(Settings::FILE)).unwrap();
        assert!(link.file_type().is_symlink(), "the link survives");
        assert!(!gone.exists());
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

        let loaded = Settings::load(&dir).unwrap();

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

        let loaded = Settings::load(&dir).unwrap();

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
        assert_eq!(Settings::load(&dir).unwrap(), Settings::default());

        let changed = everything_changed();
        changed.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).unwrap(), changed);
        assert_eq!(entries(&dir), [Settings::FILE], "the temp was renamed over");
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
