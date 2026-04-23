//! Persistent store for connectors. Flat JSON file in the config
//! dir for v1 — simple enough to audit by hand, small enough to fit
//! in memory (<100 entries per device realistically). Upgraded to
//! sqlite/sled if a use case ever needs indexed lookups.

use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::connector::Connector;

/// Wire shape of `<config>/connectors.json`. `version` lets future
/// migrations distinguish old from new without eyeballing the file.
#[derive(Debug, Serialize, Deserialize, Default)]
struct StoreFile {
    version: u32,
    connectors: Vec<Connector>,
}

const STORE_VERSION: u32 = 1;

pub struct Store {
    path: PathBuf,
    connectors: Vec<Connector>,
}

impl Store {
    /// Load the store at `<config_dir>/connectors.json`, or return an
    /// empty store if the file doesn't exist.
    ///
    /// Tolerates entries with kinds this bastion no longer supports
    /// (e.g. `ssh-host`, `anthropic`, `github`, `local-shell` from
    /// pre-sessions-restructure bastions): those rows are skipped with
    /// a stderr warning and omitted from the loaded list. On next
    /// `save()` they are dropped from disk.
    pub fn load(config_dir: &Path) -> Result<Self> {
        let path = config_dir.join("connectors.json");
        let connectors = if path.exists() {
            let text =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let raw: serde_json::Value =
                serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
            let arr = raw
                .get("connectors")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let mut out = Vec::with_capacity(arr.len());
            for (i, item) in arr.into_iter().enumerate() {
                match serde_json::from_value::<Connector>(item.clone()) {
                    Ok(c) => out.push(c),
                    Err(e) => {
                        let label = item
                            .get("label")
                            .and_then(|v| v.as_str())
                            .unwrap_or("<unknown>");
                        eprintln!(
                            "bastion: skipping unrecognized connector #{i} ({label}): {e}"
                        );
                    }
                }
            }
            out
        } else {
            Vec::new()
        };
        Ok(Self { path, connectors })
    }

    pub fn list(&self) -> &[Connector] {
        &self.connectors
    }

    pub fn get(&self, id: &str) -> Option<&Connector> {
        self.connectors.iter().find(|c| c.id == id)
    }

    /// Upsert: if an entry with the same `id` exists, replace it;
    /// otherwise append. Callers should pre-check for dup labels if
    /// they care — the store doesn't enforce that.
    pub fn upsert(&mut self, c: Connector) {
        match self.connectors.iter().position(|x| x.id == c.id) {
            Some(i) => self.connectors[i] = c,
            None => self.connectors.push(c),
        }
    }

    pub fn remove(&mut self, id: &str) -> Option<Connector> {
        self.connectors
            .iter()
            .position(|c| c.id == id)
            .map(|i| self.connectors.remove(i))
    }

    /// Persist the current state atomically. Writes to `.tmp` then
    /// renames — crash-safe for the "half-written JSON" case.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = StoreFile {
            version: STORE_VERSION,
            connectors: self.connectors.clone(),
        };
        let text = serde_json::to_string_pretty(&file)?;
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        #[cfg(unix)]
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::ConnectorKind;

    #[test]
    fn empty_dir_returns_empty_store() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Store::load(tmp.path()).unwrap();
        assert!(s.list().is_empty());
    }

    #[test]
    fn upsert_save_load_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::load(tmp.path()).unwrap();
        let c = Connector::new(ConnectorKind::DdEnclave, "dev-box");
        let id = c.id.clone();
        s.upsert(c);
        s.save().unwrap();

        let s2 = Store::load(tmp.path()).unwrap();
        assert_eq!(s2.list().len(), 1);
        assert_eq!(s2.get(&id).unwrap().label, "dev-box");
    }

    #[test]
    fn remove_works() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::load(tmp.path()).unwrap();
        let c = Connector::new(ConnectorKind::DdEnclave, "main-key");
        let id = c.id.clone();
        s.upsert(c);
        assert!(s.remove(&id).is_some());
        assert!(s.list().is_empty());
    }
}
