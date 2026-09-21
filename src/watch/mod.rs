use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;
use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer_opt, Debouncer, NoCache, DebouncedEvent};

#[derive(Debug, PartialEq)]
pub struct FsEvent {
    pub paths: Vec<PathBuf>,
}

pub enum Watch {
    Live(Debouncer<notify::RecommendedWatcher, NoCache>),
    Manual(String),
}

pub fn start(root: &Path, tx: Sender<FsEvent>) -> Watch {
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
    let mut debouncer = match new_debouncer_opt::<_, notify::RecommendedWatcher, NoCache>(
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
        NoCache::new(),
        notify::Config::default(),
    ) {
        Ok(d) => d,
        Err(e) => return Watch::Manual(format!("Debouncer init failed: {}", e)),
    };

    if let Err(e) = debouncer.watch(root, RecursiveMode::Recursive) {
        return Watch::Manual(format!("Watch failed: {}", e));
    }

    let tx_final = tx.clone();
    std::thread::spawn(move || {
        while let Ok(event) = rx_internal.recv() {
            if tx_final.send(event).is_err() {
                break;
            }
        }
    });

    Watch::Live(debouncer)
}

pub fn manual(reason: impl Into<String>) -> Watch {
    Watch::Manual(reason.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::sync::mpsc;
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
    /// registration cost must not scale with the number of files under the
    /// watched root (a `--all` target, or any `.kiro` root large enough or
    /// reaching a large directory via a followed symlink). Before the fix,
    /// `new_debouncer`'s default `RecommendedCache` (`FileIdMap` on
    /// non-Linux platforms) walks and `stat`s every file under the root
    /// synchronously inside `watch()` to seed a rename-id cache that this
    /// crate's `FsEvent { paths }` never consults -- so a directory with
    /// tens of thousands of files (a real `node_modules`/`target` tree)
    /// made `start()` take hundreds of ms to seconds, blocking the very
    /// first frame (`main()` calls `watch::start` before the first
    /// `terminal.draw`). This compares `start()`'s cost on a near-empty
    /// directory against a directory with 50,000 files: the *delta* must
    /// stay small regardless of machine speed.
    #[test]
    fn test_start_registration_cost_is_not_proportional_to_file_count() {
        let small = env::temp_dir().join(format!(
            "spec-viewer-watch-perf-small-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&small);
        std::fs::create_dir_all(&small).expect("create small scratch dir");
        std::fs::write(small.join("a.txt"), "").unwrap();

        let large = env::temp_dir().join(format!(
            "spec-viewer-watch-perf-large-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&large);
        std::fs::create_dir_all(&large).expect("create large scratch dir");
        const FILE_COUNT: usize = 50_000;
        for i in 0..FILE_COUNT {
            std::fs::write(large.join(format!("f{i}.txt")), "").unwrap();
        }

        let (tx_small, _rx_small) = mpsc::channel();
        let t0 = std::time::Instant::now();
        let watch_small = start(&small, tx_small);
        let small_elapsed = t0.elapsed();
        assert!(matches!(watch_small, Watch::Live(_)), "expected Watch::Live on the small dir");

        let (tx_large, _rx_large) = mpsc::channel();
        let t1 = std::time::Instant::now();
        let watch_large = start(&large, tx_large);
        let large_elapsed = t1.elapsed();
        assert!(matches!(watch_large, Watch::Live(_)), "expected Watch::Live on the large dir");

        drop(watch_small);
        drop(watch_large);
        let _ = std::fs::remove_dir_all(&small);
        let _ = std::fs::remove_dir_all(&large);

        let budget = Duration::from_millis(150);
        assert!(
            large_elapsed < small_elapsed + budget,
            "start() on a {FILE_COUNT}-file directory took {large_elapsed:?} vs {small_elapsed:?} \
             on a near-empty one -- registration must not scale with directory content \
             (regression: a file-id cache walking/stat-ing every file up front)"
        );
    }
}
