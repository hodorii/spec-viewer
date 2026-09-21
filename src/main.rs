//! Entry point (design.md "File Structure Plan" -> `main.rs # Entry,
//! Terminal init/restore"; requirements 1.1-1.5, 8.1, 8.2, 8.6).
//!
//! Architecture note (task 7.1): real terminal I/O (`crossterm::event::poll`
//! / `read` against the actual tty, and `ratatui::init`/`restore` toggling
//! real raw-mode/alt-screen state) cannot be meaningfully unit-tested --
//! `TestBackend` has no raw-mode/alt-screen concept to assert against, and a
//! blocking read against real stdin has no place in an automated suite.
//! `main()` itself is therefore a thin, deliberately untested wrapper. The
//! two pieces that *are* testable are pulled out as free functions:
//!
//! - [`resolve_startup`]: pure path/validation logic (root discovery, `--log`
//!   rejection), no terminal or event-loop I/O.
//! - [`step`]: one reducer-plus-render iteration, generic over
//!   `ratatui::backend::Backend` so tests can drive it with `TestBackend`.
//!
//! "실행 -> 트리 표시 -> q 종료 후 터미널 복원" end-to-end, against a real
//! terminal, is verified by a separate manual check outside this test suite
//! (see this task's Status Report CONCERNS).

use std::path::PathBuf;
use clap::{Parser, ValueEnum};

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeMode {
    Always,
    Auto,
    Hidden,
    Single,
}

/// `--sort` starting value (requirement 2.9). Mirrors
/// `spec_viewer::spec::SortKey` one-for-one -- kept as its own CLI-facing
/// enum for the same reason `TreeMode` above is, rather than deriving
/// `ValueEnum` on the library type directly.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortArg {
    Name,
    Phase,
    Updated,
    Progress,
}

impl From<SortArg> for spec_viewer::spec::SortKey {
    fn from(arg: SortArg) -> Self {
        match arg {
            SortArg::Name => spec_viewer::spec::SortKey::Name,
            SortArg::Phase => spec_viewer::spec::SortKey::Phase,
            SortArg::Updated => spec_viewer::spec::SortKey::Updated,
            SortArg::Progress => spec_viewer::spec::SortKey::Progress,
        }
    }
}

/// Default mermaid graph engine: `dg` (~/tools/dg, on by default via the
/// `engine-dg` feature), otherwise `mdview`.
#[cfg(feature = "engine-dg")]
const DEFAULT_DIAGRAM_ENGINE: &str = "dg";
#[cfg(not(feature = "engine-dg"))]
const DEFAULT_DIAGRAM_ENGINE: &str = "mdview";

#[derive(Parser, Debug)]
#[command(name = "m", about = "Spec Viewer CLI")]
pub struct Args {
    /// Optional starting path; defaults to the current directory (1.1, 1.2).
    /// Under `--all`, this is the directory to scan instead of a `.kiro`
    /// root (requirement 1.8).
    path: Option<PathBuf>,
    /// Browse any directory of markdown files, without a `.kiro` root
    /// (requirement 1.8): folders fold/unfold, files render on selection,
    /// no spec badges/progress/definition (requirement 1.9). `1.3`'s root-
    /// search failure does not apply in this mode.
    #[arg(long = "all")]
    all: bool,
    /// Disable filesystem watching (requirement 7.6).
    #[arg(long = "no-watch")]
    no_watch: bool,
    /// Optional log file path. Rejected if it resolves inside the found
    /// `.kiro` root (requirement 8.2).
    #[arg(long = "log")]
    log: Option<PathBuf>,
    /// Tree panel visibility mode.
    #[arg(long, value_enum, default_value = "auto")]
    tree: TreeMode,
    /// Spec tree starting sort key (requirement 2.9); the `s` hotkey cycles
    /// through the same four values at runtime.
    #[arg(long, value_enum, default_value = "name")]
    sort: SortArg,
    /// Mermaid graph-diagram engine (design.md "Pluggable GraphEngine").
    /// `dg` (~/tools/dg, feature `engine-dg`, on by default) is the default; `mdview` (task 14.2) — band-routed wiring instead of
    /// `builtin`'s shared-vertical-bus (requirement 5.20).
    #[arg(long = "diagram-engine", default_value = DEFAULT_DIAGRAM_ENGINE)]
    diagram_engine: String,
}

/// Why [`resolve_startup`] refused to start. Both variants exit with the
/// same non-zero code (requirement 1.3's "비제로 종료"; the controller's
/// explicit instruction fixes it at 2 for both modes here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupError {
    /// No `.kiro` root found by searching from (or at) this path.
    RootNotFound(PathBuf),
    /// The requested `--log` path resolves inside the found `.kiro` root.
    LogInsideKiro(PathBuf),
    /// `--all`'s target directory (requirement 1.8) does not exist, is not
    /// a directory, or cannot be listed (task 17.3: applies 1.3's own
    /// "path-including stderr message, exit 2" rule here too -- `--all`
    /// must never silently show an empty tree for a bad path).
    AllTargetUnreadable(PathBuf),
}

impl StartupError {
    /// stderr message (requirement 1.3: "탐색 경로 포함 오류 메시지").
    pub fn message(&self) -> String {
        match self {
            StartupError::RootNotFound(start) => {
                format!(
                    "spec-viewer: no .kiro directory found searching from '{}'",
                    start.display()
                )
            }
            StartupError::LogInsideKiro(log_path) => {
                format!(
                    "spec-viewer: --log path '{}' resolves inside the .kiro root; refusing (requirement 8.2)",
                    log_path.display()
                )
            }
            StartupError::AllTargetUnreadable(dir) => {
                format!(
                    "spec-viewer: --all target directory '{}' does not exist or cannot be read",
                    dir.display()
                )
            }
        }
    }

