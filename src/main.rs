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

use std::path::{Path, PathBuf};
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
#[command(name = "m", about = "Spec Viewer CLI", version)]
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
    /// Editor command used for the edit-mode hotkey (`run_loop` ->
    /// `handle_edit_file`); takes priority over `$VISUAL`/`$EDITOR`/`vi`
    /// (requirement 2.1, `resolve_editor`).
    #[arg(long = "editor")]
    editor: Option<String>,
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
    mouse_capture_enabled: bool,
    cli_editor: Option<&str>,
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
            // Set when a raw event's reducer call returns
            // `Control::EditFile` -- the drain loop stops immediately (any
            // other raw events still queued in this same batch predate
            // opening the editor, so they're stale and safe to drop) and the
            // path is handed to `handle_edit_file` once this batch's own
            // draw (below) has happened.
            let mut edit_file: Option<PathBuf> = None;
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
                match spec_viewer::app::update(state, action) {
                    spec_viewer::app::Control::Quit => {
                        quit = true;
                        break;
                    }
                    spec_viewer::app::Control::EditFile(path) => {
                        edit_file = Some(path);
                        break;
                    }
                    spec_viewer::app::Control::Continue => {}
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
            if let Some(path) = edit_file {
                let control =
                    handle_edit_file(terminal, state, path, cli_editor, mouse_capture_enabled)?;
                if control == spec_viewer::app::Control::Quit {
                    break;
                }
            }
        } else if state.auto_scroll.is_some() {
            // Poll timed out with no input at all -- deliver a Tick so
            // time-driven behavior (task 12.5's drag auto-scroll) can
            // progress even without a fresh key/mouse event. `Action::Tick`
            // (see `update`) is a no-op unless `auto_scroll` is armed, so
            // skip both the update *and* the draw otherwise -- redrawing on
            // a bare poll-timeout with nothing to show for it just makes
            // the terminal repaint on a ~100ms heartbeat forever while
            // idle, for no reason (unlike e.g. mdview's pager loop, which
            // only ever draws in response to a real event).
            if step(terminal, state, spec_viewer::app::Action::Tick)?
                == spec_viewer::app::Control::Quit
            {
                break;
            }
        }
    }
    Ok(())
}

/// Picks the editor command by priority (`--editor` > `$VISUAL` > `$EDITOR`
/// > `"vi"`, requirements 2.1-2.4) and tokenizes it on whitespace (e.g.
/// `"code -w"` -> `["code", "-w"]`); `tokens[0]` is the executable,
/// `tokens[1..]` are fixed arguments the caller passes to `Command` ahead of
/// the file path (research.md: no shell is involved, so quoted multi-word
/// arguments are out of scope).
pub fn resolve_editor(cli_editor: Option<&str>) -> Vec<String> {
    let chosen = cli_editor
        .map(|s| s.to_string())
        .or_else(|| std::env::var("VISUAL").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "vi".to_string());

    chosen.split_whitespace().map(|s| s.to_string()).collect()
}

/// Outcome of [`run_editor`]: whether the caller ([`handle_edit_file`])
/// should treat this as a successful edit (reload/redraw path, requirement
/// 3.1/3.3's territory) or a failure to surface to the user (requirement
/// 4.1, 4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOutcome {
    /// The editor process ran and exited with a success status.
    Reloaded,
    /// The editor could not be launched, or exited with a failure status;
    /// the `String` is a user-facing message (requirement 4.1, 4.2).
    Failed(String),
}

/// Suspends the terminal (mirrors `main()`'s own mouse-capture/raw-mode/alt-
/// screen setup and teardown, requirement 1.2, 5.1, 5.2), runs the resolved
/// editor (requirement 2.1-2.4 via [`resolve_editor`]) against `path`
/// without a shell (research.md), waits for it to exit, then restores the
/// terminal and forces a full repaint via `terminal.clear()`.
///
/// The terminal-state toggles (`enable_raw_mode`/`disable_raw_mode`,
/// `EnterAlternateScreen`/`LeaveAlternateScreen`, mouse capture) are
/// best-effort: like `main()`'s own `mouse_capture_enabled` handling, a
/// `Result::Err` here (routine outside a real tty, e.g. under `TestBackend`
/// in this crate's own test suite) is silently ignored rather than
/// panicking -- this function must keep running the child process and
/// deciding the `EditOutcome` even when those calls fail.
fn run_editor<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    path: &Path,
    cli_editor: Option<&str>,
    mouse_capture_enabled: bool,
) -> EditOutcome {
    let tokens = resolve_editor(cli_editor);
    let Some(program) = tokens.first() else {
        return EditOutcome::Failed("resolved editor command is empty".to_string());
    };
    let fixed_args = &tokens[1..];

    if mouse_capture_enabled {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    }
    let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    let _ = crossterm::terminal::disable_raw_mode();

    let spawn_status = std::process::Command::new(program)
        .args(fixed_args)
        .arg(path)
        .status();

    let _ = crossterm::terminal::enable_raw_mode();
    let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen);
    if mouse_capture_enabled {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture);
    }

    // Force a full redraw once control returns, regardless of outcome --
    // whatever the editor drew over the alternate screen must not linger.
    let _ = terminal.clear();

    match spawn_status {
        Ok(status) if status.success() => EditOutcome::Reloaded,
        Ok(status) => EditOutcome::Failed(format!(
            "editor '{}' exited with {}",
            tokens.join(" "),
            status
        )),
        Err(e) => EditOutcome::Failed(format!(
            "failed to launch editor '{}': {}",
            tokens.join(" "),
            e
        )),
    }
}

