use super::Template;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::{env, fs};

const CACHE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: String,
    fingerprint: String,
    templates: Vec<Template>,
}

pub struct TemplateCache {
    cache_dir: PathBuf,
}

impl Default for TemplateCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateCache {
    pub fn new() -> Self {
        let cache_dir = cache_directory();
        Self { cache_dir }
    }

    /// Try to load templates for `dir` from disk cache.
    /// Returns `None` if the cache is missing, stale, or corrupt.
    pub fn load(&self, dir: &Path) -> Option<Vec<Template>> {
        let fp = compute_fingerprint(dir)?;
        let cache_path = self.cache_path(&fp);

        let raw = fs::read(&cache_path).ok()?;
        let cf: CacheFile = serde_json::from_slice(&raw).ok()?;

        // Reject if ruclei version changed (struct layout may differ)
        if cf.version != CACHE_VERSION || cf.fingerprint != fp {
            return None;
        }

        Some(cf.templates)
    }

    /// Persist parsed templates to disk for future runs.
    pub fn save(&self, dir: &Path, templates: &[Template]) {
        let fp = match compute_fingerprint(dir) {
            Some(f) => f,
            None => return,
        };

        if let Err(e) = fs::create_dir_all(&self.cache_dir) {
            eprintln!("[WRN] Cache dir create failed: {}", e);
            return;
        }

        let cf = CacheFile {
            version: CACHE_VERSION.to_string(),
            fingerprint: fp.clone(),
            templates: templates.to_vec(),
        };

        match serde_json::to_vec(&cf) {
            Ok(bytes) => {
                let path = self.cache_path(&fp);
                if let Err(e) = fs::write(&path, bytes) {
                    eprintln!("[WRN] Cache write failed: {}", e);
                }
            }
            Err(e) => eprintln!("[WRN] Cache serialize failed: {}", e),
        }
    }

    fn cache_path(&self, fingerprint: &str) -> PathBuf {
        self.cache_dir
            .join(format!("templates-{}.json", fingerprint))
    }

    /// Remove all cache files (e.g. on --clear-cache).
    pub fn clear(&self) -> Result<()> {
        for entry in fs::read_dir(&self.cache_dir)? {
            let p = entry?.path();
            if p.extension().and_then(|e| e.to_str()) == Some("json") {
                let _ = fs::remove_file(p);
            }
        }
        Ok(())
    }
}

// ─── Fingerprint ─────────────────────────────────────────────────────────────

/// Walk `dir` and compute a hash over `(path, mtime_secs, file_size)` for every
/// YAML file, sorted by path. Fast: only stat calls, no file reads.
fn compute_fingerprint(dir: &Path) -> Option<String> {
    let mut entries: Vec<(String, u64, u64)> = Vec::new();
    collect_yaml_stats(dir, &mut entries);

    if entries.is_empty() {
        return None;
    }

    entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let mut h = DefaultHasher::new();
    for (path, mtime, size) in &entries {
        path.hash(&mut h);
        mtime.hash(&mut h);
        size.hash(&mut h);
    }

    // Include count in key so a pure rename (same mtime) is still detected
    Some(format!("{:016x}-{}", h.finish(), entries.len()))
}

fn collect_yaml_stats(dir: &Path, out: &mut Vec<(String, u64, u64)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_yaml_stats(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        ) {
            if let Ok(meta) = fs::metadata(&path) {
                let mtime = meta
                    .modified()
                    .unwrap_or(SystemTime::UNIX_EPOCH)
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                out.push((path.to_string_lossy().into_owned(), mtime, meta.len()));
            }
        }
    }
}

fn cache_directory() -> PathBuf {
    // XDG_CACHE_HOME > ~/.cache > /tmp
    if let Ok(xdg) = env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("ruclei");
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".cache").join("ruclei");
    }
    PathBuf::from("/tmp/ruclei-cache")
}