    /// Process exit code (requirement 1.3's "비제로 종료").
    pub fn exit_code(&self) -> i32 {
        2
    }
}

/// Pure startup resolution: find the `.kiro` root and validate `--log`,
/// without touching the terminal (requirements 1.1, 1.2, 1.3, 8.2).
pub fn resolve_startup(args: &Args) -> Result<(PathBuf, Option<PathBuf>), StartupError> {
    let start = args
        .path
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let root = spec_viewer::spec::find_root(&start)
        .ok_or_else(|| StartupError::RootNotFound(start.clone()))?;

    let Some(log_path) = &args.log else {
        return Ok((root, None));
    };

    let cwd = std::env::current_dir().unwrap_or_default();
    let absolute_log = if log_path.is_absolute() {
        log_path.clone()
    } else {
        cwd.join(log_path)
    };
    let resolved_log = match absolute_log.parent() {
        Some(parent) if parent.exists() => match std::fs::canonicalize(parent) {
            Ok(canon_parent) => match absolute_log.file_name() {
                Some(name) => canon_parent.join(name),
                None => canon_parent,
            },
            Err(_) => absolute_log.clone(),
        },
        _ => absolute_log.clone(),
    };

    let resolved_root = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());

    if resolved_log.starts_with(&resolved_root) {
        return Err(StartupError::LogInsideKiro(log_path.clone()));
    }

    Ok((root, Some(log_path.clone())))
}

/// One reducer-plus-render iteration (design.md "app — State & Reducer" +
/// "ui — Panels"), generic over `Backend` so tests can drive it with
/// `ratatui::backend::TestBackend` (requirements 1.4, 1.5).
pub fn step<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut spec_viewer::app::AppState,
    action: spec_viewer::app::Action,
) -> Result<spec_viewer::app::Control, B::Error> {
    let control = spec_viewer::app::update(state, action);
    terminal.draw(|f| spec_viewer::ui::render(f, state))?;
    Ok(control)
}

fn run_loop<B: ratatui::backend::Backend<Error = std::io::Error>>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut spec_viewer::app::AppState,
    rx: &std::sync::mpsc::Receiver<spec_viewer::watch::FsEvent>,
) -> std::io::Result<()> {
    // Draw once before waiting on any event: without this, the screen stays
    // blank until the first keypress or fs event arrives, since every draw
    // below is gated on receiving one first (found via a real-terminal smoke
    // test after task 7.2; requirement 1.4 "뷰어 오픈 → ... 병렬 표시" means
    // immediately on open, not after the user's first input).
    terminal.draw(|f| spec_viewer::ui::render(f, state))?;

    loop {
        if let Ok(fs_event) = rx.try_recv() {
            if step(terminal, state, spec_viewer::app::Action::Fs(fs_event))?
                == spec_viewer::app::Control::Quit
            {
                break;
            }
        }

        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            // Apply every already-queued input event (an `update`, no
            // `terminal.draw`) before rendering once at the end, rather than
            // `step`-ing (update + draw) per raw event as before. A mouse
            // wheel/trackpad burst hands crossterm many discrete
            // ScrollUp/ScrollDown events for what the user feels as one
            // continuous scroll gesture; a diagram-heavy doc panel makes
            // each `terminal.draw` comparatively expensive (most of the
            // panel's cells change every scroll tick, so ratatui's
            // double-buffer diff has little to skip), so drawing once per
            // *raw* event falls behind the input rate and the backlog only
            // grows for as long as the burst continues -- exactly the
            // "느려짐이 누적되는" symptom this coalescing avoids. Draining
            // down to "nothing left queued *right now*" (zero-timeout poll)
            // and rendering once reflects the batch's final state instead.
            let mut quit = false;
            let mut applied_any = false;
            loop {
                let action = match crossterm::event::read()? {
                    crossterm::event::Event::Key(k) => spec_viewer::app::Action::Key(k),
                    crossterm::event::Event::Mouse(m) => spec_viewer::app::Action::Mouse(m),
                    crossterm::event::Event::Resize(w, h) => spec_viewer::app::Action::Resize(w, h),
                    _ => {
                        if !crossterm::event::poll(std::time::Duration::ZERO)? {
                            break;
                        }
                        continue;
                    }
                };
                applied_any = true;
                if spec_viewer::app::update(state, action) == spec_viewer::app::Control::Quit {
                    quit = true;
                    break;
                }
                if !crossterm::event::poll(std::time::Duration::ZERO)? {
                    break;
                }
            }
            if applied_any {
                terminal.draw(|f| spec_viewer::ui::render(f, state))?;
            }
            if quit {
                break;
            }
        } else {
            // Poll timed out with no input at all -- deliver a Tick so
            // time-driven behavior (task 12.5's drag auto-scroll) can
            // progress even without a fresh key/mouse event.
            if step(terminal, state, spec_viewer::app::Action::Tick)?
                == spec_viewer::app::Control::Quit
            {
                break;
            }
        }
    }
    Ok(())
}

/// What kind of tree source startup resolved to, plus everything `main()`
/// needs to wire up the rest of the app around it: the watch/scan root,
/// whether requirement 1.6's file view applies, and (only then) the file
/// path to load into it.
///
/// Still touches the real filesystem (`load_snapshot`/`FsTree::scan` have
/// no injectable-snapshot equivalent, same as `resolve_startup`'s own
/// `is_dir`/`canonicalize` calls) so this isn't "pure" in the no-I/O sense
/// -- but like `resolve_startup`, it has no terminal/event-loop dependency,
/// so it can be driven directly from a test with a real temp directory.
struct Startup {
    source: spec_viewer::spec::TreeSource,
    root: PathBuf,
    is_file: bool,
    file_view_path: Option<PathBuf>,
}

