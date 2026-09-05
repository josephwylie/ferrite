//! The last subscription windows a provider reported, remembered across
//! Sessions and launches.
//!
//! Rate limits are an account fact, not a Thread's: the same 5-hour and
//! weekly budget covers every Thread on that provider. The transcript log
//! deliberately does not replay them (a percentage from an old log is a
//! lie), so without this cache the meter reads "Not reported" on every
//! launch until the first turn of the first Thread happens to report.
//!
//! A remembered reading is served only while its window can still be the
//! one it was measured in — see `fresh`. An expired window reverts to
//! unknown rather than showing a percentage that has since rolled over.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::store::Provider;
use crate::transcript::RateLimits;
use crate::RateLimitWindow;

/// The rolling spans the two windows name, in seconds — the longest a
/// reading can possibly still describe when the provider gave no usable
/// reset instant.
const FIVE_HOUR: u64 = 5 * 60 * 60;
const WEEKLY: u64 = 7 * 24 * 60 * 60;

#[derive(Serialize, Deserialize, Clone, Copy)]
struct Window {
    used_fraction: f32,
    resets_at: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct Snapshot {
    schema: u32,
    /// When this reading was taken, in unix seconds — our own clock, so
    /// freshness never depends on a provider's units.
    saved_at: u64,
    five_hour: Option<Window>,
    weekly: Option<Window>,
}

pub(crate) struct LimitCache {
    dir: PathBuf,
    claude: Option<Snapshot>,
    codex: Option<Snapshot>,
}

impl LimitCache {
    /// Missing, corrupt, or newer-schema caches leave that provider
    /// unknown. Loading never writes.
    pub(crate) fn load(store: &Path) -> Self {
        let dir = store.join("rate-limits");
        Self {
            claude: read(&dir.join("claude.json")),
            codex: read(&dir.join("codex.json")),
            dir,
        }
    }

    /// What this provider last reported, with any window that can no
    /// longer be the one measured dropped back to unknown.
    pub(crate) fn get(&self, provider: Provider) -> RateLimits {
        let Some(snapshot) = self.snapshot(provider) else {
            return RateLimits::default();
        };
        let now = now();
        RateLimits {
            five_hour: window(snapshot.five_hour, snapshot.saved_at, FIVE_HOUR, now),
            weekly: window(snapshot.weekly, snapshot.saved_at, WEEKLY, now),
        }
    }

    /// A reported reading replaces the previous one. A report with neither
    /// window says nothing and is ignored, so one empty announcement
    /// cannot erase a usable reading. Disk failure must not discard the
    /// live answer.
    pub(crate) fn remember(&mut self, provider: Provider, limits: RateLimits) -> bool {
        if limits.five_hour.is_none() && limits.weekly.is_none() {
            return false;
        }
        let snapshot = Snapshot {
            schema: 1,
            saved_at: now(),
            five_hour: limits.five_hour.map(store_window),
            weekly: limits.weekly.map(store_window),
        };
        let (known, name) = match provider {
            Provider::Claude => (&mut self.claude, "claude.json"),
            Provider::Codex => (&mut self.codex, "codex.json"),
        };
        let unchanged = known.is_some_and(|known| {
            same(known.five_hour, snapshot.five_hour) && same(known.weekly, snapshot.weekly)
        });
        *known = Some(snapshot);
        // Providers repeat the same reading every turn; only a changed one
        // is worth the write, and only a changed one is worth a repaint.
        if unchanged {
            return false;
        }
        if let Err(error) = save(&self.dir.join(name), &snapshot) {
            eprintln!("ferrite: could not save {provider:?} rate limits: {error}");
        }
        true
    }

    fn snapshot(&self, provider: Provider) -> Option<Snapshot> {
        match provider {
            Provider::Claude => self.claude,
            Provider::Codex => self.codex,
        }
    }
}

/// A remembered window, if it can still be the window it was measured in.
///
/// The provider's own `resets_at` decides when it reads as a unix instant
/// after the reading was taken — the two providers do not agree on that
/// field's units, so a value that does not is not guessed at. Otherwise
/// the window's own rolling span is the outer bound: a 5-hour reading is
/// certainly stale five hours later, whatever the provider meant.
fn window(saved: Option<Window>, saved_at: u64, span: u64, now: u64) -> Option<RateLimitWindow> {
    let saved = saved?;
    let expires = match saved.resets_at {
        Some(resets_at) if resets_at > saved_at => resets_at.min(saved_at.saturating_add(span)),
        _ => saved_at.saturating_add(span),
    };
    (now < expires).then_some(RateLimitWindow {
        used_fraction: saved.used_fraction,
        resets_at: saved.resets_at,
    })
}

fn store_window(window: RateLimitWindow) -> Window {
    Window {
        used_fraction: window.used_fraction,
        resets_at: window.resets_at,
    }
}

fn same(left: Option<Window>, right: Option<Window>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.used_fraction == right.used_fraction && left.resets_at == right.resets_at
        }
        _ => false,
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

fn read(path: &Path) -> Option<Snapshot> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok())
        .filter(|snapshot| snapshot.schema == 1)
}

