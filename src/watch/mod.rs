use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;
use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer_opt, Debouncer, FileIdCache, NoCache, DebouncedEvent};

#[derive(Debug, PartialEq)]
pub struct FsEvent {
    pub paths: Vec<PathBuf>,
}

pub enum Watch {
    Live(Debouncer<notify::RecommendedWatcher, NoCache>),
    Manual(String),
}

pub fn start(root: &Path, tx: Sender<FsEvent>) -> Watch {
    match start_with_cache(root, tx, NoCache::new()) {
        Ok(debouncer) => Watch::Live(debouncer),
        Err(reason) => Watch::Manual(reason),
    }
}

fn start_with_cache<C: FileIdCache + Send + 'static>(
    root: &Path,
    tx: Sender<FsEvent>,
    file_id_cache: C,
) -> Result<Debouncer<notify::RecommendedWatcher, C>, String> {
    let (tx_internal, rx_internal) = std::sync::mpsc::channel();
    
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    
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

    if let Err(e) = debouncer.watch(root, RecursiveMode::Recursive) {
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

    Ok(debouncer)
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