/// Resolve which [`TreeSource`](spec_viewer::spec::TreeSource) and watch
/// root to use for this run: `--all` (requirement 1.8) skips
/// `find_root`/`.kiro` entirely and scans `args.path` (default cwd)
/// directly as a plain `FsTree`; otherwise this is the existing
/// `.kiro`-rooted `SpecRoot` flow, `resolve_startup`'s root-search failure
/// (1.3) included. Requirement 1.6's file-view special case is a
/// `.kiro`-mode-only concept (that requirement's own text), so `--all`
/// never produces a `file_view_path`.
fn resolve_source(args: &Args) -> Result<Startup, StartupError> {
    if args.all {
        let dir = args
            .path
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        // Task 17.3: a nonexistent or unreadable target must fail the same
        // way 1.3 does, not silently render an empty tree -- `FsTree::scan`
        // itself has no way to report this (`ignore::WalkBuilder` just
        // yields nothing for a root it can't open), so the check has to
        // happen here, before scanning.
        if std::fs::read_dir(&dir).is_err() {
            return Err(StartupError::AllTargetUnreadable(dir));
        }
        let tree = spec_viewer::spec::FsTree::scan(&dir);
        return Ok(Startup {
            source: spec_viewer::spec::TreeSource::Files(tree),
            root: dir,
            is_file: false,
            file_view_path: None,
        });
    }

    let (root, log) = resolve_startup(args)?;
    // Accepted/validated (requirement 8.2); this task does not otherwise
    // consume the log destination.
    let _log_path = log;

    // requirement 1.6: a `.md` path argument opens a file view (no tree,
    // doc panel at full width) instead of the tree-rooted spec browser.
    // `resolve_startup`'s `root` is already found from the file's own
    // location (`find_root`'s ancestor search starts at `start`'s parent),
    // so no second root lookup is needed here.
    let start_path = args
        .path
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let is_file = start_path.is_file();

    let snapshot = spec_viewer::app::load_snapshot(&root);
    let mut spec_root = spec_viewer::spec::build(&snapshot);
    spec_viewer::spec::sort_specs(&mut spec_root.specs, args.sort.into());
    let file_view_path = if is_file { Some(start_path) } else { None };
    Ok(Startup {
        source: spec_viewer::spec::TreeSource::Kiro(spec_root),
        root,
        is_file,
        file_view_path,
    })
}

