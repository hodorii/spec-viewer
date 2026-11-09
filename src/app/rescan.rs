//! Runs tree rescans off the input thread. At most one scan is in flight for
//! the current generation; requests arriving meanwhile collapse into a single
//! follow-up scan. Changing the generation abandons whatever is running.

use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use crate::spec::{SortKey, TreeSource, TreeSourceKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RescanRequest {
    pub kind: TreeSourceKind,
    pub root: PathBuf,
    pub sort_key: SortKey,
}

/// Name of the scan threads, so the process panic hook can recognise them.
pub const SCAN_THREAD_NAME: &str = "rescan";

type ScanFn = Arc<dyn Fn(&RescanRequest) -> TreeSource + Send + Sync>;
type SpawnFn = Box<dyn Fn(Box<dyn FnOnce() + Send>) -> io::Result<()>>;

/// `None` means the scan thread died before producing a tree.
type ScanOutcome = (u64, Option<TreeSource>);

pub struct RescanCoordinator {
    generation: u64,
    is_scan_in_flight: bool,
    pending_request: Option<RescanRequest>,
    /// Result of a scan run on the caller's thread because no thread could
    /// be created; handed out by the next `poll`.
    inline_result: Option<TreeSource>,
    outcome_sender: Sender<ScanOutcome>,
    outcome_receiver: Receiver<ScanOutcome>,
    scan: ScanFn,
    spawn: SpawnFn,
}

impl Default for RescanCoordinator {
    fn default() -> Self {
        Self::with_scan(Arc::new(|request: &RescanRequest| {
            super::scan_tree_source(request.kind, &request.root, request.sort_key)
        }))
    }
}

impl RescanCoordinator {
    pub fn with_scan(scan: ScanFn) -> Self {
        Self::with_scan_and_spawn(
            scan,
            Box::new(|job| {
                thread::Builder::new().name(SCAN_THREAD_NAME.into()).spawn(job).map(|_| ())
            }),
        )
    }

    fn with_scan_and_spawn(scan: ScanFn, spawn: SpawnFn) -> Self {
        let (outcome_sender, outcome_receiver) = channel();
        Self {
            generation: 0,
            is_scan_in_flight: false,
            pending_request: None,
            inline_result: None,
            outcome_sender,
            outcome_receiver,
            scan,
            spawn,
        }
    }

    pub fn request(&mut self, request: RescanRequest) {
        if self.is_scan_in_flight {
            self.pending_request = Some(request);
        } else {
            self.start_scan(request);
        }
    }

    /// Abandon the running scan's result and any queued follow-up. The old
    /// thread is not waited for, so a new request may start right away.
    pub fn invalidate(&mut self) {
        self.generation += 1;
        self.is_scan_in_flight = false;
        self.pending_request = None;
        self.inline_result = None;
    }

    pub fn is_scanning(&self) -> bool {
        self.is_scan_in_flight
    }

    pub fn poll(&mut self) -> Option<TreeSource> {
        let mut finished_tree = None;
        while let Ok((generation, outcome)) = self.outcome_receiver.try_recv() {
            if generation != self.generation {
                continue;
            }
            self.is_scan_in_flight = false;
            finished_tree = outcome;
        }
        if !self.is_scan_in_flight {
            if let Some(request) = self.pending_request.take() {
                self.start_scan(request);
            }
        }
        finished_tree.or_else(|| self.inline_result.take())
    }

    fn start_scan(&mut self, request: RescanRequest) {
        let generation = self.generation;
        let scan = Arc::clone(&self.scan);
        let sender = self.outcome_sender.clone();
        let scan_request = request.clone();
        let job = Box::new(move || {
            let tree = run_guarded(&scan, &scan_request);
            // The coordinator may have been dropped; nobody wants the tree then.
            let _ = sender.send((generation, tree));
        });
        match (self.spawn)(job) {
            Ok(()) => self.is_scan_in_flight = true,
            // Better a brief stall than a tree that never refreshes.
            Err(_) => self.inline_result = run_guarded(&self.scan, &request),
        }
    }
}

/// A panicking scan must cost one refresh, never the program.
fn run_guarded(scan: &ScanFn, request: &RescanRequest) -> Option<TreeSource> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scan(request))).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, Barrier};

    fn request(kind: TreeSourceKind) -> RescanRequest {
        RescanRequest { kind, root: PathBuf::from("/unused"), sort_key: SortKey::Name }
    }

    fn tree_for(kind: TreeSourceKind) -> TreeSource {
        match kind {
            TreeSourceKind::SpecKit => TreeSource::SpecKit(Vec::new()),
            TreeSourceKind::Files => TreeSource::Files(crate::spec::FsTree {
                root: PathBuf::new(),
                entries: Vec::new(),
            }),
            TreeSourceKind::Kiro => TreeSource::Kiro(crate::spec::SpecRoot {
                specs: Vec::new(),
                groups: Vec::new(),
            }),
        }
    }

    /// Coordinator whose threads are kept so tests can join them instead of
    /// waiting on the clock.
    struct Harness {
        coordinator: RescanCoordinator,
        threads: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
    }

    impl Harness {
        fn new(scan: ScanFn) -> Self {
            let threads: Arc<Mutex<Vec<thread::JoinHandle<()>>>> = Arc::default();
            let kept = Arc::clone(&threads);
            let coordinator = RescanCoordinator::with_scan_and_spawn(
                scan,
                Box::new(move |job| {
                    kept.lock().unwrap().push(thread::spawn(job));
                    Ok(())
                }),
            );
            Self { coordinator, threads }
        }

        fn join_threads(&self) {
            let handles: Vec<_> = self.threads.lock().unwrap().drain(..).collect();
            for handle in handles {
                handle.join().unwrap();
            }
        }
    }

    fn counting_scan(runs: Arc<AtomicUsize>) -> ScanFn {
        Arc::new(move |request| {
            runs.fetch_add(1, Ordering::SeqCst);
            tree_for(request.kind)
        })
    }

    #[test]
    fn requests_during_a_scan_collapse_into_one_more_run() {
        let runs = Arc::new(AtomicUsize::new(0));
        let mut harness = Harness::new(counting_scan(Arc::clone(&runs)));
        for _ in 0..5 {
            harness.coordinator.request(request(TreeSourceKind::Files));
        }
        harness.join_threads();
        assert_eq!(runs.load(Ordering::SeqCst), 1, "only the first request starts a scan");

        assert!(harness.coordinator.poll().is_some());
        harness.join_threads();
        assert!(harness.coordinator.poll().is_some());
        harness.join_threads();
        assert!(harness.coordinator.poll().is_none());
        assert_eq!(runs.load(Ordering::SeqCst), 2, "five requests, one initial run plus one follow-up");
        assert!(!harness.coordinator.is_scanning());
    }

    #[test]
    fn a_result_from_before_invalidate_is_never_returned() {
        let mut harness = Harness::new(counting_scan(Arc::default()));
        harness.coordinator.request(request(TreeSourceKind::Files));
        harness.coordinator.invalidate();
        harness.join_threads();
        assert!(harness.coordinator.poll().is_none());
        assert!(!harness.coordinator.is_scanning());
    }

    #[test]
    fn invalidate_drops_the_queued_follow_up() {
        let runs = Arc::new(AtomicUsize::new(0));
        let mut harness = Harness::new(counting_scan(Arc::clone(&runs)));
        harness.coordinator.request(request(TreeSourceKind::Files));
        harness.coordinator.request(request(TreeSourceKind::Files));
        harness.coordinator.invalidate();
        harness.join_threads();
        assert!(harness.coordinator.poll().is_none());
        harness.join_threads();
        assert_eq!(runs.load(Ordering::SeqCst), 1, "the queued request must not run");
    }

    #[test]
    fn a_new_generation_starts_without_waiting_for_the_abandoned_scan() {
        let started = Arc::new(Barrier::new(2));
        let release_old = Arc::new(Barrier::new(2));
        let calls = Arc::new(AtomicUsize::new(0));
        let scan: ScanFn = {
            let (started, release_old, calls) =
                (Arc::clone(&started), Arc::clone(&release_old), Arc::clone(&calls));
            Arc::new(move |request| {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    started.wait();
                    release_old.wait();
                }
                tree_for(request.kind)
            })
        };
        let mut harness = Harness::new(scan);
        harness.coordinator.request(request(TreeSourceKind::Files));
        started.wait();
        harness.coordinator.invalidate();
        harness.coordinator.request(request(TreeSourceKind::SpecKit));
        // The abandoned scan is still blocked, yet the new one has its own thread.
        let new_thread = harness.threads.lock().unwrap().pop().unwrap();
        new_thread.join().unwrap();
        let tree = harness.coordinator.poll().expect("new generation result");
        assert_eq!(tree.kind(), TreeSourceKind::SpecKit);
        release_old.wait();
        harness.join_threads();
        assert!(harness.coordinator.poll().is_none(), "the old Files result must be discarded");
    }

    #[test]
    fn a_panicking_scan_yields_nothing_and_the_next_request_runs() {
        let calls = Arc::new(AtomicUsize::new(0));
        let scan: ScanFn = {
            let calls = Arc::clone(&calls);
            Arc::new(move |request| {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("scan failed on purpose");
                }
                tree_for(request.kind)
            })
        };
        let mut harness = Harness::new(scan);
        harness.coordinator.request(request(TreeSourceKind::Files));
        harness.join_threads();
        assert!(harness.coordinator.poll().is_none());
        assert!(!harness.coordinator.is_scanning(), "a dead scan must not block later ones");

        harness.coordinator.request(request(TreeSourceKind::Files));
        harness.join_threads();
        assert!(harness.coordinator.poll().is_some());
    }

    #[test]
    fn at_most_one_scan_runs_at_a_time_within_a_generation() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let scan: ScanFn = {
            let (active, max_active) = (Arc::clone(&active), Arc::clone(&max_active));
            Arc::new(move |request| {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_active.fetch_max(now, Ordering::SeqCst);
                let tree = tree_for(request.kind);
                active.fetch_sub(1, Ordering::SeqCst);
                tree
            })
        };
        let mut harness = Harness::new(scan);
        for _ in 0..4 {
            harness.coordinator.request(request(TreeSourceKind::Files));
            harness.coordinator.request(request(TreeSourceKind::Files));
            harness.join_threads();
            harness.coordinator.poll();
            harness.join_threads();
            harness.coordinator.poll();
        }
        assert_eq!(max_active.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failing_to_create_a_thread_falls_back_to_a_synchronous_scan() {
        let runs = Arc::new(AtomicUsize::new(0));
        let mut coordinator = RescanCoordinator::with_scan_and_spawn(
            counting_scan(Arc::clone(&runs)),
            Box::new(|_| Err(io::Error::other("no threads"))),
        );
        coordinator.request(request(TreeSourceKind::SpecKit));
        assert!(!coordinator.is_scanning());
        let tree = coordinator.poll().expect("synchronous fallback result");
        assert_eq!(tree.kind(), TreeSourceKind::SpecKit);
        assert!(coordinator.poll().is_none(), "the fallback result is handed out once");
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_default_scan_reads_the_real_tree() {
        let dir = std::env::temp_dir().join(format!("spec_viewer_rescan_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "# a").unwrap();
        let default_scan = RescanCoordinator::default().scan;
        let mut harness = Harness::new(default_scan);
        harness.coordinator.request(RescanRequest {
            kind: TreeSourceKind::Files,
            root: dir.clone(),
            sort_key: SortKey::Name,
        });
        harness.join_threads();
        let tree = harness.coordinator.poll().expect("scan result");
        let _ = std::fs::remove_dir_all(&dir);
        match tree {
            TreeSource::Files(files) => assert!(!files.entries.is_empty()),
            _ => panic!("expected a Files tree"),
        }
    }

    fn spawn_always_fails() -> SpawnFn {
        Box::new(|_| Err(io::Error::other("no threads")))
    }

    #[test]
    fn a_panicking_synchronous_fallback_scan_yields_nothing_and_does_not_unwind() {
        let calls = Arc::new(AtomicUsize::new(0));
        let scan: ScanFn = {
            let calls = Arc::clone(&calls);
            Arc::new(move |request| {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("fallback scan failed on purpose");
                }
                tree_for(request.kind)
            })
        };
        let mut coordinator = RescanCoordinator::with_scan_and_spawn(scan, spawn_always_fails());
        coordinator.request(request(TreeSourceKind::Files));
        assert!(!coordinator.is_scanning());
        assert!(coordinator.poll().is_none());

        coordinator.request(request(TreeSourceKind::Files));
        assert!(coordinator.poll().is_some(), "the next request must work normally");
    }

    #[test]
    fn invalidate_discards_a_synchronous_fallback_result() {
        let mut coordinator = RescanCoordinator::with_scan_and_spawn(
            counting_scan(Arc::default()),
            spawn_always_fails(),
        );
        coordinator.request(request(TreeSourceKind::Files));
        coordinator.invalidate();
        assert!(coordinator.poll().is_none());
    }
}
