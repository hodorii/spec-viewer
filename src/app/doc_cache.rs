//! Reuses rendered documents when the same file is selected again at the same
//! width, so re-selection skips the read and the markdown render.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::loader;
use super::DocView;

/// Entries kept before the least recently used one is dropped.
const DOC_CACHE_CAPACITY: usize = 16;

type DocReader = Box<dyn FnMut(&Path, u16) -> DocView + Send>;

/// Size and modification time are both part of the key: an edit within the
/// timestamp's resolution still changes the size most of the time, and a
/// same-size edit still moves the timestamp.
#[derive(Clone, PartialEq, Eq)]
struct DocCacheKey {
    path: PathBuf,
    modified: SystemTime,
    size: u64,
    width: u16,
}

pub struct DocCache {
    /// Least recently used first.
    entries: Vec<(DocCacheKey, DocView)>,
    read_doc: DocReader,
}

impl Default for DocCache {
    fn default() -> Self {
        Self::with_reader(Box::new(loader::load_doc))
    }
}

impl DocCache {
    /// The reader is injectable so tests can count how often the file is
    /// really read instead of timing it.
    pub fn with_reader(read_doc: DocReader) -> Self {
        Self { entries: Vec::new(), read_doc }
    }

    /// Forget every width of `path` so the next load cannot serve text that a
    /// same-size, same-timestamp edit made stale.
    pub fn invalidate(&mut self, path: &Path) {
        self.entries.retain(|(key, _)| key.path != path);
    }

    pub fn load(&mut self, path: &Path, width: u16) -> DocView {
        let Ok(stat) = fs::metadata(path) else {
            self.entries.retain(|(key, _)| key.path != path);
            return (self.read_doc)(path, width);
        };
        let key = DocCacheKey {
            path: path.to_path_buf(),
            modified: stat.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            size: stat.len(),
            width,
        };

        if let Some(index) = self.entries.iter().position(|(cached, _)| *cached == key) {
            let entry = self.entries.remove(index);
            let view = entry.1.clone();
            self.entries.push(entry);
            return view;
        }

        // A stale entry for the same path can never match again.
        self.entries.retain(|(cached, _)| !(cached.path == key.path && cached.width == width));
        let view = (self.read_doc)(path, width);
        if matches!(view, DocView::Rendered { .. }) {
            if self.entries.len() >= DOC_CACHE_CAPACITY {
                self.entries.remove(0);
            }
            self.entries.push((key, view.clone()));
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("spec_viewer_doc_cache_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn counting_cache() -> (DocCache, Arc<AtomicUsize>) {
        let reads = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&reads);
        let cache = DocCache::with_reader(Box::new(move |path, width| {
            counter.fetch_add(1, Ordering::SeqCst);
            loader::load_doc(path, width)
        }));
        (cache, reads)
    }

    fn write_doc(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        path
    }

    fn set_modified(path: &Path, modified: SystemTime) {
        fs::File::options().write(true).open(path).unwrap().set_modified(modified).unwrap();
    }

    #[test]
    fn second_load_of_same_path_and_width_does_not_read() {
        let dir = scratch_dir("hit");
        let path = write_doc(&dir, "a.md", "# A\n\nbody\n");
        let (mut cache, reads) = counting_cache();

        let first = cache.load(&path, 80);
        let second = cache.load(&path, 80);

        assert_eq!(reads.load(Ordering::SeqCst), 1);
        match (first, second) {
            (
                DocView::Rendered { r: first_r, .. },
                DocView::Rendered { r: second_r, .. },
            ) => assert_eq!(first_r.plain, second_r.plain),
            _ => panic!("expected rendered views"),
        }
    }

    #[test]
    fn cache_hit_meta_equals_freshly_read_meta() {
        let dir = scratch_dir("meta");
        let path = write_doc(&dir, "a.md", "# A\n\nline\nline\n");
        let (mut cache, _) = counting_cache();

        cache.load(&path, 80);
        let hit = cache.load(&path, 80);
        let fresh = loader::load_doc(&path, 80);

        match (hit, fresh) {
            (DocView::Rendered { meta: hit_meta, .. }, DocView::Rendered { meta: fresh_meta, .. }) => {
                assert_eq!(hit_meta, fresh_meta)
            }
            _ => panic!("expected rendered views"),
        }
    }

    #[test]
    fn changed_modified_time_rereads() {
        let dir = scratch_dir("mtime");
        let path = write_doc(&dir, "a.md", "# A\n");
        let (mut cache, reads) = counting_cache();
        cache.load(&path, 80);

        set_modified(&path, SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000));
        cache.load(&path, 80);

        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn changed_size_with_same_modified_time_rereads() {
        let dir = scratch_dir("size");
        let path = write_doc(&dir, "a.md", "# A\n");
        let pinned = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000);
        set_modified(&path, pinned);
        let (mut cache, reads) = counting_cache();
        cache.load(&path, 80);

        fs::write(&path, "# A\n\nmore text\n").unwrap();
        set_modified(&path, pinned);
        cache.load(&path, 80);

        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn changed_width_rereads() {
        let dir = scratch_dir("width");
        let path = write_doc(&dir, "a.md", "# A\n");
        let (mut cache, reads) = counting_cache();
        cache.load(&path, 80);

        cache.load(&path, 60);

        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn deleted_file_is_missing_and_not_cached() {
        let dir = scratch_dir("deleted");
        let path = write_doc(&dir, "a.md", "# A\n");
        let (mut cache, reads) = counting_cache();
        cache.load(&path, 80);

        fs::remove_file(&path).unwrap();
        let view = cache.load(&path, 80);

        assert!(matches!(view, DocView::Missing(_)));
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        cache.load(&path, 80);
        assert_eq!(reads.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn seventeenth_entry_evicts_least_recently_used() {
        let dir = scratch_dir("lru");
        let paths: Vec<PathBuf> =
            (0..=DOC_CACHE_CAPACITY).map(|i| write_doc(&dir, &format!("{i}.md"), "# T\n")).collect();
        let (mut cache, reads) = counting_cache();
        for path in &paths[..DOC_CACHE_CAPACITY] {
            cache.load(path, 80);
        }
        // Touching the oldest makes the second one the least recently used.
        cache.load(&paths[0], 80);
        cache.load(&paths[DOC_CACHE_CAPACITY], 80);
        let reads_before = reads.load(Ordering::SeqCst);

        cache.load(&paths[0], 80);
        assert_eq!(reads.load(Ordering::SeqCst), reads_before, "recently used entry stays");
        cache.load(&paths[1], 80);
        assert_eq!(reads.load(Ordering::SeqCst), reads_before + 1, "oldest untouched entry was evicted");
    }

    #[test]
    fn evicts_the_first_inserted_when_nothing_was_reused() {
        let dir = scratch_dir("lru_plain");
        let paths: Vec<PathBuf> =
            (0..=DOC_CACHE_CAPACITY).map(|i| write_doc(&dir, &format!("{i}.md"), "# T\n")).collect();
        let (mut cache, reads) = counting_cache();
        for path in &paths {
            cache.load(path, 80);
        }
        assert_eq!(reads.load(Ordering::SeqCst), DOC_CACHE_CAPACITY + 1);

        cache.load(&paths[0], 80);

        assert_eq!(reads.load(Ordering::SeqCst), DOC_CACHE_CAPACITY + 2);
    }
}
