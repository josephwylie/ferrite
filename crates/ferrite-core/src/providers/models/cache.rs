//! Last usable provider catalogs, remembered across Sessions and launches.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::store::Provider;
use crate::ModelInfo;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    schema: u32,
    models: Vec<ModelInfo>,
}

pub(crate) struct ModelCache {
    dir: PathBuf,
    claude: Vec<ModelInfo>,
    codex: Vec<ModelInfo>,
}

impl ModelCache {
    /// Missing, corrupt, or newer-schema caches leave that provider on
    /// its bundled fallback. Loading never writes or creates a Session.
    pub(crate) fn load(store: &Path) -> Self {
        let dir = store.join("model-catalogs");
        Self {
            claude: read(&dir.join("claude.json")),
            codex: read(&dir.join("codex.json")),
            dir,
        }
    }

    pub(crate) fn get(&self, provider: Provider) -> &[ModelInfo] {
        match provider {
            Provider::Claude => &self.claude,
            Provider::Codex => &self.codex,
        }
    }

    /// A complete announcement replaces the previous catalog, so removed
    /// models stay removed. Empty or malformed announcements preserve the
    /// last usable menu. Disk failure must not discard the live answer.
    pub(crate) fn remember(&mut self, provider: Provider, models: &[ModelInfo]) -> bool {
        let Some(models) = usable(models) else {
            return false;
        };
        let (known, name) = match provider {
            Provider::Claude => (&mut self.claude, "claude.json"),
            Provider::Codex => (&mut self.codex, "codex.json"),
        };
        if *known == models {
            return false;
        }
        *known = models;
        if let Err(error) = save(&self.dir.join(name), known) {
            eprintln!("ferrite: could not save {provider:?} model catalog: {error}");
        }
        true
    }
}

fn usable(models: &[ModelInfo]) -> Option<Vec<ModelInfo>> {
    if models.is_empty()
        || models
            .iter()
            .any(|model| model.value.trim().is_empty() || model.display.trim().is_empty())
    {
        return None;
    }
    let mut rows: Vec<ModelInfo> = Vec::new();
    for model in models {
        if !rows.iter().any(|known| known.value == model.value) {
            rows.push(model.clone());
        }
    }
    Some(rows)
}

fn read(path: &Path) -> Vec<ModelInfo> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok())
        .filter(|snapshot| snapshot.schema == 1)
        .and_then(|snapshot| usable(&snapshot.models))
        .unwrap_or_default()
}

fn save(path: &Path, models: &[ModelInfo]) -> io::Result<()> {
    static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);
    fs::create_dir_all(path.parent().expect("a catalog has a directory"))?;
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
            serde_json::to_vec(&Snapshot {
                schema: 1,
                models: models.to_vec(),
            })
            .map_err(io::Error::other)?,
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
            std::env::temp_dir().join(format!("ferrite-model-cache-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_corrupt_provider_cache_does_not_hide_the_other_provider() {
        let dir = scratch("corrupt");
        let mut cache = ModelCache::load(&dir);
        let claude = vec![ModelInfo::bare("future-claude")];
        let codex = vec![ModelInfo::bare("future-codex")];
        cache.remember(Provider::Claude, &claude);
        cache.remember(Provider::Codex, &codex);
        let path = dir.join("model-catalogs/claude.json");
        for bad in ["truncated", r#"{"schema":99,"models":[]}"#] {
            fs::write(&path, bad).unwrap();
            let loaded = ModelCache::load(&dir);
            assert!(loaded.get(Provider::Claude).is_empty());
            assert_eq!(loaded.get(Provider::Codex), codex);
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                bad,
                "loading never overwrites"
            );
        }
    }

    #[test]
    fn a_write_failure_keeps_the_announced_models_usable_in_memory() {
        let dir = scratch("write-failure");
        fs::write(dir.join("model-catalogs"), "not a directory").unwrap();
        let mut cache = ModelCache::load(&dir);
        let models = vec![ModelInfo::bare("future-claude")];
        assert!(cache.remember(Provider::Claude, &models));
        assert_eq!(cache.get(Provider::Claude), models);
        assert!(!cache.remember(Provider::Claude, &[]));
        assert_eq!(cache.get(Provider::Claude), models);
    }
}
