use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use mpq::Archive;

/// Deterministic MPQ reader with byte caching.
///
/// We prioritize patch archives before base archives by sorting on file name
/// and scanning in reverse lexical order.
pub struct ArchiveSet {
    root: PathBuf,
    archives: Vec<PathBuf>,
    cache: RefCell<HashMap<String, Vec<u8>>>,
}

impl ArchiveSet {
    pub fn open(root: &Path) -> Result<Self> {
        let mut archives = Vec::new();
        for entry in
            fs::read_dir(root).with_context(|| format!("read dir {}", root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let is_mpq = path
                .extension()
                .map(|ext| ext.to_string_lossy().to_ascii_lowercase() == "mpq")
                .unwrap_or(false);
            if is_mpq {
                archives.push(path);
            }
        }

        archives.sort_by_key(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default()
        });
        archives.reverse();

        Ok(Self {
            root: root.to_path_buf(),
            archives,
            cache: RefCell::new(HashMap::new()),
        })
    }

    pub fn has_mpq_hint(&self) -> bool {
        !self.archives.is_empty()
    }

    pub fn archives(&self) -> &[PathBuf] {
        &self.archives
    }

    pub fn read_file(&self, file_path: &str) -> Result<Vec<u8>> {
        let normalized = file_path.replace('/', "\\").to_ascii_lowercase();
        if let Some(cached) = self.cache.borrow().get(&normalized) {
            return Ok(cached.clone());
        }

        for archive_path in &self.archives {
            let mut archive = Archive::open(archive_path)
                .with_context(|| format!("open MPQ {}", archive_path.display()))?;
            match archive.open_file(&normalized) {
                Ok(file) => {
                    let mut out = vec![0u8; file.size() as usize];
                    file.read(&mut archive, &mut out).with_context(|| {
                        format!("read {normalized} from {}", archive_path.display())
                    })?;
                    self.cache.borrow_mut().insert(normalized.clone(), out.clone());
                    return Ok(out);
                }
                Err(_) => continue,
            }
        }

        bail!(
            "file not found in MPQ set rooted at {}: {}",
            self.root.display(),
            normalized
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn open_empty_dir_yields_no_archives() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let tmp = std::env::temp_dir().join(format!("wme_archive_set_{nanos}"));
        fs::create_dir_all(&tmp).expect("create temp dir");
        let set = ArchiveSet::open(&tmp).expect("open archive set");
        assert!(set.archives().is_empty());
        fs::remove_dir_all(&tmp).expect("cleanup");
    }
}
