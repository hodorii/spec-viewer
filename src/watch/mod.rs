use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;
use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer_opt, Debouncer, FileIdCache, NoCache, DebouncedEvent};

#[derive(Debug, PartialEq)]
pub struct FsEvent {
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchScope {
    Recursive,
    /// Only the root is registered up front; further folders are added
    /// through `LiveWatch::sync_dirs`, so start cost ignores folder count.
    OpenedDirs,
}

pub struct LiveWatch<C: FileIdCache + Send + 'static = NoCache> {
    debouncer: Debouncer<notify::RecommendedWatcher, C>,
    scope: WatchScope,
    root: PathBuf,
    watched_dirs: BTreeSet<PathBuf>,
}

impl<C: FileIdCache + Send + 'static> LiveWatch<C> {
    pub fn scope(&self) -> WatchScope {
        self.scope
    }

    pub fn watched_dirs(&self) -> &BTreeSet<PathBuf> {
        &self.watched_dirs
    }

    /// Makes the watched folder set equal `desired` (plus the root, which is
    /// never dropped) and returns how many folders were newly registered.
    /// Only watcher calls are made; the filesystem is never written.
    pub fn sync_dirs(&mut self, desired: &BTreeSet<PathBuf>) -> usize {
        if self.scope == WatchScope::Recursive {
            return 0;
        }
        let desired: BTreeSet<PathBuf> = desired.iter().map(|dir| normalize_path(dir)).collect();

        // A folder deleted from disk is dropped by the OS watcher on its
        // own; forgetting it here is what lets a recreated folder register again.
        let stale: Vec<PathBuf> = self
            .watched_dirs
            .iter()
            .filter(|dir| **dir != self.root && (!desired.contains(*dir) || !dir.is_dir()))
            .cloned()
            .collect();
        for dir in stale {
            let _ = self.debouncer.unwatch(&dir);
            self.watched_dirs.remove(&dir);
        }

        let mut newly_registered = 0;
        for dir in desired {
            if self.watched_dirs.contains(&dir) || !dir.is_dir() {
                continue;
            }
            // A failed registration (limit hit, folder raced away) stays out
            // of the set so the next sync retries it.
            if self.debouncer.watch(&dir, RecursiveMode::NonRecursive).is_ok() {
                self.watched_dirs.insert(dir);
                newly_registered += 1;
            }
        }
        newly_registered
    }
}

pub enum Watch {
    Live(LiveWatch),
    Manual(String),
}

/// Single normalization shared by `start_scoped` and `sync_dirs` so a root
/// given as a relative path or through a symlink compares equal to itself.
fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn start(root: &Path, tx: Sender<FsEvent>) -> Watch {
    start_scoped(root, tx, WatchScope::Recursive)
}

pub fn start_scoped(root: &Path, tx: Sender<FsEvent>, scope: WatchScope) -> Watch {
    match start_with_cache(root, tx, scope, NoCache::new()) {
        Ok(live_watch) => Watch::Live(live_watch),
        Err(reason) => Watch::Manual(reason),
    }
}

fn start_with_cache<C: FileIdCache + Send + 'static>(
    root: &Path,
    tx: Sender<FsEvent>,
    scope: WatchScope,
    file_id_cache: C,
) -> Result<LiveWatch<C>, String> {
    let (tx_internal, rx_internal) = std::sync::mpsc::channel();

    let root = normalize_path(root);

    // `new_debouncer_opt` + `NoCache` instead of `new_debouncer`'s default
    // `RecommendedCache` (`FileIdMap` on non-Linux platforms, incl. macOS):
    // `FileIdMap::add_path` walks and `stat`s every file under the watched
    // root synchronously inside `watch()` below, to seed a rename-id cache
    // that only exists to stitch delete+create into a logical rename on
    // backends without rename cookies -- `FsEvent { paths }` above never
    // distinguishes rename from delete/create, so that cache is pure
    // overhead here, and it made `start()` take hundreds of ms to seconds
    // on a real `node_modules`/`target` tree (spec-viewer-watch-startup-latency
    // 1.1/1.2), blocking the first frame since `main()` calls this before
    // its first `terminal.draw`. `NoCache::add_path` is a no-op, so
    // registration cost no longer depends on directory content (2.1/2.2).
    let mut debouncer = match new_debouncer_opt::<_, notify::RecommendedWatcher, C>(
        Duration::from_millis(300),
        None,
        move |result: std::result::Result<Vec<DebouncedEvent>, Vec<notify::Error>>| {
            if let Ok(events) = result {
                let mut paths = Vec::new();
                for event in events {
                    for path in &event.paths {
                        if let Ok(canonical) = path.canonicalize() {
                            paths.push(canonical);
                        } else {
                            paths.push(path.clone());
                        }
                    }
                }
                if !paths.is_empty() {
                    let _ = tx_internal.send(FsEvent { paths });
                }
            }
        },
        file_id_cache,
        notify::Config::default(),
    ) {
        Ok(d) => d,
        Err(e) => return Err(format!("Debouncer init failed: {}", e)),
    };

    let recursive_mode = match scope {
        WatchScope::Recursive => RecursiveMode::Recursive,
        WatchScope::OpenedDirs => RecursiveMode::NonRecursive,
    };
    if let Err(e) = debouncer.watch(&root, recursive_mode) {
        return Err(format!("Watch failed: {}", e));
    }

    let tx_final = tx.clone();
    std::thread::spawn(move || {
        while let Ok(event) = rx_internal.recv() {
            if tx_final.send(event).is_err() {
                break;
            }
        }
    });

    Ok(LiveWatch {
        debouncer,
        scope,
        watched_dirs: BTreeSet::from([root.clone()]),
        root,
    })
}

