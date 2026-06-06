use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cache::embedder::cosine_similarity;

const VECTORS: TableDefinition<&str, &[u8]> = TableDefinition::new("vectors");
const META: TableDefinition<&str, &str> = TableDefinition::new("meta");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    pub key: String,
    pub scope: String,
    pub fingerprint: String,
    pub vector: Vec<f32>,
    pub output: Value,
    pub cached_request_id: String,
    pub created_at_secs: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub exact_entries: usize,
    pub vector_entries: usize,
    pub scopes: usize,
    pub persistent: bool,
    pub store_path: Option<String>,
}

#[derive(Clone)]
pub struct VectorDatabase {
    inner: Arc<RwLock<VectorState>>,
    db: Option<Arc<Database>>,
    path: Option<PathBuf>,
}

#[derive(Default)]
struct VectorState {
    records: HashMap<String, VectorRecord>,
    scope_fingerprints: HashMap<String, String>,
    hits: u64,
    misses: u64,
}

impl VectorDatabase {
    pub fn open(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let Some(path) = path else {
            return Ok(Self {
                inner: Arc::new(RwLock::new(VectorState::default())),
                db: None,
                path: None,
            });
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Arc::new(Database::create(&path)?);
        let inner = Arc::new(RwLock::new(VectorState::default()));
        let this = Self {
            inner: inner.clone(),
            db: Some(db.clone()),
            path: Some(path),
        };
        this.load_from_disk()?;
        Ok(this)
    }

    pub fn memory_only() -> Self {
        Self {
            inner: Arc::new(RwLock::new(VectorState::default())),
            db: None,
            path: None,
        }
    }

    fn load_from_disk(&self) -> anyhow::Result<()> {
        let Some(db) = &self.db else {
            return Ok(());
        };
        let read = db.begin_read()?;
        if !read.open_table(VECTORS).is_ok() {
            return Ok(());
        }
        let table = read.open_table(VECTORS)?;
        let mut guard = self.inner.write().expect("vector lock");
        for item in table.iter()? {
            let (_, value) = item?;
            let record: VectorRecord = serde_json::from_slice(value.value())?;
            guard.records.insert(record.key.clone(), record);
        }
        if let Ok(meta) = read.open_table(META) {
            for item in meta.iter()? {
                let (key, value) = item?;
                let key = key.value();
                if let Some(scope) = key.strip_prefix("scope:") {
                    guard
                        .scope_fingerprints
                        .insert(scope.to_string(), value.value().to_string());
                }
            }
        }
        Ok(())
    }

    fn persist_record(&self, record: &VectorRecord) -> anyhow::Result<()> {
        let Some(db) = &self.db else {
            return Ok(());
        };
        let write = db.begin_write()?;
        {
            let mut table = write.open_table(VECTORS)?;
            let bytes = serde_json::to_vec(record)?;
            table.insert(record.key.as_str(), bytes.as_slice())?;
            let mut meta = write.open_table(META)?;
            meta.insert(
                format!("scope:{}", record.scope).as_str(),
                record.fingerprint.as_str(),
            )?;
        }
        write.commit()?;
        Ok(())
    }

    fn delete_record_key(&self, key: &str) -> anyhow::Result<()> {
        let Some(db) = &self.db else {
            return Ok(());
        };
        let write = db.begin_write()?;
        {
            let mut table = write.open_table(VECTORS)?;
            let _ = table.remove(key);
        }
        write.commit()?;
        Ok(())
    }

    pub fn set_scope_fingerprint(&self, scope: &str, fingerprint: String) {
        let mut guard = self.inner.write().expect("vector lock");
        let previous = guard.scope_fingerprints.insert(scope.to_string(), fingerprint.clone());
        if previous.as_deref() == Some(fingerprint.as_str()) {
            return;
        }
        let stale: Vec<String> = guard
            .records
            .values()
            .filter(|r| r.scope == scope && r.fingerprint != fingerprint)
            .map(|r| r.key.clone())
            .collect();
        for key in stale {
            guard.records.remove(&key);
            let _ = self.delete_record_key(&key);
        }
    }

    pub fn search(
        &self,
        scope: &str,
        fingerprint: &str,
        query: &[f32],
        threshold: f64,
        ttl_secs: u64,
    ) -> Option<(VectorRecord, f64)> {
        let now = unix_now();
        let mut guard = self.inner.write().expect("vector lock");
        guard.misses += 1;

        let mut best: Option<(VectorRecord, f64)> = None;
        for record in guard.records.values() {
            if record.scope != scope || record.fingerprint != fingerprint {
                continue;
            }
            if ttl_secs > 0 && now.saturating_sub(record.created_at_secs) > ttl_secs {
                continue;
            }
            let score = cosine_similarity(query, &record.vector);
            if score >= threshold {
                if best.as_ref().map(|(_, s)| score > *s).unwrap_or(true) {
                    best = Some((record.clone(), score));
                }
            }
        }

        if let Some((record, score)) = best {
            guard.hits += 1;
            guard.misses = guard.misses.saturating_sub(1);
            Some((record, score))
        } else {
            None
        }
    }

    pub fn insert(&self, record: VectorRecord, max_entries: usize) -> anyhow::Result<()> {
        let evicted = {
            let mut guard = self.inner.write().expect("vector lock");
            let is_update = guard.records.contains_key(&record.key);
            let evicted = if !is_update {
                evict_oldest_in_scope(&mut guard, &record.scope, max_entries)
            } else {
                None
            };
            guard
                .scope_fingerprints
                .insert(record.scope.clone(), record.fingerprint.clone());
            guard.records.insert(record.key.clone(), record.clone());
            evicted
        };
        if let Some(key) = evicted {
            let _ = self.delete_record_key(&key);
        }
        self.persist_record(&record)
    }

    pub fn len(&self) -> usize {
        self.inner
            .read()
            .map(|g| g.records.len())
            .unwrap_or(0)
    }

    pub fn stats(&self, exact_entries: usize) -> CacheStats {
        let guard = self.inner.read().expect("vector lock");
        let scopes: std::collections::HashSet<_> = guard.records.values().map(|r| &r.scope).collect();
        CacheStats {
            exact_entries,
            vector_entries: guard.records.len(),
            scopes: scopes.len(),
            persistent: self.db.is_some(),
            store_path: self.path.as_ref().map(|p| p.display().to_string()),
            ..Default::default()
        }
    }

    pub fn hit_rate(&self) -> f64 {
        let guard = self.inner.read().expect("vector lock");
        let total = guard.hits + guard.misses;
        if total == 0 {
            0.0
        } else {
            guard.hits as f64 / total as f64
        }
    }
}

fn evict_oldest_in_scope(
    guard: &mut VectorState,
    scope: &str,
    max_entries: usize,
) -> Option<String> {
    let scope_count = guard.records.values().filter(|r| r.scope == scope).count();
    if scope_count < max_entries {
        return None;
    }
    let oldest_key = guard
        .records
        .values()
        .filter(|r| r.scope == scope)
        .min_by(|a, b| {
            a.created_at_secs
                .cmp(&b.created_at_secs)
                .then(a.key.cmp(&b.key))
        })
        .map(|r| r.key.clone())?;
    guard.records.remove(&oldest_key);
    Some(oldest_key)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn new_record(
    key: String,
    scope: String,
    fingerprint: String,
    vector: Vec<f32>,
    output: Value,
    cached_request_id: String,
) -> VectorRecord {
    VectorRecord {
        key,
        scope,
        fingerprint,
        vector,
        output,
        cached_request_id,
        created_at_secs: unix_now(),
    }
}