fn save(path: &Path, snapshot: &Snapshot) -> io::Result<()> {
    static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);
    fs::create_dir_all(path.parent().expect("a cache file has a directory"))?;
    // One file per provider; independent app processes never share a temp
    // filename. Renaming preserves the previous complete file on failure.
    let tmp = path.with_extension(format!(
        "json.{}.{}.tmp",
        std::process::id(),
        NEXT_WRITE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        fs::write(
            &tmp,
            serde_json::to_vec(snapshot).map_err(io::Error::other)?,
        )?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-limit-cache-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn limits(five: f32, weekly: f32) -> RateLimits {
        RateLimits {
            five_hour: Some(RateLimitWindow {
                used_fraction: five,
                resets_at: None,
            }),
            weekly: Some(RateLimitWindow {
                used_fraction: weekly,
                resets_at: None,
            }),
        }
    }

    #[test]
    fn a_reported_reading_survives_a_relaunch() {
        let dir = scratch("relaunch");
        let mut cache = LimitCache::load(&dir);
        assert_eq!(cache.get(Provider::Claude), RateLimits::default());
        assert!(cache.remember(Provider::Claude, limits(0.52, 0.08)));
        // The same reading again is not a change worth a repaint.
        assert!(!cache.remember(Provider::Claude, limits(0.52, 0.08)));

        let loaded = LimitCache::load(&dir);
        assert_eq!(loaded.get(Provider::Claude), limits(0.52, 0.08));
        // One provider's cache says nothing about the other.
        assert_eq!(loaded.get(Provider::Codex), RateLimits::default());
    }

    #[test]
    fn a_window_that_has_rolled_over_reads_as_unknown_again() {
        let stale = now() - WEEKLY - 1;
        let saved = Some(Window {
            used_fraction: 0.9,
            resets_at: None,
        });
        assert_eq!(window(saved, stale, FIVE_HOUR, now()), None);
        assert_eq!(window(saved, stale, WEEKLY, now()), None);
        // A fresh weekly reading outlives a five-hour one taken with it.
        let recent = now() - FIVE_HOUR - 1;
        assert_eq!(window(saved, recent, FIVE_HOUR, now()), None);
        assert!(window(saved, recent, WEEKLY, now()).is_some());
    }

    #[test]
    fn a_providers_own_reset_instant_expires_the_reading_early() {
        let saved_at = now() - 60;
        let saved = Some(Window {
            used_fraction: 0.4,
            resets_at: Some(saved_at + 30),
        });
        assert_eq!(window(saved, saved_at, FIVE_HOUR, now()), None);
        // A reset instant in units we cannot read is ignored, not obeyed.
        let opaque = Some(Window {
            used_fraction: 0.4,
            resets_at: Some(11),
        });
        assert!(window(opaque, saved_at, FIVE_HOUR, now()).is_some());
    }

    #[test]
    fn a_corrupt_cache_leaves_the_provider_unknown_and_is_not_overwritten() {
        let dir = scratch("corrupt");
        let mut cache = LimitCache::load(&dir);
        cache.remember(Provider::Claude, limits(0.5, 0.1));
        cache.remember(Provider::Codex, limits(0.2, 0.3));
        let path = dir.join("rate-limits/claude.json");
        for bad in ["truncated", r#"{"schema":99}"#] {
            fs::write(&path, bad).unwrap();
            let loaded = LimitCache::load(&dir);
            assert_eq!(loaded.get(Provider::Claude), RateLimits::default());
            assert_eq!(loaded.get(Provider::Codex), limits(0.2, 0.3));
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                bad,
                "loading never overwrites"
            );
        }
    }

    #[test]
    fn an_empty_report_cannot_erase_a_usable_reading() {
        let dir = scratch("empty");
        let mut cache = LimitCache::load(&dir);
        cache.remember(Provider::Claude, limits(0.5, 0.1));
        assert!(!cache.remember(Provider::Claude, RateLimits::default()));
        assert_eq!(cache.get(Provider::Claude), limits(0.5, 0.1));
    }
}