fn main() {
    let args = Args::parse();

    // Resolve the diagram engine before any terminal I/O; an unknown name
    // prints the available list to stderr and exits 2 (task 14.1).
    if let Err(e) = spec_viewer::markdown::mermaid::engine::select(&args.diagram_engine) {
        eprintln!("{e}");
        std::process::exit(2);
    }

    let Startup {
        source,
        root,
        is_file,
        file_view_path,
    } = match resolve_source(&args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e.message());
            std::process::exit(e.exit_code());
        }
    };

    let mut terminal = ratatui::init();
    let size = terminal
        .size()
        .map(|s| (s.width, s.height))
        .unwrap_or((80, 24));

    // Requirement 9.8 "마우스 캡처 실패 터미널 → 키보드 조작 전부 정상, 상태
    // 표시줄에 표시 없음": best-effort only -- a terminal that rejects mouse
    // reporting still gets every keyboard feature, silently, with no flag
    // anywhere in `AppState` to track the degraded mode.
    let mouse_capture_enabled = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::EnableMouseCapture
    )
    .is_ok();

    let (tx, rx) = std::sync::mpsc::channel();
    let watch = if args.no_watch {
        spec_viewer::watch::manual("--no-watch")
    } else {
        spec_viewer::watch::start(&root, tx)
    };
    let watch_status = match &watch {
        spec_viewer::watch::Watch::Live(_) => spec_viewer::app::WatchStatus::Live,
        spec_viewer::watch::Watch::Manual(reason) => spec_viewer::app::WatchStatus::Manual {
            reason: reason.clone(),
        },
    };

    let tree_mode = match args.tree {
        TreeMode::Always => spec_viewer::app::TreeMode::Always,
        TreeMode::Auto => spec_viewer::app::TreeMode::Auto,
        TreeMode::Hidden => spec_viewer::app::TreeMode::Hidden,
        TreeMode::Single => spec_viewer::app::TreeMode::Single,
    };

    let tree_visible = spec_viewer::app::initial_tree_visible(tree_mode, is_file, size.0);

    let mut state = spec_viewer::app::AppState::new(
        source,
        root.clone(),
        size,
        watch_status,
        tree_mode,
        tree_visible,
    );
    // `resolve_source` already sorted the built `SpecRoot` by this same key
    // (`--all` mode has no sort concept and never reaches here with a
    // non-Kiro source, per `sort_key`'s own doc comment) -- this just keeps
    // `state.sort_key` in sync with it for the `s` hotkey / a later resync.
    state.sort_key = args.sort.into();

    if let Some(path) = &file_view_path {
        // The doc panel's own outer width for this state, not the raw
        // terminal width -- a two-panel layout's doc panel is narrower than
        // the terminal, and `size.0` here wrapped text past the panel's
        // right border, silently dropping clipped lines (tasks.md 20.1).
        state.doc = spec_viewer::app::loader::load_doc(path, spec_viewer::app::doc_panel_width(&state));
    }

    // `watch` (holding the live Debouncer, if any) must outlive the loop
    // below -- dropping it early silently stops filesystem watching.
    let run_result = run_loop(&mut terminal, &mut state, &rx);

    drop(watch);
    if mouse_capture_enabled {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    }
    ratatui::restore();

    if let Err(e) = run_result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use spec_viewer::app::{Action, DocView, Panel, Popup};
    use spec_viewer::spec::{DocKind, NodeId};
    use std::fs;
    use std::path::Path;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("spec_viewer_main_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    fn fixtures_root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
    }

    fn args_with_path(path: Option<PathBuf>) -> Args {
        Args {
            path,
            all: false,
            no_watch: false,
            log: None,
            tree: TreeMode::Auto,
            sort: SortArg::Name,
            diagram_engine: "builtin".to_string(),
        }
    }

    #[test]
    fn resolve_startup_root_not_found_reports_search_path_and_exit_2() {
        // A leaf dir under a scratch tree with no `.kiro` anywhere in its
        // ancestry that we control (same caution as `find_root`'s own
        // tests: never search upward from a real place that might
        // accidentally contain a `.kiro` above it).
        let leaf = scratch_dir("root_not_found").join("a/b");
        fs::create_dir_all(&leaf).unwrap();

        let args = args_with_path(Some(leaf.clone()));
        let result = resolve_startup(&args);

        match result {
            Err(StartupError::RootNotFound(searched)) => {
                assert_eq!(searched, leaf);
                let err = StartupError::RootNotFound(searched);
                assert_eq!(err.exit_code(), 2);
                assert!(
                    err.message().contains(&leaf.to_string_lossy().to_string()),
                    "message should include the searched path: {}",
                    err.message()
                );
            }
            other => panic!("expected RootNotFound, got {other:?}"),
        }

        fs::remove_dir_all(leaf.parent().unwrap().parent().unwrap()).ok();
    }

    #[test]
    fn resolve_startup_succeeds_for_real_kiro_root() {
        let scratch = scratch_dir("success");
        fs::create_dir_all(scratch.join(".kiro")).unwrap();

        let args = args_with_path(Some(scratch.clone()));
        let result = resolve_startup(&args);

        match result {
            Ok((root, log)) => {
                assert!(log.is_none());
                assert_eq!(root.file_name().unwrap(), ".kiro");
            }
            Err(e) => panic!("expected Ok, got {e:?}"),
        }

        fs::remove_dir_all(&scratch).ok();
    }

    fn args_all(path: Option<PathBuf>) -> Args {
        let mut args = args_with_path(path);
        args.all = true;
        args
    }

    // --- task 19.2: --sort applies to the built SpecRoot at startup ------

    fn write_spec_with_phase(root: &Path, name: &str, phase: &str) {
        let dir = root.join("specs").join(name);
        fs::create_dir_all(&dir).unwrap();
        let spec_json = format!(
            "{{\n  \"name\": \"{name}\",\n  \"phase\": \"{phase}\"\n}}\n"
        );
        fs::write(dir.join("spec.json"), spec_json).unwrap();
        fs::write(dir.join("requirements.md"), "# demo\n").unwrap();
    }

    #[test]
    fn resolve_source_sorts_specs_by_the_requested_sort_key() {
        let root = scratch_dir("sort_arg");
        fs::create_dir_all(root.join(".kiro/specs")).unwrap();
        let kiro = root.join(".kiro");
        write_spec_with_phase(&kiro, "z-spec", "discovery");
        write_spec_with_phase(&kiro, "a-spec", "tasks");

        let mut args = args_with_path(Some(kiro.clone()));
        args.sort = SortArg::Phase;
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        match startup.source {
            spec_viewer::spec::TreeSource::Kiro(spec_root) => {
                let names: Vec<&str> = spec_root.specs.iter().map(|s| s.name.as_str()).collect();
                // "discovery" < "tasks" alphabetically -- z-spec (discovery)
                // sorts before a-spec (tasks) under SortKey::Phase, proving
                // this is not just name order.
                assert_eq!(names, vec!["z-spec", "a-spec"]);
            }
            spec_viewer::spec::TreeSource::Files(_) => panic!("expected TreeSource::Kiro"),
        }

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolve_source_all_mode_needs_no_kiro_root() {
        // Requirement 1.8: `--all` must work under a plain directory that
        // has no `.kiro` anywhere in its ancestry -- the exact shape that
        // makes `resolve_startup_root_not_found_reports_search_path_and_exit_2`
        // fail for the non-`--all` path above.
        let dir = scratch_dir("all_no_kiro");
        fs::write(dir.join("a.md"), "# A\n").unwrap();

        let args = args_all(Some(dir.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        assert_eq!(startup.root, dir);
        assert!(!startup.is_file, "requirement 1.6's file view is .kiro-only");
        assert!(startup.file_view_path.is_none());
        match startup.source {
            spec_viewer::spec::TreeSource::Files(tree) => {
                assert!(
                    tree.entries.iter().any(|e| e.path == dir.join("a.md")),
                    "expected the scanned tree to contain a.md, got: {:?}",
                    tree.entries
                );
            }
            spec_viewer::spec::TreeSource::Kiro(_) => {
                panic!("expected TreeSource::Files under --all")
            }
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_source_all_mode_defaults_to_current_directory() {
        // No `path` argument at all -- requirement 1.8's "기본 현재
        // 디렉터리". Exercised against a real scratch dir set as cwd would
        // race other tests sharing the process's cwd, so instead this just
        // confirms the no-path branch resolves to *some* real, existing
        // directory rather than panicking -- the default-to-cwd wiring
        // itself is the one line under test, not cwd's specific contents.
        let args = args_all(None);
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));
        assert!(startup.root.is_dir());
    }

    #[test]
    fn resolve_source_all_mode_rejects_a_nonexistent_directory() {
        // Task 17.3's own defect report: `m --all /없는/경로` must not
        // silently show an empty tree -- it should fail the same way 1.3
        // does (path-including stderr message, exit 2), not succeed with
        // nothing in it. A leaf under a scratch dir that is never created.
        let missing = scratch_dir("all_nonexistent").join("does-not-exist");

        let args = args_all(Some(missing.clone()));
        let result = resolve_source(&args);

        match result {
            Err(e) => {
                assert_eq!(e.exit_code(), 2);
                assert!(
                    e.message().contains(&missing.to_string_lossy().to_string()),
                    "message should include the missing path: {}",
                    e.message()
                );
            }
            Ok(_) => panic!(
                "expected --all against a nonexistent directory to fail, not silently show an empty tree"
            ),
        }

        fs::remove_dir_all(missing.parent().unwrap()).ok();
    }

    #[test]
    fn resolve_source_all_mode_rejects_an_unreadable_directory() {
        // "읽을 수 없는 디렉터리" -- exists but denies listing. Permission
        // bits are meaningless to git and to a root-run test process, but
        // this is a real, mechanically-verifiable filesystem state on this
        // platform, exercised directly (same spirit as `find_root`'s own
        // permission-based tests elsewhere in this crate).
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch_dir("all_unreadable");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();

        let args = args_all(Some(dir.clone()));
        let result = resolve_source(&args);

        // Restore permissions before any cleanup/panic unwinding, or the
        // scratch dir can't even be removed afterward.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();

        if unsafe { libc_geteuid_is_root() } {
            // Running as root ignores permission bits entirely -- nothing
            // meaningful to assert in that environment.
            fs::remove_dir_all(&dir).ok();
            return;
        }

        match result {
            Err(e) => {
                assert_eq!(e.exit_code(), 2);
                assert!(
                    e.message().contains(&dir.to_string_lossy().to_string()),
                    "message should include the unreadable path: {}",
                    e.message()
                );
            }
            Ok(_) => panic!(
                "expected --all against an unreadable directory to fail, not silently show an empty tree"
            ),
        }

        fs::remove_dir_all(&dir).ok();
    }

    /// `true` when running as root (uid 0) -- root bypasses the permission
    /// check the previous test relies on, so that test becomes a no-op
    /// there rather than a false failure.
    unsafe fn libc_geteuid_is_root() -> bool {
        extern "C" {
            fn geteuid() -> u32;
        }
        geteuid() == 0
    }

    #[test]
    fn resolve_source_non_all_mode_still_requires_a_kiro_root() {
        // Sanity: `--all`'s new branch must not have loosened the existing
        // (non-`--all`) startup path -- same fixture shape as
        // `resolve_startup_root_not_found_reports_search_path_and_exit_2`.
        let leaf = scratch_dir("resolve_source_needs_kiro").join("a/b");
        fs::create_dir_all(&leaf).unwrap();

        let args = args_with_path(Some(leaf.clone()));
        let result = resolve_source(&args);

        match result {
            Err(StartupError::RootNotFound(searched)) => assert_eq!(searched, leaf),
            Err(other) => panic!("expected RootNotFound, got {other:?}"),
            Ok(_) => panic!("expected RootNotFound, got Ok"),
        }

        fs::remove_dir_all(leaf.parent().unwrap().parent().unwrap()).ok();
    }

    #[test]
    fn resolve_startup_finds_root_from_a_markdown_file_argument() {
        // Requirement 1.6 "루트 탐지는 파일 위치 기준으로 수행": passing a
        // `.md` file (not a directory) still finds the enclosing `.kiro`
        // root, via `find_root`'s own ancestor search starting at the
        // file's parent -- `main()` relies on this rather than special-
        // casing file arguments itself.
        let scratch = scratch_dir("file_arg_root");
        let kiro = scratch.join(".kiro");
        let spec_dir = kiro.join("specs/demo");
        fs::create_dir_all(&spec_dir).unwrap();
        let file_path = spec_dir.join("design.md");
        fs::write(&file_path, "# Demo\n").unwrap();

        let args = args_with_path(Some(file_path.clone()));
        let result = resolve_startup(&args);

        match result {
            Ok((root, log)) => {
                assert!(log.is_none());
                assert_eq!(root, kiro);
            }
            Err(e) => panic!("expected Ok, got {e:?}"),
        }

        fs::remove_dir_all(&scratch).ok();
    }

    #[test]
    fn resolve_startup_rejects_log_path_inside_kiro_root() {
        let scratch = scratch_dir("log_inside");
        fs::create_dir_all(scratch.join(".kiro")).unwrap();
        let root = scratch.join(".kiro");
        // Parent (`root` itself) exists, so `resolve_startup` canonicalizes
        // it the same way it canonicalizes the found root -- this avoids a
        // false negative on platforms (e.g. macOS's /var -> /private/var)
        // where the *non*-canonical and canonical forms of an existing path
        // legitimately differ.
        let log_path = root.join("app.log");

        let mut args = args_with_path(Some(scratch.clone()));
        args.log = Some(log_path.clone());

        let result = resolve_startup(&args);

        match result {
            Err(StartupError::LogInsideKiro(rejected)) => {
                assert_eq!(rejected, log_path);
                assert_eq!(StartupError::LogInsideKiro(rejected).exit_code(), 2);
            }
            other => panic!("expected LogInsideKiro, got {other:?}"),
        }

        fs::remove_dir_all(&scratch).ok();
    }

    #[test]
    fn resolve_startup_accepts_log_path_outside_kiro_root() {
        let scratch = scratch_dir("log_outside_root");
        fs::create_dir_all(scratch.join(".kiro")).unwrap();

        let outside = scratch_dir("log_outside_dest");
        let log_path = outside.join("app.log");

        let mut args = args_with_path(Some(scratch.clone()));
        args.log = Some(log_path.clone());

        let result = resolve_startup(&args);

        match result {
            Ok((_root, log)) => assert_eq!(log, Some(log_path)),
            Err(e) => panic!("expected Ok, got {e:?}"),
        }

        fs::remove_dir_all(&scratch).ok();
        fs::remove_dir_all(&outside).ok();
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn test_state(size: (u16, u16)) -> spec_viewer::app::AppState {
        let snapshot = spec_viewer::app::load_snapshot(&fixtures_root());
        let root = spec_viewer::spec::build(&snapshot);
        spec_viewer::app::AppState::new(
            spec_viewer::spec::TreeSource::Kiro(root),
            fixtures_root(),
            size,
            spec_viewer::app::WatchStatus::Live,
            spec_viewer::app::TreeMode::Auto,
            true,
        )

    }

    fn buffer_contains(buffer: &ratatui::buffer::Buffer, needle: &str) -> bool {
        let area = buffer.area();
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buffer[(x, y)].symbol());
            }
            if row.contains(needle) {
                return true;
            }
        }
        false
    }

    // --- task 7.2: key-sequence integration tests -----------------------
    //
    // These drive real `step()` calls (reducer + render, one iteration each)
    // through the popup state machine (task 5.6) end to end, asserting both
    // the reducer's state transitions *and* the very next frame's pixels --
    // "프레임과 스크롤이 기대대로 동기화" (this task's DONE text) means neither
    // half alone is sufficient. Each scenario builds its own dedicated
    // `.kiro`-shaped fixture on disk (the checked-in
    // `tests/fixtures/kiro/specs/sample-signup/design.md` fixture has only a
    // single heading and no non-heading target line, so it cannot exercise
    // "well separated" TOC entries or a search hit past the first line) --
    // same throwaway-scratch-dir pattern `app::mod`'s own `resync` tests use
    // (never under `tests/fixtures/`).

    fn temp_kiro_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "spec_viewer_main_integration_{}_{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("specs")).unwrap();
        dir
    }

    fn write_requirements_spec(root: &Path, name: &str, requirements: &str) {
        let dir = root.join("specs").join(name);
        fs::create_dir_all(&dir).unwrap();
        let spec_json = format!(
            "{{\n  \"name\": \"{name}\",\n  \"created_at\": \"2026-01-01T00:00:00Z\",\n  \"updated_at\": \"2026-01-01T00:00:00Z\",\n  \"language\": \"ko\",\n  \"phase\": \"design\",\n  \"approvals\": {{\n    \"requirements\": {{ \"generated\": true, \"approved\": true }}\n  }}\n}}\n"
        );
        fs::write(dir.join("spec.json"), spec_json).unwrap();
        fs::write(dir.join("requirements.md"), requirements).unwrap();
    }

    fn build_state_from(root: &Path, size: (u16, u16)) -> spec_viewer::app::AppState {
        let snapshot = spec_viewer::app::load_snapshot(root);
        let spec_root = spec_viewer::spec::build(&snapshot);
        spec_viewer::app::AppState::new(
            spec_viewer::spec::TreeSource::Kiro(spec_root),
            root.to_path_buf(),
            size,
            spec_viewer::app::WatchStatus::Live,
            spec_viewer::app::TreeMode::Auto,
            true,
        )
    }

    /// Row/column of the first cell where `needle` (an ASCII-only substring
    /// in these fixtures, so one buffer cell == one byte/char) starts,
    /// scanning top to bottom.
    fn find_text_cell(buffer: &ratatui::buffer::Buffer, needle: &str) -> Option<(u16, u16)> {
        let area = buffer.area();
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buffer[(x, y)].symbol());
            }
            if let Some(col) = row.find(needle) {
                return Some((y, col as u16));
            }
        }
        None
    }

    fn cell_style(
        buffer: &ratatui::buffer::Buffer,
        x: u16,
        y: u16,
    ) -> (
        ratatui::style::Color,
        ratatui::style::Color,
        ratatui::style::Modifier,
    ) {
        let cell = &buffer[(x, y)];
        (cell.fg, cell.bg, cell.modifier)
    }

    /// The exact centered sub-rectangle `ui::popup::render` draws into
    /// (mirroring its own private `centered_rect(60, 60, area)` -- not
    /// exported, so duplicated here rather than reached into). Needed
    /// because the popup is *not* opaque over the whole frame: outside this
    /// rect the panels underneath (which can carry the same literal text,
    /// e.g. a heading title that is both a TOC entry and already visible in
    /// the doc panel behind it) remain visible, so a plain whole-buffer text
    /// search can land on the wrong occurrence.
    fn popup_rect(full: ratatui::layout::Rect) -> ratatui::layout::Rect {
        use ratatui::layout::{Constraint, Direction, Layout};
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ])
            .split(full);
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ])
            .split(vertical[1])[1]
    }

    /// Like [`find_text_cell`], scoped to `area` only.
    fn find_text_cell_in(
        buffer: &ratatui::buffer::Buffer,
        area: ratatui::layout::Rect,
        needle: &str,
    ) -> Option<(u16, u16)> {
        for y in area.y..area.y + area.height {
            let mut row = String::new();
            for x in area.x..area.x + area.width {
                row.push_str(buffer[(x, y)].symbol());
            }
            if let Some(col) = row.find(needle) {
                return Some((y, area.x + col as u16));
            }
        }
        None
    }

    /// Scenario A (task 7.2 DONE text): `t` -> move -> Enter against a real
    /// TOC popup, asserting the reducer's `Popup::Toc` transitions *and* that
    /// `ui::render`'s very next frame actually reflects them (the popup box
    /// drawn on top with real heading text, the highlighted row moving with
    /// the selection, and the doc panel re-rendering at the jumped-to
    /// heading's line right after confirm).
    #[test]
    fn toc_open_move_confirm_syncs_frame_and_scroll() {
        let root = temp_kiro_root("toc_scenario_a");
        // Three headings, each followed by two short one-line paragraphs
        // (blank-line-separated so each becomes its own `plain`/`lines`
        // entry -- see `markdown::render`'s doc comment / its own tests),
        // so the headings land on precisely computable, well-separated line
        // numbers (0, 3, 6) rather than being adjacent.
        let content = "\
# Heading Alpha

alpha detail line one

alpha detail line two

# Heading Beta

beta detail line one

beta detail line two

# Heading Gamma

gamma detail line one

gamma detail line two
";
        write_requirements_spec(&root, "toc-demo", content);

        let mut state = build_state_from(&root, (120, 40));
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");

        // Load the doc via the normal select-and-Enter tree flow.
        state.tree.select(vec![
            NodeId::Spec("toc-demo".to_string()),
            NodeId::Doc("toc-demo".to_string(), DocKind::Requirements),
        ]);
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)))
            .expect("step should succeed");

        let headings: Vec<(u8, String, usize)> = match &state.doc {
            DocView::Rendered { r, .. } => {
                r.headings.iter().map(|h| (h.level, h.text.clone(), h.line)).collect()
            }
            other => panic!("expected Rendered doc, got {other:?}"),
        };
        assert_eq!(headings.len(), 3, "expected 3 headings in the fixture doc");
        assert_eq!(state.scroll, 0);

        // Step 1 -> 2: `t` opens the TOC popup at the heading at/before the
        // current scroll (0 -> heading index 0), and the very next frame
        // shows the bordered "TOC" box with real heading text on top of the
        // panels (not just a reducer-level state change).
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Char('t'))))
            .expect("step should succeed");
        match &state.popup {
            Some(Popup::Toc(idx)) => assert_eq!(*idx, 0),
            other => panic!("expected Some(Popup::Toc(0)), got {other:?}"),
        }
        let buffer_opened = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer_opened, "TOC"), "expected the TOC box title visible");
        assert!(
            buffer_contains(&buffer_opened, "Heading Alpha")
                && buffer_contains(&buffer_opened, "Heading Beta"),
            "expected at least two real heading texts visible in the rendered popup"
        );

        // Scoped to the popup's own rect (not the whole buffer): the popup
        // is only opaque over its centered sub-area, and "Heading Alpha" /
        // "Heading Beta" are also already visible, unrelated to selection,
        // in the doc panel behind it -- a whole-buffer search would find
        // that background occurrence instead of the TOC list's own row.
        let popup_area = popup_rect(*buffer_opened.area());
        let (row_alpha, col_alpha) = find_text_cell_in(&buffer_opened, popup_area, "Heading Alpha")
            .expect("Heading Alpha visible inside the TOC popup");
        let (row_beta, col_beta) = find_text_cell_in(&buffer_opened, popup_area, "Heading Beta")
            .expect("Heading Beta visible inside the TOC popup");
        let style_alpha_selected = cell_style(&buffer_opened, col_alpha, row_alpha);
        let style_beta_unselected = cell_style(&buffer_opened, col_beta, row_beta);

        // Step 3: move the selection down one -- the reducer's selected
        // index advances, and the *rendered* highlighted row moves with it
        // (style-diffing the same two fixed screen cells across frames,
        // mirroring `ui::popup`'s own highlight test).
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Down)))
            .expect("step should succeed");
        match &state.popup {
            Some(Popup::Toc(idx)) => assert_eq!(*idx, 1),
            other => panic!("expected Some(Popup::Toc(1)), got {other:?}"),
        }
        let buffer_moved = terminal.backend().buffer().clone();
        let style_alpha_after = cell_style(&buffer_moved, col_alpha, row_alpha);
        let style_beta_after = cell_style(&buffer_moved, col_beta, row_beta);
        assert_ne!(
            style_alpha_selected, style_alpha_after,
            "expected the previously-selected row's style to change once selection moves away"
        );
        assert_ne!(
            style_beta_unselected, style_beta_after,
            "expected the newly-selected row's style to change once selection moves onto it"
        );
        assert_eq!(
            style_alpha_after, style_beta_unselected,
            "sanity: an unselected row's style should be the same regardless of which row it is"
        );

        // Step 4: confirm with Enter -- popup closes, `state.scroll` lands
        // exactly on the *selected* heading's line (computed from the real
        // doc content above, not guessed), and the doc panel's very next
        // frame actually shows that heading near the top of its viewport.
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)))
            .expect("step should succeed");
        assert_eq!(state.popup, None);
        let expected_scroll = headings[1].2;
        assert_eq!(state.scroll, expected_scroll);

        let buffer_jumped = terminal.backend().buffer().clone();
        let (row_after, _) =
            find_text_cell(&buffer_jumped, "Heading Beta").expect("Heading Beta visible after jump");
        assert!(
            row_after <= 2,
            "expected the jumped-to heading near the top of the doc panel viewport, got row {row_after}"
        );

        // Negative/cancel sanity check (already unit-tested at the reducer
        // level -- task 5.6): reopening the popup and pressing Esc instead
        // of confirming closes it without touching the scroll this jump
        // just landed on.
        let scroll_after_jump = state.scroll;
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Char('t'))))
            .expect("step should succeed");
        assert!(matches!(state.popup, Some(Popup::Toc(_))));
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Down)))
            .expect("step should succeed");
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Esc)))
            .expect("step should succeed");
        assert_eq!(state.popup, None);
        assert_eq!(
            state.scroll, scroll_after_jump,
            "Esc should cancel without changing scroll"
        );

        fs::remove_dir_all(&root).ok();
    }

    /// Scenario B (task 7.2 DONE text): `/` -> type -> Enter against a real
    /// search-input popup, asserting the reducer's `Popup::SearchInput`
    /// transitions *and* that `ui::render`'s very next frame reflects them
    /// (the "Search" box with a bare `/` prompt, the typed buffer, and the
    /// doc panel re-rendering at the matched line with a visibly different
    /// highlight style right after confirm).
    #[test]
    fn search_open_type_confirm_syncs_frame_and_scroll() {
        let root = temp_kiro_root("search_scenario_b");
        // Each sentence is its own blank-line-separated paragraph so it
        // becomes exactly one `plain`/`lines` entry (see
        // `markdown::render`'s tests) -- the target word sits on a
        // non-first line so a real scroll jump is meaningful.
        let content = "\
# Search Demo

alpha intro line

beta needle marker line

gamma trailing line
";
        write_requirements_spec(&root, "search-demo", content);

        let mut state = build_state_from(&root, (120, 40));
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");

        state.tree.select(vec![
            NodeId::Spec("search-demo".to_string()),
            NodeId::Doc("search-demo".to_string(), DocKind::Requirements),
        ]);
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)))
            .expect("step should succeed");

        let expected_match_line = match &state.doc {
            DocView::Rendered { r, .. } => r
                .plain
                .iter()
                .position(|l| l.to_lowercase().contains("needle"))
                .expect("expected a line containing 'needle' in the fixture doc"),
            other => panic!("expected Rendered doc, got {other:?}"),
        };
        assert!(
            expected_match_line > 0,
            "target line should not be the first line, so the scroll jump is meaningful"
        );

        // Requirement 2.10: `/` now routes to whichever panel is focused --
        // Enter/"select" above loads the doc but does not itself move focus
        // off the tree, so switch to the doc panel first (`Tab`) to reach
        // this test's actual target, the doc-panel search.
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Tab)))
            .expect("step should succeed");
        assert_eq!(state.focus, Panel::Doc);

        // Step 2: `/` opens the search-input popup with an empty buffer, and
        // the very next frame shows the "Search" box with a bare `/` prompt.
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Char('/'))))
            .expect("step should succeed");
        match &state.popup {
            Some(Popup::SearchInput(s)) => assert!(s.is_empty()),
            other => panic!("expected Some(Popup::SearchInput(\"\")), got {other:?}"),
        }
        let buffer_opened = terminal.backend().buffer().clone();
        // Scoped to the popup's own rect: the fixture doc's own "# Search
        // Demo" heading also literally contains "Search" and is already
        // visible in the doc panel behind the (non-fully-opaque) popup, so
        // a whole-buffer search would pass even if the popup itself drew
        // nothing.
        let popup_area_search = popup_rect(*buffer_opened.area());
        assert!(
            find_text_cell_in(&buffer_opened, popup_area_search, "Search").is_some(),
            "expected the Search box title visible inside the popup"
        );
        assert!(
            find_text_cell_in(&buffer_opened, popup_area_search, "/").is_some(),
            "expected the bare '/' prompt visible inside the popup"
        );

        // Step 3: type the query one character at a time -- the popup's
        // buffer accumulates exactly what was typed.
        for c in "needle".chars() {
            step(&mut terminal, &mut state, Action::Key(key(KeyCode::Char(c))))
                .expect("step should succeed");
        }
        match &state.popup {
            Some(Popup::SearchInput(s)) => assert_eq!(s, "needle"),
            other => panic!("expected Some(Popup::SearchInput(\"needle\")), got {other:?}"),
        }

        // Step 4: confirm with Enter -- popup closes, matches are non-empty
        // and contain the expected line, `state.scroll` lands on the first
        // match, and the doc panel's very next frame shows that matched
        // line, visibly highlighted, near the top of its viewport.
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)))
            .expect("step should succeed");
        assert_eq!(state.popup, None);
        assert!(!state.search.matches.is_empty());
        assert!(state.search.matches.contains(&expected_match_line));
        assert_eq!(state.scroll, expected_match_line);

        let buffer_jumped = terminal.backend().buffer().clone();
        let (row_matched, col_matched) =
            find_text_cell(&buffer_jumped, "needle").expect("matched line visible after jump");
        assert!(
            row_matched <= 2,
            "expected the matched line near the top of the doc panel viewport, got row {row_matched}"
        );
        let (row_other, col_other) = find_text_cell(&buffer_jumped, "gamma trailing line")
            .expect("a known non-matched line visible for style comparison");
        assert_ne!(row_matched, row_other);
        let style_matched = cell_style(&buffer_jumped, col_matched, row_matched);
        let style_other = cell_style(&buffer_jumped, col_other, row_other);
        assert_ne!(
            style_matched, style_other,
            "expected the matched line's style to differ from a non-matched line's style"
        );

        // Negative/cancel sanity check: reopening the popup, typing, and
        // pressing Esc instead of confirming closes it without touching the
        // prior confirmed search or scroll.
        let scroll_after_search = state.scroll;
        let search_after_search = state.search.clone();
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Char('/'))))
            .expect("step should succeed");
        for c in "xyz".chars() {
            step(&mut terminal, &mut state, Action::Key(key(KeyCode::Char(c))))
                .expect("step should succeed");
        }
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Esc)))
            .expect("step should succeed");
        assert_eq!(state.popup, None);
        assert_eq!(state.scroll, scroll_after_search);
        assert_eq!(state.search, search_after_search);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn step_renders_tree_and_detects_quit() {
        let mut state = test_state((120, 40));
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");

        // A harmless key first (1.4: 실행 -> 트리 표시).
        let control = step(&mut terminal, &mut state, spec_viewer::app::Action::Key(key(KeyCode::Tab)))
            .expect("step should succeed");
        assert_eq!(control, spec_viewer::app::Control::Continue);

        let buffer = terminal.backend().buffer().clone();
        assert!(
            buffer_contains(&buffer, "sample-signup"),
            "expected a spec name visible in the rendered tree panel"
        );

        // Then the quit key (1.5: 종료 키 입력).
        let control = step(&mut terminal, &mut state, spec_viewer::app::Action::Key(key(KeyCode::Char('q'))))
            .expect("step should succeed");
        assert_eq!(control, spec_viewer::app::Control::Quit);
    }
}