/// Consumes a `Control::EditFile(path)` returned from the reducer
/// (design.md: "`run_loop` 안에서 `Control::EditFile(path)` 수신 시 ...
/// `run_editor` 호출 -> `EditOutcome`에 따라 `step()`으로 `Action::EditFailed`
/// / `Action::Fs` 디스패치"): runs the resolved editor via [`run_editor`],
/// then dispatches its outcome back into the reducer.
///
/// - `EditOutcome::Failed(msg)` -> `Action::EditFailed(msg)` (requirements
///   4.1, 4.2), which `step` both applies and redraws for.
/// - `EditOutcome::Reloaded` while `state.watch == WatchStatus::Live` ->
///   `Action::Fs` for `path` (requirement 3.1), reusing the same full-resync
///   reducer path a live filesystem-watch event takes -- again via `step`,
///   which redraws.
/// - `EditOutcome::Reloaded` while watch is `Manual` -> no further `Action`
///   at all (requirement 3.2: manual refresh only, no auto-reload), but the
///   screen must still be redrawn explicitly here: `run_editor` leaves the
///   terminal's buffer blank (its own `terminal.clear()`, needed regardless
///   of outcome so nothing the editor drew lingers), and neither `step` nor
///   any other call in this branch would otherwise paint the viewer's own
///   frame back over it (requirement 1.2's "화면은 항상 다시 그려진다").
///
/// Deliberately factored out of `run_loop` itself: this function never polls
/// or reads a `crossterm::event`, so unlike `run_loop` (which blocks on the
/// real tty and has no test in this suite for that reason -- see this file's
/// module doc comment) it can be driven directly from a `TestBackend` plus a
/// fake-editor script (the same pattern `run_editor`'s own tests use).
///
/// Generic over plain `Backend` (like [`step`], not pinned to
/// `Backend<Error = std::io::Error>` the way `run_loop` itself is) precisely
/// so a `TestBackend` (`Error = Infallible`) can drive it directly in tests;
/// `run_loop`'s own `B::Error = std::io::Error` bound makes its `?` on this
/// function's result a no-op conversion.
fn handle_edit_file<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut spec_viewer::app::AppState,
    path: PathBuf,
    cli_editor: Option<&str>,
    mouse_capture_enabled: bool,
) -> Result<spec_viewer::app::Control, B::Error> {
    match run_editor(terminal, &path, cli_editor, mouse_capture_enabled) {
        EditOutcome::Failed(msg) => {
            step(terminal, state, spec_viewer::app::Action::EditFailed(msg))
        }
        EditOutcome::Reloaded => {
            if state.watch == spec_viewer::app::WatchStatus::Live {
                step(
                    terminal,
                    state,
                    spec_viewer::app::Action::Fs(spec_viewer::watch::FsEvent { paths: vec![path] }),
                )
            } else {
                terminal.draw(|f| spec_viewer::ui::render(f, state))?;
                Ok(spec_viewer::app::Control::Continue)
            }
        }
    }
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

    // Requirement 1.1-1.3: decide between `.kiro` and spec-kit before
    // falling back to `resolve_startup`'s `.kiro`-only search. `find_root`
    // and `spec_kit::find_spec_kit_root` are each independently searched
    // from the same `start`; `find_spec_kit_root` already resolves same-
    // directory coexistence in `.kiro`'s favor (requirement 1.2) by
    // returning `None` in that case, so here only the "different levels"
    // case needs an explicit tie-break.
    let start = args
        .path
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let kiro_dir = spec_viewer::spec::find_root(&start);
    let spec_kit_root = spec_viewer::spec::spec_kit::find_spec_kit_root(&start);

    let prefer_spec_kit = match (&kiro_dir, &spec_kit_root) {
        (None, Some(_)) => true,
        (Some(kiro_dir), Some(spec_kit_root)) => {
            // Recover the actual `.specify` marker path so both sides
            // compare like-for-like (`kiro_dir` is already `.kiro` itself).
            // Whichever marker sits deeper (more path components, i.e.
            // closer to `start`) wins; an exact tie favors `.kiro`.
            let specify_marker = spec_kit_root.join(".specify");
            specify_marker.components().count() > kiro_dir.components().count()
        }
        _ => false,
    };

    if prefer_spec_kit {
        let sk_root = spec_kit_root.expect("prefer_spec_kit implies Some(spec_kit_root)");
        let specs_dir = sk_root.join("specs");
        let features = spec_viewer::spec::spec_kit::build(&specs_dir);
        return Ok(Startup {
            source: spec_viewer::spec::TreeSource::SpecKit(features),
            // The watch/scan root is `specs/` itself, not the whole
            // project -- mirrors `.kiro` mode's `root` being the `.kiro`
            // directory, not its parent.
            root: specs_dir,
            is_file: false,
            // Requirement 1.6's file-view special case is `.kiro`-only
            // (design.md Out-of-Scope for spec-kit).
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
    let run_result = run_loop(
        &mut terminal,
        &mut state,
        &rx,
        mouse_capture_enabled,
        args.editor.as_deref(),
    );

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
            editor: None,
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
            _ => panic!("expected TreeSource::Kiro"),
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
            _ => panic!("expected TreeSource::Files under --all"),
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

    // --- spec-viewer-spec-kit-support task 4: source priority (1.1-1.3) ---

    /// Creates `.specify/` and `specs/001-demo/spec.md` directly under
    /// `root`, and returns `root/specs` (the spec-kit watch root).
    fn init_spec_kit(root: &Path) -> PathBuf {
        fs::create_dir_all(root.join(".specify")).unwrap();
        let specs_dir = root.join("specs");
        fs::create_dir_all(specs_dir.join("001-demo")).unwrap();
        fs::write(specs_dir.join("001-demo").join("spec.md"), "# Demo\n").unwrap();
        specs_dir
    }

    #[test]
    fn resolve_source_only_kiro_present_uses_kiro_tree_source() {
        let root = scratch_dir("priority_only_kiro");
        fs::create_dir_all(root.join(".kiro/specs")).unwrap();

        let args = args_with_path(Some(root.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        assert!(matches!(startup.source, spec_viewer::spec::TreeSource::Kiro(_)));
        assert_eq!(startup.root, root.join(".kiro"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolve_source_only_spec_kit_present_uses_spec_kit_tree_source() {
        let root = scratch_dir("priority_only_spec_kit");
        let specs_dir = init_spec_kit(&root);

        let args = args_with_path(Some(root.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        match startup.source {
            spec_viewer::spec::TreeSource::SpecKit(features) => {
                assert_eq!(features.len(), 1);
                assert_eq!(features[0].name, "001-demo");
            }
            _ => panic!("expected TreeSource::SpecKit"),
        }
        assert_eq!(
            startup.root, specs_dir,
            "watch root should be specs/, not the project root"
        );
        assert!(!startup.is_file);
        assert!(startup.file_view_path.is_none());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolve_source_kiro_and_spec_kit_coexist_in_same_dir_prefers_kiro() {
        let root = scratch_dir("priority_coexist_same_dir");
        fs::create_dir_all(root.join(".kiro/specs")).unwrap();
        init_spec_kit(&root);

        let args = args_with_path(Some(root.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        assert!(matches!(startup.source, spec_viewer::spec::TreeSource::Kiro(_)));
        assert_eq!(startup.root, root.join(".kiro"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolve_source_prefers_deeper_spec_kit_over_shallower_kiro() {
        // `.kiro` sits 3 levels above `.specify` -- spec-kit is the closer
        // (deeper) root relative to `start` and must win even though both
        // exist (just not in the same directory, so `find_spec_kit_root`'s
        // own same-dir-coexistence rule does not apply here).
        let far_root = scratch_dir("priority_far_kiro");
        fs::create_dir_all(far_root.join(".kiro/specs")).unwrap();
        let mid = far_root.join("a/b/c");
        fs::create_dir_all(&mid).unwrap();
        let specs_dir = init_spec_kit(&mid);
        let start = mid.join("d/e");
        fs::create_dir_all(&start).unwrap();

        let args = args_with_path(Some(start.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        match startup.source {
            spec_viewer::spec::TreeSource::SpecKit(_) => {}
            _ => panic!("expected TreeSource::SpecKit (closer/deeper than .kiro)"),
        }
        assert_eq!(startup.root, specs_dir);

        fs::remove_dir_all(&far_root).ok();
    }

    #[test]
    fn resolve_source_neither_kiro_nor_spec_kit_returns_root_not_found() {
        let leaf = scratch_dir("priority_neither").join("a/b");
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
    fn resolve_source_all_flag_wins_even_when_spec_kit_present() {
        // Requirement/regression: `--all` must remain the highest-priority
        // mode even when a `.specify` marker is also present.
        let root = scratch_dir("priority_all_wins");
        init_spec_kit(&root);
        fs::write(root.join("plain.md"), "# Plain\n").unwrap();

        let args = args_all(Some(root.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        match startup.source {
            spec_viewer::spec::TreeSource::Files(_) => {}
            _ => panic!("expected TreeSource::Files under --all, even with .specify present"),
        }

        fs::remove_dir_all(&root).ok();
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

    // --- task 3.1: resolve_editor priority + tokenization -------------------

    /// Serializes tests that mutate `$VISUAL`/`$EDITOR` -- these are
    /// process-global, and while the validation command for this file asks
    /// for `--test-threads=1`, this guard keeps the tests correct even
    /// without that flag (e.g. under a default parallel `cargo test`).
    static EDITOR_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Serializes every test in this file that can make `run_editor` (or
    /// `handle_edit_file`, which calls it) write real ANSI escape sequences
    /// to the process's actual stdout (fd 1) -- `run_editor` does this
    /// unconditionally (`LeaveAlternateScreen`/`EnterAlternateScreen`, raw
    /// mode) and conditionally (`EnableMouseCapture`/`DisableMouseCapture`
    /// when `mouse_capture_enabled`), regardless of the `TestBackend` these
    /// tests otherwise use. Under a default parallel `cargo test` (this
    /// repo's `Makefile` `test:` target has no `--test-threads=1`), two such
    /// tests running concurrently write to the *same* real fd 1 at once;
    /// task 5.4's own test additionally redirects fd 1 to a scratch file for
    /// the duration of one `handle_edit_file` call to inspect exactly what
    /// was written there -- without this lock, another thread's concurrent
    /// escape-sequence writes could land in that same capture window (or in
    /// the real terminal instead, if this lock's own critical section from a
    /// *different* test is what's holding the redirect at that moment),
    /// producing a flaky false positive/negative unrelated to the behavior
    /// under test. Held for the *entire* body of every test that touches
    /// `run_editor`/`handle_edit_file`, task 5.4's redirect-and-restore
    /// section included, not just that section alone.
    static STDOUT_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII guard restoring whatever value (or absence) an env var had
    /// before the test set/unset it, so one test's env mutation never
    /// leaks into the next.
    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            EnvVarGuard { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            EnvVarGuard { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn resolve_editor_prefers_cli_editor_over_env_vars() {
        let _lock = EDITOR_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _visual = EnvVarGuard::set("VISUAL", "should-not-be-used");
        let _editor = EnvVarGuard::set("EDITOR", "should-not-be-used-either");

        // Also covers tokenization: "code -w" -> ["code", "-w"] (requirement
        // 2.1, research.md's whitespace-split rule).
        assert_eq!(
            resolve_editor(Some("code -w")),
            vec!["code".to_string(), "-w".to_string()]
        );
    }

    #[test]
    fn resolve_editor_uses_visual_when_no_cli_editor_is_given() {
        let _lock = EDITOR_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _visual = EnvVarGuard::set("VISUAL", "nano");
        let _editor = EnvVarGuard::unset("EDITOR");

        assert_eq!(resolve_editor(None), vec!["nano".to_string()]);
    }

    #[test]
    fn resolve_editor_uses_editor_when_no_cli_editor_or_visual_is_given() {
        let _lock = EDITOR_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _visual = EnvVarGuard::unset("VISUAL");
        let _editor = EnvVarGuard::set("EDITOR", "emacs -nw");

        assert_eq!(
            resolve_editor(None),
            vec!["emacs".to_string(), "-nw".to_string()]
        );
    }

    #[test]
    fn resolve_editor_falls_back_to_vi_when_nothing_is_configured() {
        let _lock = EDITOR_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _visual = EnvVarGuard::unset("VISUAL");
        let _editor = EnvVarGuard::unset("EDITOR");

        assert_eq!(resolve_editor(None), vec!["vi".to_string()]);
    }

    // --- task 3.2: --editor CLI option ---------------------------------------

    #[test]
    fn cli_parses_the_editor_option_into_args_editor() {
        let args = Args::try_parse_from(["m", "--editor", "code -w"]).expect("should parse");
        assert_eq!(args.editor.as_deref(), Some("code -w"));
    }

    #[test]
    fn cli_editor_option_defaults_to_none_when_not_given() {
        let args = Args::try_parse_from(["m"]).expect("should parse");
        assert_eq!(args.editor, None);
    }

    // --- task 3.3/3.4: run_editor spawns and waits on a real child process --

    /// Writes a "fake editor" shell script to a scratch dir and marks it
    /// executable, so the test suite exercises `run_editor`'s real
    /// `Command::spawn`/`wait` path without depending on any real editor
    /// (`vi`/`code`/...) being installed (3.3's DONE text: "즉시 정상 종료
    /// 하는 테스트 전용 가짜 에디터 스크립트").
    fn write_fake_editor(name: &str, script_body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch_dir(&format!("fake_editor_{name}"));
        let script_path = dir.join("fake-editor.sh");
        fs::write(&script_path, script_body).expect("write fake editor script");
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
            .expect("chmod fake editor script");
        script_path
    }

    #[test]
    fn run_editor_returns_reloaded_when_the_editor_exits_successfully() {
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let script = write_fake_editor("success", "#!/bin/sh\nexit 0\n");
        let target_dir = scratch_dir("run_editor_success_target");
        let target = target_dir.join("doc.md");
        fs::write(&target, "hello").unwrap();

        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let outcome = run_editor(&mut terminal, &target, Some(script.to_str().unwrap()), true);
        assert_eq!(outcome, EditOutcome::Reloaded);

        fs::remove_dir_all(script.parent().unwrap()).ok();
        fs::remove_dir_all(&target_dir).ok();
    }

    #[test]
    fn run_editor_completes_without_reenabling_mouse_capture_that_was_already_off() {
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 3.3's DONE text: "진입 시점에 마우스 캡처가 이미 꺼져 있던 경우
        // 복귀 후에도 켜지지 않는 것까지 같은 테스트로 확인" -- outside a
        // real tty there is no process-observable mouse-capture state to
        // assert on (see this file's module doc comment and this task's
        // design.md adjustment), so what this test mechanically pins down
        // is the input/output contract: passing `mouse_capture_enabled =
        // false` must not panic (i.e. `run_editor` never unconditionally
        // calls `EnableMouseCapture`) and must still resolve to the correct
        // `EditOutcome`. `EnableMouseCapture`/`DisableMouseCapture` are
        // both gated behind `if mouse_capture_enabled` in `run_editor`'s
        // implementation, mirroring `main()`'s own gating.
        let script = write_fake_editor("mouse_off", "#!/bin/sh\nexit 0\n");
        let target_dir = scratch_dir("run_editor_mouse_off_target");
        let target = target_dir.join("doc.md");
        fs::write(&target, "hello").unwrap();

        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let outcome = run_editor(&mut terminal, &target, Some(script.to_str().unwrap()), false);
        assert_eq!(outcome, EditOutcome::Reloaded);

        fs::remove_dir_all(script.parent().unwrap()).ok();
        fs::remove_dir_all(&target_dir).ok();
    }

    #[test]
    fn run_editor_returns_failed_when_the_command_does_not_exist() {
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let target_dir = scratch_dir("run_editor_missing_target");
        let target = target_dir.join("doc.md");
        fs::write(&target, "hello").unwrap();

        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let outcome = run_editor(
            &mut terminal,
            &target,
            Some("definitely-not-a-real-editor-binary-xyz"),
            true,
        );

        match outcome {
            EditOutcome::Failed(message) => {
                assert!(
                    message.contains("definitely-not-a-real-editor-binary-xyz"),
                    "message should name the failed command: {message}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        fs::remove_dir_all(&target_dir).ok();
    }

    #[test]
    fn run_editor_returns_failed_with_a_distinct_message_when_the_editor_exits_nonzero() {
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let script = write_fake_editor("nonzero", "#!/bin/sh\nexit 7\n");
        let target_dir = scratch_dir("run_editor_nonzero_target");
        let target = target_dir.join("doc.md");
        fs::write(&target, "hello").unwrap();

        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let outcome = run_editor(&mut terminal, &target, Some(script.to_str().unwrap()), true);

        match outcome {
            EditOutcome::Failed(message) => {
                assert!(
                    message.contains("exited"),
                    "nonzero-exit message should read differently from a launch failure: {message}"
                );
                assert!(
                    !message.contains("definitely-not-a-real-editor-binary-xyz"),
                    "message should not be the launch-failure message: {message}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        fs::remove_dir_all(script.parent().unwrap()).ok();
        fs::remove_dir_all(&target_dir).ok();
    }

    // --- task 4: handle_edit_file wires Control::EditFile end to end -------

    #[test]
    fn handle_edit_file_reloads_doc_via_action_fs_when_watch_is_live() {
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = temp_kiro_root("edit_live");
        write_requirements_spec(&root, "demo", "# Before\n\noriginal content\n");
        let target = root.join("specs/demo/requirements.md");

        let mut state = build_state_from(&root, (120, 40));
        let width = spec_viewer::app::doc_panel_width(&state);
        state.doc = spec_viewer::app::loader::load_doc(&target, width);
        assert_eq!(state.watch, spec_viewer::app::WatchStatus::Live);

        let script = write_fake_editor(
            "reload_live",
            "#!/bin/sh\ncat > \"$1\" <<'EOF'\n# After\n\nupdated content\nEOF\nexit 0\n",
        );

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let control = handle_edit_file(
            &mut terminal,
            &mut state,
            target.clone(),
            Some(script.to_str().unwrap()),
            true,
        )
        .expect("handle_edit_file should succeed");
        assert_eq!(control, spec_viewer::app::Control::Continue);

        match &state.doc {
            DocView::Rendered { r, .. } => {
                assert!(
                    r.plain.iter().any(|l| l.contains("updated content")),
                    "expected the doc to reload the editor's write, got: {:?}",
                    r.plain
                );
            }
            other => panic!("expected Rendered doc, got {other:?}"),
        }

        fs::remove_dir_all(script.parent().unwrap()).ok();
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn handle_edit_file_skips_reload_but_still_redraws_when_watch_is_manual() {
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = temp_kiro_root("edit_manual");
        write_requirements_spec(&root, "demo", "# Before\n\noriginal content\n");
        let target = root.join("specs/demo/requirements.md");

        // Wide enough that the status bar's full "path · file-info · % ·
        // watch off" line (this scratch path is long) fits without the
        // trailing "watch off" segment getting clipped by the `Paragraph`'s
        // own width-based truncation -- narrower widths below made this
        // test flap on the exact string it's trying to observe.
        let size = (240, 40);
        let mut state = build_state_from(&root, size);
        let width = spec_viewer::app::doc_panel_width(&state);
        state.doc = spec_viewer::app::loader::load_doc(&target, width);
        state.watch = spec_viewer::app::WatchStatus::Manual {
            reason: "--no-watch".to_string(),
        };

        let script = write_fake_editor(
            "reload_manual",
            "#!/bin/sh\ncat > \"$1\" <<'EOF'\n# After\n\nmanual updated content\nEOF\nexit 0\n",
        );

        let backend = TestBackend::new(size.0, size.1);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let control = handle_edit_file(
            &mut terminal,
            &mut state,
            target.clone(),
            Some(script.to_str().unwrap()),
            true,
        )
        .expect("handle_edit_file should succeed");
        assert_eq!(control, spec_viewer::app::Control::Continue);

        // Requirement 3.2: no auto-reload under Manual watch -- the doc panel
        // keeps showing the pre-edit content even though the file on disk
        // changed underneath it.
        match &state.doc {
            DocView::Rendered { r, .. } => {
                assert!(
                    r.plain.iter().any(|l| l.contains("original content")),
                    "expected the doc to still show pre-edit content, got: {:?}",
                    r.plain
                );
                assert!(
                    !r.plain.iter().any(|l| l.contains("manual updated content")),
                    "expected no auto-reload under Manual watch, got: {:?}",
                    r.plain
                );
            }
            other => panic!("expected Rendered doc, got {other:?}"),
        }

        // "화면은 항상 다시 그려진다" (requirement 1.2): `run_editor` leaves the
        // terminal buffer blank via its own `terminal.clear()`, and the
        // Manual branch sends no further `step`/`Action` -- so this must
        // still draw explicitly, or the buffer would stay blank instead of
        // showing the real frame (e.g. the "watch off" status-bar segment
        // Manual watch renders).
        let buffer = terminal.backend().buffer().clone();
        assert!(
            buffer_contains(&buffer, "watch off"),
            "expected a real redrawn frame (with the Manual watch-off status segment) after handle_edit_file, not a blank post-clear buffer"
        );

        fs::remove_dir_all(script.parent().unwrap()).ok();
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn handle_edit_file_surfaces_edit_failed_popup_when_the_editor_fails() {
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = temp_kiro_root("edit_failed");
        write_requirements_spec(&root, "demo", "# Before\n\noriginal content\n");
        let target = root.join("specs/demo/requirements.md");

        let mut state = build_state_from(&root, (120, 40));
        let width = spec_viewer::app::doc_panel_width(&state);
        state.doc = spec_viewer::app::loader::load_doc(&target, width);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let control = handle_edit_file(
            &mut terminal,
            &mut state,
            target.clone(),
            Some("definitely-not-a-real-editor-binary-xyz"),
            true,
        )
        .expect("handle_edit_file should succeed");
        assert_eq!(control, spec_viewer::app::Control::Continue);

        match &state.popup {
            Some(Popup::Message(msg)) => {
                assert!(
                    msg.contains("definitely-not-a-real-editor-binary-xyz"),
                    "expected the EditFailed message to name the failed command: {msg}"
                );
            }
            other => panic!("expected Some(Popup::Message(_)), got {other:?}"),
        }

        fs::remove_dir_all(&root).ok();
    }

    // --- task 5: E2E -- Action::Edit through handle_edit_file, full pipeline
    //
    // Unlike task 4's tests above (which call `handle_edit_file` directly
    // with a hand-picked path), each scenario here first drives
    // `spec_viewer::app::update(&mut state, Action::Edit)` -- the exact call
    // `run_loop`'s key-read branch makes once the `'e'` keymap binding
    // decodes to `Action::Edit` -- and only then feeds the `Control::EditFile`
    // path it returns into `handle_edit_file`, so the whole "키 입력 -> 편집
    // 대상 판정 -> 에디터 실행 -> 반영" pipeline is exercised end to end, not
    // just its tail half.

    #[test]
    fn e2e_live_watch_auto_reflects_saved_edit_through_the_action_edit_pipeline() {
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Task 5.1 / requirement 3.1: a real temp `.kiro` tree + a fake
        // editor script that actually rewrites the file on disk, driven
        // through `Action::Edit` -> `Control::EditFile` -> `handle_edit_file`,
        // must show the new content on the very next frame with no separate
        // refresh key.
        let root = temp_kiro_root("e2e_live_saved");
        write_requirements_spec(&root, "e2e-demo", "# Doc\n\nORIGINAL_MARKER_TEXT\n");
        let target = root.join("specs/e2e-demo/requirements.md");

        let mut state = build_state_from(&root, (120, 40)); // WatchStatus::Live
        let width = spec_viewer::app::doc_panel_width(&state);
        state.doc = spec_viewer::app::loader::load_doc(&target, width);
        assert_eq!(state.watch, spec_viewer::app::WatchStatus::Live);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");

        // Drive the real entry point: this is what turns the `'e'` keymap
        // binding into a `Control::EditFile(path)` for `run_loop` to act on.
        let path = match spec_viewer::app::update(&mut state, Action::Edit) {
            spec_viewer::app::Control::EditFile(path) => path,
            other => panic!("expected Control::EditFile from Action::Edit, got {other:?}"),
        };
        assert_eq!(path, target);

        let script = write_fake_editor(
            "e2e_live_saved",
            "#!/bin/sh\necho 'UPDATED_MARKER_TEXT' > \"$1\"\nexit 0\n",
        );

        let control = handle_edit_file(
            &mut terminal,
            &mut state,
            path,
            Some(script.to_str().unwrap()),
            true,
        )
        .expect("handle_edit_file should succeed");
        assert_eq!(control, spec_viewer::app::Control::Continue);

        // `handle_edit_file`'s own Live/Reloaded branch already dispatches
        // `Action::Fs` via `step` (update + draw) internally -- the screen is
        // already showing the reloaded content by the time this call
        // returns, with no further key input or explicit draw needed here
        // (tasks.md 5.1's "별도 새로고침 키 입력 없이").
        let buffer = terminal.backend().buffer().clone();
        assert!(
            find_text_cell(&buffer, "UPDATED_MARKER_TEXT").is_some(),
            "expected the auto-reloaded content visible on the very next frame"
        );
        assert!(
            find_text_cell(&buffer, "ORIGINAL_MARKER_TEXT").is_none(),
            "expected the pre-edit marker to be gone once auto-reload has happened"
        );

        fs::remove_dir_all(script.parent().unwrap()).ok();
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn e2e_no_watch_manual_status_skips_auto_reload_and_shows_watch_off_guidance() {
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Task 5.2 / requirement 3.2: same scenario as 5.1, but with
        // `WatchStatus::Manual` (what `--no-watch` produces at startup) --
        // the fake editor still rewrites the file on disk, but the doc panel
        // must keep showing pre-edit content, and the status bar must carry
        // the manual-refresh guidance (`ui::status_bar`'s "watch off"
        // segment, the only such guidance requirement 7.6 wires up).
        //
        // `AppState::new` is called directly here (not via `build_state_from`,
        // which hardcodes `WatchStatus::Live`) to get `Manual` instead --
        // per the brief, that shared helper is left untouched so the other
        // tests using it keep their existing behavior.
        //
        // Width 240 (not the 120 most other scenarios here use): task 4's
        // review found the status bar's full "path · file-info · % · watch
        // off" line gets clipped by the `Paragraph`'s width-based truncation
        // on a narrower terminal, which would make the "watch off" assertion
        // below flap on terminal width rather than on real behavior.
        let root = temp_kiro_root("e2e_manual_saved");
        write_requirements_spec(&root, "e2e-demo", "# Doc\n\nORIGINAL_MARKER_TEXT\n");
        let target = root.join("specs/e2e-demo/requirements.md");

        let size = (240, 40);
        let snapshot = spec_viewer::app::load_snapshot(&root);
        let spec_root = spec_viewer::spec::build(&snapshot);
        let mut state = spec_viewer::app::AppState::new(
            spec_viewer::spec::TreeSource::Kiro(spec_root),
            root.clone(),
            size,
            spec_viewer::app::WatchStatus::Manual {
                reason: "--no-watch".to_string(),
            },
            spec_viewer::app::TreeMode::Auto,
            true,
        );
        let width = spec_viewer::app::doc_panel_width(&state);
        state.doc = spec_viewer::app::loader::load_doc(&target, width);

        let backend = TestBackend::new(size.0, size.1);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let path = match spec_viewer::app::update(&mut state, Action::Edit) {
            spec_viewer::app::Control::EditFile(path) => path,
            other => panic!("expected Control::EditFile from Action::Edit, got {other:?}"),
        };

        let script = write_fake_editor(
            "e2e_manual_saved",
            "#!/bin/sh\necho 'UPDATED_MARKER_TEXT' > \"$1\"\nexit 0\n",
        );

        let control = handle_edit_file(
            &mut terminal,
            &mut state,
            path,
            Some(script.to_str().unwrap()),
            true,
        )
        .expect("handle_edit_file should succeed");
        assert_eq!(control, spec_viewer::app::Control::Continue);

        let buffer = terminal.backend().buffer().clone();
        assert!(
            find_text_cell(&buffer, "ORIGINAL_MARKER_TEXT").is_some(),
            "expected the doc panel to still show pre-edit content under --no-watch"
        );
        assert!(
            find_text_cell(&buffer, "UPDATED_MARKER_TEXT").is_none(),
            "expected no auto-reload under --no-watch even though the file changed on disk"
        );
        assert!(
            buffer_contains(&buffer, "watch off"),
            "expected the status bar's manual-refresh guidance ('watch off') to be visible"
        );

        fs::remove_dir_all(script.parent().unwrap()).ok();
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn e2e_editor_exits_without_saving_leaves_doc_content_unchanged() {
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Task 5.3 / requirement 3.3: live watch, but the fake editor exits
        // clean without touching the target file at all -- the doc panel's
        // content must remain exactly what it was before the edit, whether
        // or not `Action::Fs` ends up dispatched (it does, on `Reloaded`,
        // but reloading unchanged content is a no-op the doc panel should
        // show as unchanged either way).
        let root = temp_kiro_root("e2e_no_save");
        write_requirements_spec(&root, "e2e-demo", "# Doc\n\nORIGINAL_MARKER_TEXT\n");
        let target = root.join("specs/e2e-demo/requirements.md");

        let mut state = build_state_from(&root, (120, 40));
        let width = spec_viewer::app::doc_panel_width(&state);
        state.doc = spec_viewer::app::loader::load_doc(&target, width);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let path = match spec_viewer::app::update(&mut state, Action::Edit) {
            spec_viewer::app::Control::EditFile(path) => path,
            other => panic!("expected Control::EditFile from Action::Edit, got {other:?}"),
        };

        // Deliberately leaves the target file untouched -- exits clean
        // without writing anything (task 5.3: "저장하지 않고 종료").
        let script = write_fake_editor("e2e_no_save", "#!/bin/sh\nexit 0\n");

        let control = handle_edit_file(
            &mut terminal,
            &mut state,
            path,
            Some(script.to_str().unwrap()),
            true,
        )
        .expect("handle_edit_file should succeed");
        assert_eq!(control, spec_viewer::app::Control::Continue);

        let buffer = terminal.backend().buffer().clone();
        assert!(
            find_text_cell(&buffer, "ORIGINAL_MARKER_TEXT").is_some(),
            "expected the doc content unchanged after exiting the editor without saving"
        );

        fs::remove_dir_all(script.parent().unwrap()).ok();
        fs::remove_dir_all(&root).ok();
    }

    /// RAII guard for task 5.4's real-fd-1 redirection below: restores the
    /// original fd 1 on drop (including on an assertion panic mid-test), so
    /// a failure here never leaves the rest of the test process's stdout
    /// pointed at a scratch file.
    #[cfg(unix)]
    struct StdoutFdGuard {
        saved_fd: i32,
    }

    #[cfg(unix)]
    impl StdoutFdGuard {
        fn redirect_to(file: &fs::File) -> Self {
            use std::os::unix::io::AsRawFd;
            unsafe extern "C" {
                fn dup(fd: i32) -> i32;
                fn dup2(oldfd: i32, newfd: i32) -> i32;
            }
            let saved_fd = unsafe { dup(1) };
            assert!(saved_fd >= 0, "failed to dup fd 1");
            let result = unsafe { dup2(file.as_raw_fd(), 1) };
            assert!(result >= 0, "failed to dup2 the capture file onto fd 1");
            StdoutFdGuard { saved_fd }
        }
    }

    #[cfg(unix)]
    impl Drop for StdoutFdGuard {
        fn drop(&mut self) {
            unsafe extern "C" {
                fn dup2(oldfd: i32, newfd: i32) -> i32;
                fn close(fd: i32) -> i32;
            }
            unsafe {
                dup2(self.saved_fd, 1);
                close(self.saved_fd);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn e2e_mouse_capture_already_off_before_edit_stays_off_after_returning() {
        // Task 5.4 / requirement 5.1: a session that entered edit mode with
        // mouse capture already disabled (e.g. `EnableMouseCapture` failed at
        // startup, requirement 9.8) must not have it silently re-enabled once
        // the editor returns.
        //
        // `run_editor`'s `EnableMouseCapture`/`DisableMouseCapture` calls
        // write real ANSI bytes to the process's actual stdout (fd 1) via
        // `crossterm::execute!(std::io::stdout(), ...)`, not into any
        // `TestBackend` buffer -- this file's own module doc comment already
        // establishes that real terminal I/O like this has no
        // `TestBackend`-buffer equivalent to assert against. The only way to
        // mechanically observe (or fail to observe) those bytes is to
        // redirect the real fd 1 to a file for the duration of the call and
        // inspect it afterward. `dup`/`dup2`/`close` are declared directly
        // here rather than adding a `libc` Cargo dependency (research.md:
        // no new external libraries) -- every Rust binary already links
        // against libc, so this introduces nothing new.
        //
        // crossterm 0.29's `EnableMouseCapture::write_ansi` (src/event.rs)
        // writes `"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h"`;
        // this test looks for the first of those five sequences.
        //
        // Held for this entire test, the fd-1 redirect-and-restore section
        // (`StdoutFdGuard::redirect_to` through its `Drop`) included: any
        // other test in this file concurrently writing real ANSI bytes to
        // fd 1 while it is redirected here would land in this test's own
        // capture file instead, and any such test running while *this* test
        // holds the redirect would have its bytes silently swallowed into
        // that same file rather than reaching the real terminal.
        let _guard = STDOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = temp_kiro_root("e2e_mouse_off");
        write_requirements_spec(&root, "e2e-demo", "# Doc\n\nORIGINAL_MARKER_TEXT\n");
        let target = root.join("specs/e2e-demo/requirements.md");

        let mut state = build_state_from(&root, (120, 40));
        let width = spec_viewer::app::doc_panel_width(&state);
        state.doc = spec_viewer::app::loader::load_doc(&target, width);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");

        let path = match spec_viewer::app::update(&mut state, Action::Edit) {
            spec_viewer::app::Control::EditFile(path) => path,
            other => panic!("expected Control::EditFile from Action::Edit, got {other:?}"),
        };

        let script = write_fake_editor(
            "e2e_mouse_off",
            "#!/bin/sh\necho 'UPDATED_MARKER_TEXT' > \"$1\"\nexit 0\n",
        );

        let capture_path = root.join("stdout-capture.bin");
        let capture_file = fs::File::create(&capture_path).expect("create capture file");
        let control_result;
        {
            // Scoped so the fd-1 redirection is restored (via `Drop`) before
            // this test reads the capture file back or makes any assertion.
            let _fd_guard = StdoutFdGuard::redirect_to(&capture_file);
            control_result = handle_edit_file(
                &mut terminal,
                &mut state,
                path,
                Some(script.to_str().unwrap()),
                false, // mouse capture was already off entering edit mode
            );
        }

        let control = control_result.expect("handle_edit_file should succeed");
        assert_eq!(control, spec_viewer::app::Control::Continue);

        let captured = fs::read(&capture_path).unwrap_or_default();
        let captured_text = String::from_utf8_lossy(&captured);
        assert!(
            !captured_text.contains("\x1b[?1000h"),
            "expected no mouse-capture-enable escape sequence written to stdout when mouse \
             capture was already off, got bytes: {captured:?}"
        );

        // Sanity: the rest of the pipeline still ran normally (doc reloaded,
        // no panic) even with mouse capture disabled throughout -- without
        // this, the escape-sequence assertion above could pass vacuously
        // against a pipeline that silently did nothing at all.
        let buffer = terminal.backend().buffer().clone();
        assert!(
            find_text_cell(&buffer, "UPDATED_MARKER_TEXT").is_some(),
            "expected the pipeline to still complete normally with mouse capture disabled"
        );

        fs::remove_dir_all(script.parent().unwrap()).ok();
        fs::remove_dir_all(&root).ok();
    }

    // --- spec-viewer-spec-kit-support task 5: E2E verification (5.1-5.5) --
    //
    // Tasks 1-4 of this spec (Milestone/warning/kiro_meta fields,
    // `spec::spec_kit`, `TreeSource::SpecKit` + shared `milestone_badge_spans`
    // rendering, `resolve_source`'s `.kiro`/spec-kit priority) are already
    // implemented and approved. This section only adds verification on top
    // of that surface, per this task's boundary (no non-test production
    // code changes anywhere in this file or elsewhere).

    /// Byte-offset-safe cell lookup for task 5's badge style assertions.
    /// `find_text_cell` above treats a row's byte offset as its column
    /// directly ("ASCII-only substring" per its own doc comment), which
    /// silently drifts on any row where a multi-byte glyph -- this tree's
    /// own box-drawing border characters, or the `▶`/`▼` expand indicator --
    /// precedes the needle, exactly the class of bug
    /// `ui::tree_panel::tests::find_cell` documents and fixes for its own
    /// module's tests via this same `cell_of_byte` byte-to-cell mapping.
    /// Returns `(x, y)` (unlike `find_text_cell`'s `(y, x)`) to match
    /// `cell_style`'s own `(x, y)` parameter order directly.
    fn find_cell_exact(buffer: &ratatui::buffer::Buffer, needle: &str) -> (u16, u16) {
        let area = buffer.area();
        for y in 0..area.height {
            let mut row = String::new();
            let mut cell_of_byte: Vec<u16> = Vec::new();
            for x in 0..area.width {
                let symbol = buffer[(x, y)].symbol();
                cell_of_byte.extend(std::iter::repeat(x).take(symbol.len()));
                row.push_str(symbol);
            }
            if let Some(byte_idx) = row.find(needle) {
                return (cell_of_byte[byte_idx], y);
            }
        }
        panic!("expected a row containing {needle:?}");
    }

    /// Writes a `.kiro` spec whose `spec.json` has all four canonical
    /// approval gates (`requirements`/`bizProcess`/`design`/`tasks`)
    /// generated+approved, at the given `phase` -- the exact shape
    /// `src/spec/mod.rs`'s own `ALL_FOUR_GATES_APPROVED` unit tests use, so
    /// this test exercises the identical fixture shape end to end through
    /// `resolve_source` + real rendering instead of `build_spec` alone.
    fn write_kiro_spec_all_gates_approved(root: &Path, name: &str, phase: &str) {
        let dir = root.join("specs").join(name);
        fs::create_dir_all(&dir).unwrap();
        let spec_json = format!(
            "{{\n  \"name\": \"{name}\",\n  \"phase\": \"{phase}\",\n  \"approvals\": {{\n    \"requirements\": {{ \"generated\": true, \"approved\": true }},\n    \"bizProcess\": {{ \"generated\": true, \"approved\": true }},\n    \"design\": {{ \"generated\": true, \"approved\": true }},\n    \"tasks\": {{ \"generated\": true, \"approved\": true }}\n  }}\n}}\n"
        );
        fs::write(dir.join("spec.json"), spec_json).unwrap();
        fs::write(dir.join("requirements.md"), "# demo\n").unwrap();
    }

    // --- 5.1: `.kiro` non-regression -- all-gates-approved-but-not-completed
    // (4/4) must render differently from completed (5/5) (requirements
    // 5.1-5.5 of this spec's own spec.json, exercised through the real
    // startup + render path rather than `build_spec` alone).
    #[test]
    fn task5_1_kiro_all_gates_approved_distinguishes_completed_from_in_progress() {
        let root = scratch_dir("task5_1_gates");
        fs::create_dir_all(root.join(".kiro/specs")).unwrap();
        let kiro = root.join(".kiro");
        write_kiro_spec_all_gates_approved(&kiro, "gates-done-not-completed", "tasks-approved");
        write_kiro_spec_all_gates_approved(&kiro, "gates-done-completed", "completed");

        let args = args_with_path(Some(root.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));
        assert!(matches!(startup.source, spec_viewer::spec::TreeSource::Kiro(_)));

        let mut state = spec_viewer::app::AppState::new(
            startup.source,
            startup.root,
            (120, 40),
            spec_viewer::app::WatchStatus::Live,
            spec_viewer::app::TreeMode::Auto,
            true,
        );
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| spec_viewer::ui::render(f, &mut state)).expect("draw");
        let buffer = terminal.backend().buffer().clone();

        assert!(
            buffer_contains(&buffer, "gates-done-not-completed 4/4"),
            "expected the in-progress-but-fully-approved spec to show 4/4 -- got buffer:\n{:?}",
            (0..buffer.area().height)
                .map(|y| (0..buffer.area().width).map(|x| buffer[(x, y)].symbol()).collect::<String>())
                .collect::<Vec<_>>()
        );
        assert!(
            buffer_contains(&buffer, "gates-done-completed 5/5"),
            "expected the completed spec to show a distinct 5/5, not collapse to 4/4"
        );

        let (col_4, row_4) = find_cell_exact(&buffer, "4/4");
        let (fg_4, _bg_4, mod_4) = cell_style(&buffer, col_4, row_4);
        assert!(mod_4.contains(ratatui::style::Modifier::BOLD), "4/4 badge should be bold");
        assert_eq!(fg_4, ratatui::style::Color::Green, "4/4 badge should be green");

        let (col_5, row_5) = find_cell_exact(&buffer, "5/5");
        let (fg_5, _bg_5, mod_5) = cell_style(&buffer, col_5, row_5);
        assert!(mod_5.contains(ratatui::style::Modifier::BOLD), "5/5 badge should be bold");
        assert_eq!(fg_5, ratatui::style::Color::Green, "5/5 badge should be green");

        fs::remove_dir_all(&root).ok();
    }

    // --- 5.2: `bugfix-only` fixture's badge tracks its *actual* recorded
    // gate count, not an inflated/hardcoded one (requirement 5.4). ---------
    #[test]
    fn task5_2_bugfix_only_fixture_badge_reflects_actual_recorded_gate_count() {
        // `tests/fixtures/kiro/specs/bugfix-only/spec.json` records exactly
        // three approval-gate keys (`bugfix`: approved, `design`: generated
        // only, `tasks`: neither) -- one of three actually approved, so the
        // badge must read "1/3", never a fabricated "1/1" (as if only the
        // single approved gate were ever recorded) nor any other count.
        let snapshot = spec_viewer::app::load_snapshot(&fixtures_root());
        let spec_root = spec_viewer::spec::build(&snapshot);
        let spec = spec_root
            .specs
            .iter()
            .find(|s| s.name == "bugfix-only")
            .expect("bugfix-only spec present in fixtures/kiro");

        assert_eq!(
            spec.milestones.len(),
            3,
            "expected exactly 3 recorded approval gates: {:?}",
            spec.milestones
        );
        assert_eq!(
            spec.milestones.iter().filter(|m| m.done).count(),
            1,
            "expected exactly 1 of the 3 recorded gates to be approved: {:?}",
            spec.milestones
        );

        let mut state = spec_viewer::app::AppState::new(
            spec_viewer::spec::TreeSource::Kiro(spec_root),
            fixtures_root(),
            (120, 40),
            spec_viewer::app::WatchStatus::Live,
            spec_viewer::app::TreeMode::Auto,
            true,
        );
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| spec_viewer::ui::render(f, &mut state)).expect("draw");
        let buffer = terminal.backend().buffer().clone();

        assert!(
            buffer_contains(&buffer, "bugfix-only 1/3"),
            "expected the badge to reflect the real 1-of-3 recorded gates"
        );
        assert!(
            !buffer_contains(&buffer, "bugfix-only 1/1"),
            "must not report an inflated/fabricated gate total that doesn't match spec.json"
        );
    }

    // --- 5.3: spec-kit temporary project E2E (requirements 2.1-2.5, 6.1,
    // 6.2, 7.1, 7.2) --------------------------------------------------------

    /// Builds a spec-kit project directly under `root`: `.specify/` marker,
    /// `specs/001-login/{spec,plan,tasks}.md` (tasks.md has 1-of-2 checked
    /// boxes) and `specs/002-billing/spec.md` only.
    fn build_spec_kit_project(root: &Path) {
        fs::create_dir_all(root.join(".specify")).unwrap();
        let specs_dir = root.join("specs");

        let login = specs_dir.join("001-login");
        fs::create_dir_all(&login).unwrap();
        fs::write(login.join("spec.md"), "# Login spec\n\nSome content.\n").unwrap();
        fs::write(login.join("plan.md"), "# Login plan\n").unwrap();
        fs::write(login.join("tasks.md"), "- [x] a\n- [ ] b\n").unwrap();

        let billing = specs_dir.join("002-billing");
        fs::create_dir_all(&billing).unwrap();
        fs::write(billing.join("spec.md"), "# Billing spec\n").unwrap();
    }

    #[test]
    fn task5_3_spec_kit_temp_project_startup_selects_spec_kit_source() {
        let root = scratch_dir("task5_3_startup");
        build_spec_kit_project(&root);

        let args = args_with_path(Some(root.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        match &startup.source {
            spec_viewer::spec::TreeSource::SpecKit(features) => {
                assert_eq!(features.len(), 2);
                assert!(features.iter().any(|f| f.name == "001-login"));
                assert!(features.iter().any(|f| f.name == "002-billing"));
            }
            _ => panic!("expected TreeSource::SpecKit"),
        }

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn task5_3_spec_kit_temp_project_tree_shows_badges_order_and_no_steering() {
        let root = scratch_dir("task5_3_tree");
        build_spec_kit_project(&root);

        let args = args_with_path(Some(root.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        let mut state = spec_viewer::app::AppState::new(
            startup.source,
            startup.root,
            (120, 40),
            spec_viewer::app::WatchStatus::Live,
            spec_viewer::app::TreeMode::Auto,
            true,
        );
        state.tree.open(vec![NodeId::Spec("001-login".to_string())]);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| spec_viewer::ui::render(f, &mut state)).expect("draw");
        let buffer = terminal.backend().buffer().clone();

        // Milestone badges: 001-login has spec.md+plan.md+tasks.md (3/3,
        // bold+green); 002-billing has only spec.md (1/3, not emphasized).
        assert!(buffer_contains(&buffer, "001-login 3/3"));
        assert!(buffer_contains(&buffer, "002-billing 1/3"));
        let (col_3_3, row_3_3) = find_cell_exact(&buffer, "3/3");
        let (fg_3_3, _, mod_3_3) = cell_style(&buffer, col_3_3, row_3_3);
        assert!(mod_3_3.contains(ratatui::style::Modifier::BOLD));
        assert_eq!(fg_3_3, ratatui::style::Color::Green);
        let (col_1_3, row_1_3) = find_cell_exact(&buffer, "1/3");
        let (_, _, mod_1_3) = cell_style(&buffer, col_1_3, row_1_3);
        assert!(!mod_1_3.contains(ratatui::style::Modifier::BOLD));

        // Canonical child order: spec.md -> plan.md -> tasks.md, all after
        // the feature's own row.
        let (row_feature, _) = find_text_cell(&buffer, "001-login 3/3").expect("feature row visible");
        let (row_spec_md, _) = find_text_cell(&buffer, "spec.md").expect("spec.md row visible");
        let (row_plan_md, _) = find_text_cell(&buffer, "plan.md").expect("plan.md row visible");
        let (row_tasks_md, _) = find_text_cell(&buffer, "tasks.md").expect("tasks.md row visible");
        assert!(row_feature < row_spec_md);
        assert!(row_spec_md < row_plan_md);
        assert!(row_plan_md < row_tasks_md);

        // spec-kit has no Steering concept at all (design.md Out-of-Scope).
        assert!(
            !buffer_contains(&buffer, "Steering"),
            "spec-kit tree must never show a Steering group"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    // Fixed: doc_item() now matches on the computed `filename` ("tasks.md")
    // rather than the `DocKind::Tasks` variant alone, so spec-kit's
    // `DocKind::Other("tasks.md")` tasks.md slot gets its progress badge too.
    fn task5_3_spec_kit_tasks_md_shows_checkbox_progress_requirement_6_1() {
        let root = scratch_dir("task5_3_tasks_progress");
        build_spec_kit_project(&root);

        let args = args_with_path(Some(root.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        let mut state = spec_viewer::app::AppState::new(
            startup.source,
            startup.root,
            (120, 40),
            spec_viewer::app::WatchStatus::Live,
            spec_viewer::app::TreeMode::Auto,
            true,
        );
        state.tree.open(vec![NodeId::Spec("001-login".to_string())]);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| spec_viewer::ui::render(f, &mut state)).expect("draw");
        let buffer = terminal.backend().buffer().clone();

        assert!(
            buffer_contains(&buffer, "tasks.md 1/2"),
            "requirement 6.1: tasks.md's checkbox progress (1 of 2 checked) must show in the tree"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn task5_3_spec_kit_doc_selection_loads_real_file_content() {
        let root = scratch_dir("task5_3_doc_load");
        build_spec_kit_project(&root);
        let spec_md_path = root.join("specs").join("001-login").join("spec.md");

        let args = args_with_path(Some(root.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));

        let mut state = spec_viewer::app::AppState::new(
            startup.source,
            startup.root,
            (120, 40),
            spec_viewer::app::WatchStatus::Live,
            spec_viewer::app::TreeMode::Auto,
            true,
        );
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");

        state.tree.select(vec![
            NodeId::Spec("001-login".to_string()),
            NodeId::Doc("001-login".to_string(), DocKind::Other("spec.md".to_string())),
        ]);
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)))
            .expect("step should succeed");

        match &state.doc {
            DocView::Rendered { path, .. } => assert_eq!(path, &spec_md_path),
            other => panic!("expected DocView::Rendered for spec.md, got {other:?}"),
        }

        let buffer = terminal.backend().buffer().clone();
        assert!(
            buffer_contains(&buffer, "Login spec"),
            "expected the real spec.md heading text rendered in the doc panel"
        );

        fs::remove_dir_all(&root).ok();
    }

    // --- 5.4: editor mode applies to a spec-kit doc with no extra wiring
    // (requirement 9.1) -- same `Action::Edit` -> `Control::EditFile`
    // pipeline the pre-existing spec-viewer-editor-mode tests above already
    // exercise for `.kiro` docs, just pointed at a spec-kit fixture. -------
    #[test]
    fn task5_4_spec_kit_doc_edit_action_returns_control_edit_file() {
        let root = scratch_dir("task5_4_edit");
        build_spec_kit_project(&root);
        let specs_dir = root.join("specs");
        let target = specs_dir.join("001-login").join("spec.md");

        let features = spec_viewer::spec::spec_kit::build(&specs_dir);
        let mut state = spec_viewer::app::AppState::new(
            spec_viewer::spec::TreeSource::SpecKit(features),
            specs_dir.clone(),
            (120, 40),
            spec_viewer::app::WatchStatus::Live,
            spec_viewer::app::TreeMode::Auto,
            true,
        );
        let width = spec_viewer::app::doc_panel_width(&state);
        state.doc = spec_viewer::app::loader::load_doc(&target, width);
        match &state.doc {
            DocView::Rendered { .. } => {}
            other => panic!("expected DocView::Rendered before editing, got {other:?}"),
        }

        // The exact call `run_loop` makes on the real `e` keypress (via
        // `handle_key` -> `keymap` -> `Action::Edit`) -- proving the
        // pre-existing editor-mode pipeline triggers unmodified for a
        // spec-kit-sourced doc, with no additional wiring from this spec.
        match spec_viewer::app::update(&mut state, Action::Edit) {
            spec_viewer::app::Control::EditFile(path) => assert_eq!(path, target),
            other => panic!(
                "expected Control::EditFile from Action::Edit for a spec-kit doc, got {other:?}"
            ),
        }

        fs::remove_dir_all(&root).ok();
    }

    // --- 5.5: read-only invariant -- browsing a spec-kit project never
    // writes to any file under it (requirement 8.1). -----------------------

    fn collect_mtimes(dir: &Path) -> std::collections::BTreeMap<PathBuf, std::time::SystemTime> {
        fn walk(dir: &Path, map: &mut std::collections::BTreeMap<PathBuf, std::time::SystemTime>) {
            let Ok(read_dir) = fs::read_dir(dir) else { return };
            for entry in read_dir.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, map);
                } else if let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) {
                    map.insert(path, modified);
                }
            }
        }
        let mut map = std::collections::BTreeMap::new();
        walk(dir, &mut map);
        map
    }

    #[test]
    fn task5_5_spec_kit_project_files_are_never_modified_by_browsing() {
        let root = scratch_dir("task5_5_readonly");
        build_spec_kit_project(&root);

        let before = collect_mtimes(&root);

        let args = args_with_path(Some(root.clone()));
        let startup = resolve_source(&args).unwrap_or_else(|e| panic!("expected Ok, got {e:?}"));
        let specs_dir = startup.root.clone();

        // An in-memory sort of a freshly-built `Vec<Spec>` -- spec-kit has
        // no `--sort` concept at startup (design.md Out-of-Scope), but this
        // still exercises `sort_specs` against real spec-kit `Spec` values
        // without touching the filesystem at all.
        let mut features_for_sort = spec_viewer::spec::spec_kit::build(&specs_dir);
        spec_viewer::spec::sort_specs(&mut features_for_sort, spec_viewer::spec::SortKey::Name);

        let mut state = spec_viewer::app::AppState::new(
            startup.source,
            startup.root,
            (120, 40),
            spec_viewer::app::WatchStatus::Live,
            spec_viewer::app::TreeMode::Auto,
            true,
        );
        state.tree.open(vec![NodeId::Spec("001-login".to_string())]);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| spec_viewer::ui::render(f, &mut state)).expect("draw");

        state.tree.select(vec![
            NodeId::Spec("001-login".to_string()),
            NodeId::Doc("001-login".to_string(), DocKind::Other("spec.md".to_string())),
        ]);
        step(&mut terminal, &mut state, Action::Key(key(KeyCode::Enter)))
            .expect("step should succeed");

        // A manual refresh (the `r`-key / `Action::Refresh` path) forces a
        // full `spec::spec_kit::build` rescan from disk, same as a live
        // filesystem-watch event would.
        assert_eq!(
            spec_viewer::app::update(&mut state, spec_viewer::app::Action::Refresh),
            spec_viewer::app::Control::Continue
        );
        terminal.draw(|f| spec_viewer::ui::render(f, &mut state)).expect("draw");

        let after = collect_mtimes(&root);
        assert_eq!(
            before, after,
            "expected no file under the spec-kit project to be modified by browsing/refreshing it"
        );

        fs::remove_dir_all(&root).ok();
    }
}
