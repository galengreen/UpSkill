//! Content-addressed object store: blobs and versions. Objects are immutable,
//! which is what makes sync conflict-free at the file level.

use crate::model::Version;
use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

const IGNORED: &[&str] = &[".DS_Store", ".git", "Thumbs.db", "node_modules", "__pycache__"];
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn is_ignored(name: &str) -> bool {
    IGNORED.contains(&name) || name.starts_with(".upskill")
}

/// Write via a temp file + rename so readers never see partial content.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub struct Store<'a> {
    pub paths: &'a Paths,
}

impl<'a> Store<'a> {
    pub fn new(paths: &'a Paths) -> Self {
        Store { paths }
    }

    fn blob_path(&self, hash: &str) -> PathBuf {
        self.paths.blobs().join(&hash[..2]).join(hash)
    }
    fn version_path(&self, id: &str) -> PathBuf {
        self.paths.versions().join(format!("{id}.json"))
    }

    pub fn put_blob(&self, bytes: &[u8]) -> Result<String> {
        let hash = sha256_hex(bytes);
        let p = self.blob_path(&hash);
        if !p.exists() {
            write_atomic(&p, bytes)?;
        }
        Ok(hash)
    }

    pub fn get_blob(&self, hash: &str) -> Result<Vec<u8>> {
        fs::read(self.blob_path(hash)).with_context(|| format!("missing blob {hash}"))
    }

    pub fn has_blob(&self, hash: &str) -> bool {
        self.blob_path(hash).exists()
    }

    pub fn put_version(&self, v: &Version) -> Result<String> {
        let bytes = serde_json::to_vec(v)?;
        let id = sha256_hex(&bytes)[..20].to_string();
        let p = self.version_path(&id);
        if !p.exists() {
            write_atomic(&p, &bytes)?;
        }
        Ok(id)
    }

    pub fn get_version(&self, id: &str) -> Result<Version> {
        let bytes = fs::read(self.version_path(id)).with_context(|| format!("missing version {id}"))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn has_version(&self, id: &str) -> bool {
        self.version_path(id).exists()
    }

    /// Hash every file under `dir` (relative paths, `/` separators).
    pub fn hash_dir(&self, dir: &Path, store_blobs: bool) -> Result<BTreeMap<String, String>> {
        let mut out = BTreeMap::new();
        if !dir.is_dir() {
            return Ok(out);
        }
        let walker = walkdir::WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| !is_ignored(&e.file_name().to_string_lossy()));
        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.metadata().map(|m| m.len() > MAX_FILE_BYTES).unwrap_or(true) {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(dir)?
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let bytes = fs::read(entry.path())?;
            let hash = if store_blobs { self.put_blob(&bytes)? } else { sha256_hex(&bytes) };
            out.insert(rel, hash);
        }
        Ok(out)
    }

    /// Replace the contents of `dir` with the files of a version.
    pub fn materialise(&self, files: &BTreeMap<String, String>, dir: &Path) -> Result<()> {
        let parent = dir.parent().context("no parent")?;
        fs::create_dir_all(parent)?;
        let tmp = parent.join(format!(".upskill-tmp-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&tmp)?;
        for (rel, hash) in files {
            if rel.split('/').any(|seg| seg == ".." || seg.is_empty()) {
                bail!("unsafe path in version: {rel}");
            }
            let target = tmp.join(rel);
            if let Some(p) = target.parent() {
                fs::create_dir_all(p)?;
            }
            fs::write(&target, self.get_blob(hash)?)?;
            #[cfg(unix)]
            if rel.starts_with("scripts/") || rel.ends_with(".sh") || rel.ends_with(".py") {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&target, fs::Permissions::from_mode(0o755));
            }
        }
        if dir.exists() {
            let old = parent.join(format!(".upskill-old-{}", uuid::Uuid::new_v4().simple()));
            fs::rename(dir, &old)?;
            fs::rename(&tmp, dir)?;
            let _ = fs::remove_dir_all(&old);
        } else {
            fs::rename(&tmp, dir)?;
        }
        Ok(())
    }

    /// True if `ancestor` is `descendant` or reachable through its parents.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([descendant.to_string()]);
        while let Some(id) = queue.pop_front() {
            if id == ancestor {
                return true;
            }
            if !seen.insert(id.clone()) || seen.len() > 5000 {
                continue;
            }
            if let Ok(v) = self.get_version(&id) {
                queue.extend(v.parents);
            }
        }
        false
    }

    /// Every version reachable from `head`, newest first.
    pub fn history(&self, head: &str) -> Vec<(String, Version)> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        let mut queue = VecDeque::from([head.to_string()]);
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id.clone()) || out.len() > 1000 {
                continue;
            }
            if let Ok(v) = self.get_version(&id) {
                queue.extend(v.parents.clone());
                out.push((id, v));
            }
        }
        out.sort_by(|a, b| b.1.ts.cmp(&a.1.ts));
        out
    }

    pub fn list_ids(dir: &Path, nested: bool) -> Vec<(String, PathBuf)> {
        let mut out = Vec::new();
        let Ok(rd) = fs::read_dir(dir) else { return out };
        for e in rd.flatten() {
            let p = e.path();
            if nested && p.is_dir() {
                out.extend(Self::list_ids(&p, false));
            } else if p.is_file() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.contains(".tmp-") {
                    continue;
                }
                out.push((name, p));
            }
        }
        out
    }
}

/// Copy every object present in `from` but missing in `to`. Returns count copied.
pub fn copy_missing_objects(from: &Path, to: &Path) -> Result<usize> {
    let mut n = 0;
    for (sub, nested) in [("blobs", true), ("versions", false)] {
        for (name, src) in Store::list_ids(&from.join(sub), nested) {
            let rel = src.strip_prefix(from)?;
            let dst = to.join(rel);
            if !dst.exists() {
                write_atomic(&dst, &fs::read(&src)?)?;
                n += 1;
            }
            let _ = name;
        }
    }
    Ok(n)
}

pub fn tree_hash(files: &BTreeMap<String, String>) -> String {
    sha256_hex(serde_json::to_string(files).unwrap_or_default().as_bytes())
}