pub fn manual(reason: impl Into<String>) -> Watch {
    Watch::Manual(reason.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc};
    use std::env;

    fn scratch_dir(label: &str) -> PathBuf {
        use std::sync::atomic::AtomicUsize;
        static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
        let dir = env::temp_dir().join(format!(
            "spec-viewer-watch-{label}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir.canonicalize().expect("canonicalize scratch dir")
    }

    fn live(watch: Watch) -> LiveWatch {
        match watch {
            Watch::Live(live_watch) => live_watch,
            Watch::Manual(reason) => panic!("expected Watch::Live, got Manual({reason})"),
        }
    }

    /// Writes `sentinel` after `probe` and drains events until the sentinel
    /// shows up, so "no event for probe" is proven by ordering rather than
    /// by a fixed sleep.
    fn probe_is_reported(rx: &mpsc::Receiver<FsEvent>, probe: &Path, sentinel: &Path) -> bool {
        std::fs::write(probe, "x").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        std::fs::write(sentinel, "x").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut probe_seen = false;
        let mut sentinel_seen = false;
        while std::time::Instant::now() < deadline && !sentinel_seen {
            if let Ok(event) = rx.recv_timeout(Duration::from_millis(200)) {
                probe_seen |= event.paths.iter().any(|p| p == probe);
                sentinel_seen |= event.paths.iter().any(|p| p == sentinel);
            }
        }
        assert!(sentinel_seen, "sentinel event never arrived: {}", sentinel.display());
        probe_seen
    }

    fn wait_for_path(rx: &mpsc::Receiver<FsEvent>, target: &Path) -> bool {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if let Ok(event) = rx.recv_timeout(Duration::from_millis(200)) {
                if event.paths.iter().any(|p| p == target) {
                    return true;
                }
            }
        }
        false
    }

    fn set_of(paths: &[&Path]) -> std::collections::BTreeSet<PathBuf> {
        paths.iter().map(|p| p.to_path_buf()).collect()
    }

    fn snapshot_tree(dir: &Path) -> Vec<(PathBuf, std::time::SystemTime)> {
        let mut entries = Vec::new();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(current) = pending.pop() {
            let modified = std::fs::metadata(&current).unwrap().modified().unwrap();
            entries.push((current.clone(), modified));
            if current.is_dir() {
                for entry in std::fs::read_dir(&current).unwrap() {
                    pending.push(entry.unwrap().path());
                }
            }
        }
        entries.sort();
        entries
    }

    #[test]
    fn opened_dirs_scope_reports_root_children_but_not_subdir_changes() {
        let root = scratch_dir("scope-events");
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let (tx, rx) = mpsc::channel();
        let _watch = live(start_scoped(&root, tx, WatchScope::OpenedDirs));

        let sub_file = sub.join("inner.md");
        let root_file = root.join("top.md");
        let probe_seen = probe_is_reported(&rx, &sub_file, &root_file);
        assert!(!probe_seen, "subdirectory change must not be reported under OpenedDirs");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn opened_dirs_scope_registers_only_the_root() {
        let root = scratch_dir("scope-root-only");
        std::fs::create_dir(root.join("a")).unwrap();
        let (tx, _rx) = mpsc::channel();
        let watch = live(start_scoped(&root, tx, WatchScope::OpenedDirs));
        assert_eq!(watch.watched_dirs(), &set_of(&[&root]));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn recursive_start_still_reports_subdir_changes() {
        let root = scratch_dir("recursive-events");
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let (tx, rx) = mpsc::channel();
        let _watch = live(start(&root, tx));
        let sub_file = sub.join("inner.md");
        std::fs::write(&sub_file, "x").unwrap();
        assert!(wait_for_path(&rx, &sub_file));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn opened_dirs_start_time_does_not_scale_with_subdirectory_count() {
        use std::time::Instant;
        const SUBDIR_COUNT: usize = 20_000;
        let small = scratch_dir("start-small");
        let large = scratch_dir("start-large");
        for i in 0..SUBDIR_COUNT {
            std::fs::create_dir(large.join(format!("d{i}"))).unwrap();
        }
        let (tx_small, _rx_small) = mpsc::channel();
        let small_started = Instant::now();
        let small_watch = start_scoped(&small, tx_small, WatchScope::OpenedDirs);
        let small_elapsed = small_started.elapsed();
        let (tx_large, _rx_large) = mpsc::channel();
        let large_started = Instant::now();
        let large_watch = start_scoped(&large, tx_large, WatchScope::OpenedDirs);
        let large_elapsed = large_started.elapsed();
        assert!(matches!(small_watch, Watch::Live(_)) && matches!(large_watch, Watch::Live(_)));
        drop((small_watch, large_watch));
        let _ = std::fs::remove_dir_all(&small);
        let _ = std::fs::remove_dir_all(&large);

        let allowed = (small_elapsed * 10).max(Duration::from_millis(40));
        assert!(
            large_elapsed <= allowed,
            "{SUBDIR_COUNT}-subdir start took {large_elapsed:?}, small took {small_elapsed:?}"
        );
    }

    #[test]
    fn sync_dirs_added_dir_is_reported_and_removed_dir_is_not() {
        let root = scratch_dir("sync-add-remove");
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let (tx, rx) = mpsc::channel();
        let mut watch = live(start_scoped(&root, tx, WatchScope::OpenedDirs));

        assert_eq!(watch.sync_dirs(&set_of(&[&root, &sub])), 1);
        let sub_file = sub.join("a.md");
        assert!(probe_is_reported(&rx, &sub_file, &root.join("s1.md")));

        assert_eq!(watch.sync_dirs(&set_of(&[&root])), 0);
        assert_eq!(watch.watched_dirs(), &set_of(&[&root]));
        let later_sub_file = sub.join("b.md");
        assert!(!probe_is_reported(&rx, &later_sub_file, &root.join("s2.md")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_dirs_skips_missing_dir_and_retries_on_next_call() {
        let root = scratch_dir("sync-missing");
        let later = root.join("later");
        let (tx, _rx) = mpsc::channel();
        let mut watch = live(start_scoped(&root, tx, WatchScope::OpenedDirs));
        let desired = set_of(&[&root, &later]);

        assert_eq!(watch.sync_dirs(&desired), 0);
        assert!(!watch.watched_dirs().contains(&later));

        std::fs::create_dir(&later).unwrap();
        assert_eq!(watch.sync_dirs(&desired), 1);
        assert!(watch.watched_dirs().contains(&later));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_dirs_does_not_count_root_given_as_relative_path() {
        let root = scratch_dir("sync-relative");
        let cwd = env::current_dir().unwrap().canonicalize().unwrap();
        let mut relative = PathBuf::new();
        for _ in cwd.components().skip(1) {
            relative.push("..");
        }
        relative.push(root.strip_prefix("/").unwrap());
        assert!(relative.is_relative() && relative.is_dir());

        let (tx, _rx) = mpsc::channel();
        let mut watch = live(start_scoped(&relative, tx, WatchScope::OpenedDirs));
        assert_eq!(watch.sync_dirs(&set_of(&[&relative])), 0);
        assert_eq!(watch.sync_dirs(&set_of(&[&root])), 0);
        assert_eq!(watch.watched_dirs().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn sync_dirs_does_not_count_root_given_through_symlink() {
        let real = scratch_dir("sync-symlink-real");
        let link_parent = scratch_dir("sync-symlink-link");
        let link = link_parent.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let (tx, _rx) = mpsc::channel();
        let mut watch = live(start_scoped(&link, tx, WatchScope::OpenedDirs));
        assert_eq!(watch.sync_dirs(&set_of(&[&link])), 0);
        assert_eq!(watch.sync_dirs(&set_of(&[&real, &link])), 0);
        assert_eq!(watch.watched_dirs().len(), 1);
        let _ = std::fs::remove_dir_all(&real);
        let _ = std::fs::remove_dir_all(&link_parent);
    }

    #[test]
    fn sync_dirs_keeps_root_when_desired_set_is_empty() {
        let root = scratch_dir("sync-keep-root");
        let (tx, rx) = mpsc::channel();
        let mut watch = live(start_scoped(&root, tx, WatchScope::OpenedDirs));
        assert_eq!(watch.sync_dirs(&set_of(&[])), 0);
        assert_eq!(watch.watched_dirs(), &set_of(&[&root]));
        let file = root.join("still.md");
        std::fs::write(&file, "x").unwrap();
        assert!(wait_for_path(&rx, &file));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_dirs_reregisters_dir_that_vanished_and_came_back() {
        let root = scratch_dir("sync-recreate");
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let (tx, rx) = mpsc::channel();
        let mut watch = live(start_scoped(&root, tx, WatchScope::OpenedDirs));
        let desired = set_of(&[&root, &sub]);
        assert_eq!(watch.sync_dirs(&desired), 1);

        std::fs::remove_dir_all(&sub).unwrap();
        assert_eq!(watch.sync_dirs(&desired), 0);
        assert!(!watch.watched_dirs().contains(&sub));

        std::fs::create_dir(&sub).unwrap();
        assert_eq!(watch.sync_dirs(&desired), 1);
        let file = sub.join("again.md");
        assert!(probe_is_reported(&rx, &file, &root.join("s.md")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_dirs_is_a_noop_for_recursive_scope() {
        let root = scratch_dir("sync-recursive-noop");
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        let (tx, _rx) = mpsc::channel();
        let mut watch = live(start(&root, tx));
        let before = watch.watched_dirs().clone();
        assert_eq!(watch.sync_dirs(&set_of(&[&root, &sub])), 0);
        assert_eq!(watch.watched_dirs(), &before);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_dirs_does_not_write_to_the_filesystem() {
        let root = scratch_dir("sync-no-write");
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("f.md"), "x").unwrap();
        let (tx, _rx) = mpsc::channel();
        let mut watch = live(start_scoped(&root, tx, WatchScope::OpenedDirs));
        let before = snapshot_tree(&root);
        watch.sync_dirs(&set_of(&[&root, &sub]));
        watch.sync_dirs(&set_of(&[&root]));
        assert_eq!(snapshot_tree(&root), before);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_start_live_event() {
        let temp_dir = env::temp_dir().join(format!("spec-viewer-test-{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
        let root = &temp_dir;
        let (tx, rx) = mpsc::channel();
        
        let _watch = start(root, tx);
        
        let file_path = root.join("test.md");
        {
            let mut f = File::create(&file_path).expect("Failed to create file");
            f.write_all(b"hello").expect("Failed to write");
        }

        let event = rx.recv_timeout(Duration::from_secs(2))
            .expect("Timed out waiting for FsEvent");
        
        let expected_path = file_path.canonicalize().expect("Failed to canonicalize expected path");
        assert!(event.paths.iter().any(|p| p == &expected_path));
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_start_non_existent_path() {
        let root = Path::new("/non/existent/path/spec-viewer-test-123");
        let (tx, _rx) = mpsc::channel();

        let watch = start(root, tx);

        if let Watch::Manual(reason) = watch {
            assert!(reason.contains("Watch failed"));
        } else {
            panic!("Expected Watch::Manual for non-existent path");
        }
    }

    /// spec-viewer-watch-startup-latency 1.1/1.2 -> 2.1/2.2: `start()`'s
    /// registration must not scale with the number of files under the
    /// watched root. A default `FileIdMap` walks and `stat`s every file
    /// inside its `add_path`; this crate never consults rename ids, so
    /// `start()` pins `NoCache`. The count-based check below is
    /// deterministic: registering a many-file directory asks the cache to
    /// add exactly one path (the root), never one per file.
    #[test]
    fn test_start_registration_cost_is_not_proportional_to_file_count() {
        struct CountingCache {
            added_path_count: Arc<AtomicUsize>,
        }
        impl FileIdCache for CountingCache {
            fn cached_file_id(&self, _path: &Path) -> Option<impl AsRef<notify_debouncer_full::file_id::FileId>> {
                None::<notify_debouncer_full::file_id::FileId>
            }
            fn add_path(&mut self, _path: &Path, _recursive_mode: RecursiveMode) {
                self.added_path_count.fetch_add(1, Ordering::SeqCst);
            }
            fn remove_path(&mut self, _path: &Path) {}
            fn rescan(&mut self, _roots: &[(PathBuf, RecursiveMode)]) {}
        }

        let large = env::temp_dir().join(format!(
            "spec-viewer-watch-perf-large-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&large);
        std::fs::create_dir_all(&large).expect("create large scratch dir");
        const FILE_COUNT: usize = 2_000;
        for i in 0..FILE_COUNT {
            std::fs::write(large.join(format!("f{i}.txt")), "").unwrap();
        }

        let added_path_count = Arc::new(AtomicUsize::new(0));
        let (tx, _rx) = mpsc::channel();
        let watch = start_with_cache(
            &large,
            tx,
            WatchScope::Recursive,
            CountingCache { added_path_count: Arc::clone(&added_path_count) },
        );
        assert!(watch.is_ok(), "expected the large directory to register");
        let added = added_path_count.load(Ordering::SeqCst);

        drop(watch);
        let _ = std::fs::remove_dir_all(&large);

        assert!(
            added <= 1,
            "registering a {FILE_COUNT}-file directory called add_path {added} times -- \
             registration must not scale with directory content"
        );
    }
}
