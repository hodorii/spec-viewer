//! File I/O -> `spec::DirSnapshot` (type defined in `spec`, re-exported here;
//! see design.md's File Structure Plan note and `spec::mod`'s own
//! `DirSnapshot` doc comment for the placement rationale: `spec::build()`'s
//! signature needs the type, and `spec` must never depend on `app`).
//!
//! Also hosts the per-document loaders that turn a real filesystem path (or
//! an already-built `spec::Spec`) into an [`super::DocView`] for the doc
//! panel (requirements 2.4, 2.6, 3.6, 3.7, 8.1, 8.3, 8.4).

use std::fs;
use std::path::Path;
use std::time::SystemTime;

pub use crate::spec::{DirSnapshot, FileSnapshot, SpecDirSnapshot};

use crate::markdown;
use crate::spec::{DocKind, Spec};

use super::{DocView, FileInfo};

/// Width consumed by the doc panel's own frame (`ui::doc_panel` renders every
/// `DocView` inside `Block::bordered()`, i.e. `Borders::ALL` — one column of
/// border on each side).  The renderers below are handed the panel's *outer*
/// width by every caller (`main.rs` file view passes the full terminal width;
/// `app` tree view passes the frame width), so markdown is rendered at
/// `outer - DOC_PANEL_BORDER` to guarantee no content line can overflow the
/// panel's inner area (요구 5.4: "오른쪽 테두리가 문서 패널 테두리 안쪽").
///
/// Lives here in the `app` layer rather than in `ui` because `app` must never
/// depend on `ui` (layering in lib.rs); the constant's value is duplicated
/// nowhere else.
const DOC_PANEL_BORDER: u16 = 2;

/// The doc panel's inner (content) width for a given outer width.
fn panel_inner_width(outer: u16) -> u16 {
    outer.saturating_sub(DOC_PANEL_BORDER)
}

/// Read a `.kiro` root directory (`root/specs/*` and `root/steering/*.md`)
/// into an in-memory [`DirSnapshot`], suitable for handing to
/// `spec::build`.
///
/// - Each direct subdirectory of `root/specs` becomes one
///   [`SpecDirSnapshot`]; only regular files directly inside it (no
///   recursion) become its [`FileSnapshot`]s.
/// - Each `*.md` file directly inside `root/steering` becomes one
///   [`FileSnapshot`] in the steering list.
/// - Dotfiles/dot-directories and nested subdirectories inside a spec dir
///   are skipped rather than erroring.
/// - A missing `root/specs` or `root/steering` is treated as zero entries
///   for that list, not an error.
/// - Content is decoded with `String::from_utf8_lossy` (requirement 8.4):
///   non-UTF-8 bytes become `U+FFFD`, never an error.
/// - No sorting is performed here — whatever order `fs::read_dir` yields is
///   preserved, per `spec::build`'s own "caller decides ordering" contract.
pub fn load_snapshot(root: &Path) -> DirSnapshot {
    let specs = read_spec_dirs(&root.join("specs"));
    let steering = read_steering_files(&root.join("steering"));
    DirSnapshot { specs, steering }
}

fn read_spec_dirs(specs_dir: &Path) -> Vec<SpecDirSnapshot> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(specs_dir) else {
        return out;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }

        let files = read_spec_files(&path);
        out.push(SpecDirSnapshot {
            name,
            dir: path,
            files,
        });
    }

    out
}

fn read_spec_files(dir: &Path) -> Vec<FileSnapshot> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // Direct files only — nested directories inside a spec dir are
        // skipped rather than recursed into or treated as an error.
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let content = String::from_utf8_lossy(&bytes).into_owned();
        out.push(FileSnapshot { name, path, content });
    }

    out
}

fn read_steering_files(steering_dir: &Path) -> Vec<FileSnapshot> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(steering_dir) else {
        return out;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".md") {
            continue;
        }
        let path = entry.path();
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let content = String::from_utf8_lossy(&bytes).into_owned();
        out.push(FileSnapshot { name, path, content });
    }

    out
}

/// Load a single document by path for the doc panel.
///
/// - Nonexistent path -> [`DocView::Missing`] (requirement 2.4).
/// - Existing path whose bytes cannot be read (e.g. permission denied) ->
///   [`DocView::ReadError`] with the OS error message (requirement 8.3).
/// - Otherwise, lossy-decoded (requirement 8.4) and rendered ->
///   [`DocView::Rendered`] (requirement 2.6).
///
/// `width` is the doc panel's *outer* width (see [`DOC_PANEL_BORDER`]): the
/// markdown is rendered at the inner width so no content line — tables in
/// particular (요구 5.4) — can overflow the panel's `Borders::ALL` frame.
///
/// Never writes to `path` or any other file (requirement 8.1) — only ever
/// opens files in read mode.
pub fn load_doc(path: &Path, width: u16) -> DocView {
    if !path.exists() {
        return DocView::Missing(path.to_path_buf());
    }

    match fs::read(path) {
        Ok(bytes) => {
            let content = String::from_utf8_lossy(&bytes).into_owned();
            // Requirement 6.12: collect the file info the status bar displays,
            // bundled with the doc so the UI needs no second stat. This runs
            // on every load -- and every file-change reload (requirement 7.1)
            // goes through this same function -- so a changed file refreshes
            // the modified date, size, and line count together. A metadata
            // failure degrades only the timestamp (read already succeeded;
            // showing the doc beats refusing it, per requirement 8.4).
            let modified = fs::metadata(path)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            DocView::Rendered {
                path: path.to_path_buf(),
                r: markdown::render(&content, panel_inner_width(width)),
                meta: FileInfo {
                    modified,
                    size: bytes.len() as u64,
                    lines: content.lines().count(),
                },
            }
        }
        Err(e) => DocView::ReadError {
            path: path.to_path_buf(),
            msg: e.to_string(),
        },
    }
}

/// Human-readable byte size for the status bar's file info (requirement 6.12
/// "크기(사람이 읽는 단위: B/KB/MB, 소수 1자리)"): whole bytes under 1 KiB,
/// one-decimal KiB/MiB above. No space between number and unit, matching the
/// task's `경로 · 12.3KB · 214행` display format.
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes < KB {
        format!("{bytes}B")
    } else if bytes < MB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    }
}

/// `modified` as `YYYY-MM-DD HH:MM` in the LOCAL timezone (requirement 6.12
/// "수정일(YYYY-MM-DD HH:MM, 로컬 시간)"). Uses the same raw-FFI pattern as
/// `main.rs`'s `libc_geteuid_is_root` — the crate declares no `libc`
/// dependency, so `localtime_r` is declared directly. The `struct tm` below
/// is reproduced at its full 56-byte macOS/Linux 64-bit size (9 ints, then
/// `tm_gmtoff`/`tm_zone`) so the C call cannot overflow the buffer. Non-unix
/// targets and any `localtime_r` failure fall back to a pure-std UTC
/// conversion.
pub fn format_modified(modified: SystemTime) -> String {
    let secs = modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, mo, d, h, mi) = local_ymd_hms(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
}

/// Local civil time for `secs` since the epoch: `localtime_r`'s answer on
/// unix, the UTC conversion below otherwise (or on any libc failure).
#[cfg(unix)]
fn local_ymd_hms(secs: i64) -> (i64, u32, u32, u32, u32) {
    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const std::os::raw::c_char,
    }

    extern "C" {
        fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
    }

    let mut tm = Tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_year: 0,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    unsafe {
        let out = localtime_r(&secs, &mut tm);
        if out.is_null() {
            utc_ymd_hms(secs)
        } else {
            (
                (tm.tm_year + 1900) as i64,
                (tm.tm_mon + 1) as u32,
                tm.tm_mday as u32,
                tm.tm_hour as u32,
                tm.tm_min as u32,
            )
        }
    }
}

/// Non-unix targets have no `localtime_r`; render UTC (deterministic and
/// documented) instead.
#[cfg(not(unix))]
fn local_ymd_hms(secs: i64) -> (i64, u32, u32, u32, u32) {
    utc_ymd_hms(secs)
}

/// Pure-std seconds -> `(year, month, day, hour, minute)` UTC conversion
/// (Howard Hinnant's civil-from-days algorithm). Also the `localtime_r`
/// failure fallback and the deterministic cross-check target for the tests.
fn utc_ymd_hms(secs: i64) -> (i64, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32)
}

/// Load a spec's `## 정의` section for the doc panel (requirements 3.6, 3.7).
///
/// - `spec.meta` is `Err` -> [`DocView::MetaError`] using the error's own
///   `Display` message, unmodified (requirement 3.6).
/// - `spec.meta` is `Ok` and `spec.definition` is `Some(text)` -> rendered
///   `text` as [`DocView::Definition`] (requirement 3.7).
/// - `spec.meta` is `Ok` but `spec.definition` is `None` (no `## 정의`
///   section found) -> [`DocView::Definition`] rendering the literal
///   "정의 없음" notice, per design.md's Error Handling section.
///
/// Like [`load_doc`], `width` is the panel's *outer* width (see
/// [`DOC_PANEL_BORDER`]) — content renders at the inner width.
pub fn load_definition(spec: &Spec, width: u16) -> DocView {
    match &spec.meta {
        Err(e) => DocView::MetaError {
            spec: spec.name.clone(),
            msg: e.to_string(),
        },
        Ok(_) => {
            let text = spec.definition.as_deref().unwrap_or("정의 없음");
            DocView::Definition {
                spec: spec.name.clone(),
                text: markdown::render(text, panel_inner_width(width)),
            }
        }
    }
}

/// Top-level dispatcher for a tree-node selection: `kind: None` selects the
/// spec node itself (its definition); `kind: Some(k)` selects one of the
/// spec's document slots.
///
/// If `kind` names a `DocKind` with no matching `DocEntry` in `spec.docs`
/// (should not normally happen, since callers are expected to pass a kind
/// taken from an existing entry), this degrades gracefully to
/// [`DocView::Missing`] at a synthetic path rather than panicking.
pub fn load_for_selection(spec: &Spec, kind: Option<&DocKind>, width: u16) -> DocView {
    match kind {
        None => load_definition(spec, width),
        Some(k) => match spec.docs.iter().find(|d| &d.kind == k) {
            Some(entry) => load_doc(&entry.path, width),
            None => DocView::Missing(spec.dir.join(format!("{k:?}"))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{self, DocStatus};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("spec_viewer_app_loader_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    fn build_fixture_spec(name: &str) -> Spec {
        let snapshot = load_snapshot(&fixtures_root());
        let root = spec::build(&snapshot);
        root.specs
            .into_iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("expected fixture spec {name}"))
    }

    // --- load_doc -----------------------------------------------------

    #[test]
    fn load_doc_on_real_existing_file_renders_non_empty_plain() {
        let path = fixtures_root()
            .join("specs")
            .join("sample-signup")
            .join("requirements.md");

        let view = load_doc(&path, 80);

        match view {
            DocView::Rendered { path: p, r, meta } => {
                assert_eq!(p, path);
                assert!(!r.plain.is_empty());
                assert!(r.plain.iter().any(|l| !l.trim().is_empty()));
                assert!(meta.size > 0, "expected non-zero size, got {}", meta.size);
                assert!(meta.lines > 0, "expected non-zero line count, got {}", meta.lines);
                assert_ne!(meta.modified, SystemTime::UNIX_EPOCH, "expected a real mtime");
            }
            other => panic!("expected Rendered, got {other:?}"),
        }
    }

    #[test]
    fn load_doc_on_nonexistent_path_is_missing() {
        let path = fixtures_root().join("specs").join("does-not-exist.md");

        let view = load_doc(&path, 80);

        match view {
            DocView::Missing(p) => assert_eq!(p, path),
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn load_doc_on_unreadable_file_is_read_error() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch_dir("unreadable");
        let path = dir.join("secret.md");
        fs::write(&path, "hello").unwrap();

        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&path, perms).unwrap();

        let view = load_doc(&path, 80);

        // Restore permissions before any panic/cleanup, and before checking
        // whether the environment actually enforces the revoked mode (e.g.
        // running as root, where reads never fail on permission bits).
        let mut restore = fs::metadata(&path).unwrap().permissions();
        restore.set_mode(0o644);
        fs::set_permissions(&path, restore).unwrap();

        if fs::read(&path).is_ok() && matches!(view, DocView::Rendered { .. }) {
            // Best-effort: some sandboxes (root, certain CI containers) do
            // not enforce the 0o000 mode, so the read succeeded anyway.
            // Nothing to assert in that case.
            let _ = fs::remove_dir_all(&dir);
            return;
        }

        match view {
            DocView::ReadError { path: p, msg } => {
                assert_eq!(p, path);
                assert!(!msg.is_empty());
            }
            other => panic!("expected ReadError, got {other:?}"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    // --- load_definition ------------------------------------------------

    #[test]
    fn load_definition_renders_definition_text() {
        let spec = build_fixture_spec("sample-signup");

        let view = load_definition(&spec, 80);

        match view {
            DocView::Definition { spec: name, text } => {
                assert_eq!(name, "sample-signup");
                let joined = text.plain.join("\n");
                assert!(joined.contains("회원가입 절차를 검증하는 요구사항 스펙이다"));
            }
            other => panic!("expected Definition, got {other:?}"),
        }
    }

    #[test]
    fn load_definition_with_no_definition_section_shows_notice() {
        let spec = build_fixture_spec("sample-billing");
        assert_eq!(spec.definition, None);

        let view = load_definition(&spec, 80);

        match view {
            DocView::Definition { spec: name, text } => {
                assert_eq!(name, "sample-billing");
                let joined = text.plain.join("\n");
                assert!(joined.contains("정의 없음"));
            }
            other => panic!("expected Definition, got {other:?}"),
        }
    }

    #[test]
    fn load_definition_on_broken_meta_is_meta_error() {
        let spec = build_fixture_spec("broken-json");
        assert!(spec.meta.is_err());

        let view = load_definition(&spec, 80);

        match view {
            DocView::MetaError { spec: name, msg } => {
                assert_eq!(name, "broken-json");
                assert!(!msg.is_empty());
            }
            other => panic!("expected MetaError, got {other:?}"),
        }
    }

    // --- load_for_selection --------------------------------------------

    #[test]
    fn load_for_selection_none_dispatches_to_definition() {
        let spec = build_fixture_spec("sample-signup");

        let view = load_for_selection(&spec, None, 80);

        assert!(matches!(view, DocView::Definition { .. }));
    }

    #[test]
    fn load_for_selection_some_kind_dispatches_to_doc() {
        let spec = build_fixture_spec("sample-signup");

        let view = load_for_selection(&spec, Some(&DocKind::Requirements), 80);

        match view {
            DocView::Rendered { path, .. } => {
                let entry = spec
                    .docs
                    .iter()
                    .find(|d| d.kind == DocKind::Requirements)
                    .unwrap();
                assert_eq!(path, entry.path);
                assert_eq!(entry.status, DocStatus::Approved);
            }
            other => panic!("expected Rendered, got {other:?}"),
        }
    }

    // --- load_snapshot ----------------------------------------------------

    #[test]
    fn load_snapshot_finds_known_spec_and_steering_names() {
        let snapshot = load_snapshot(&fixtures_root());

        let mut spec_names: Vec<&str> = snapshot.specs.iter().map(|s| s.name.as_str()).collect();
        spec_names.sort();
        assert_eq!(
            spec_names,
            vec![
                "broken-json",
                "bugfix-only",
                "no-approvals",
                "no-checkboxes",
                "non-utf8",
                "sample-billing",
                "sample-signup",
                "with-extra",
            ]
        );

        let mut steering_names: Vec<&str> =
            snapshot.steering.iter().map(|s| s.name.as_str()).collect();
        steering_names.sort();
        assert_eq!(
            steering_names,
            vec!["domain-terms.md", "product.md", "tech.md"]
        );
    }

    #[test]
    fn load_snapshot_round_trips_through_build_without_panicking() {
        let snapshot = load_snapshot(&fixtures_root());
        let expected_spec_count = snapshot.specs.len();

        let root = spec::build(&snapshot);

        assert_eq!(root.specs.len(), expected_spec_count);
        assert_eq!(root.steering.len(), 3);
    }

    #[test]
    fn load_snapshot_lossy_decodes_non_utf8_fixture_without_panicking() {
        let snapshot = load_snapshot(&fixtures_root());

        let non_utf8 = snapshot
            .specs
            .iter()
            .find(|s| s.name == "non-utf8")
            .expect("non-utf8 fixture spec present");
        let requirements = non_utf8
            .files
            .iter()
            .find(|f| f.name == "requirements.md")
            .expect("requirements.md present in non-utf8 fixture");

        // The fixture's raw bytes are not valid UTF-8; lossy decoding must
        // replace the offending bytes with U+FFFD rather than erroring.
        assert!(requirements.content.contains('\u{FFFD}'));
    }

    #[test]
    fn load_snapshot_missing_specs_or_steering_dir_is_not_an_error() {
        let dir = scratch_dir("missing_subdirs");
        // Neither `specs/` nor `steering/` exists under `dir`.

        let snapshot = load_snapshot(&dir);

        assert!(snapshot.specs.is_empty());
        assert!(snapshot.steering.is_empty());

        fs::remove_dir_all(&dir).unwrap();
    }

    // --- file-info formatting (task 19.3) ------------------------------

    #[test]
    fn format_size_human_readable_boundaries() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(1023), "1023B");
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(12_595), "12.3KB");
        assert_eq!(format_size(1_572_864), "1.5MB");
    }

    #[test]
    fn utc_ymd_hms_matches_known_instants() {
        assert_eq!(utc_ymd_hms(0), (1970, 1, 1, 0, 0));
        // 2026-09-15T18:48:00Z, picked against `TZ=UTC date -r 1789498080`.
        assert_eq!(utc_ymd_hms(1_789_498_080), (2026, 9, 15, 18, 48));
    }

    #[test]
    fn utc_ymd_hms_agrees_with_system_date() {
        // Cross-check the pure-std civil-time conversion against the OS's
        // `date` (macOS `-r <secs>`, GNU/Linux `-d @<secs>`, both under
        // `TZ=UTC`). Skips silently if date(1) is missing, e.g. in a CI sandbox.
        let secs = 1_789_498_080i64;
        let out = std::process::Command::new("date")
            .env("TZ", "UTC")
            .arg(if cfg!(target_os = "macos") {
                "-r"
            } else {
                "-d"
            })
            .arg(if cfg!(target_os = "macos") {
                secs.to_string()
            } else {
                format!("@{secs}")
            })
            .arg("+%Y-%m-%d %H:%M")
            .output()
            .ok()
            .and_then(|o| (o.status.success()).then(|| o.stdout))
            .and_then(|out| String::from_utf8(out).ok())
            .map(|s| s.trim().to_string());
        let Some(expected) = out else {
            return;
        };
        assert_eq!(
            expected.len(),
            16,
            "unexpected date(1) output: {expected:?}"
        );
        let (y, mo, d, h, mi) = utc_ymd_hms(secs);
        let actual = format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}");
        assert_eq!(actual, expected, "utc_ymd_hms diverged from system date");
    }

    #[test]
    fn format_modified_is_ymd_hms_shape() {
        // TZ-independent: whatever the local zone is, the string must be
        // exactly `YYYY-MM-DD HH:MM`. (Exact local-time contents are verified
        // in the tmux smoke check against `ls -l`'s mtime.)
        let s = format_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_789_498_080));
        assert_eq!(s.len(), 16, "got {s:?}");
        assert_eq!(&s[4..5], "-", "got {s:?}");
        assert_eq!(&s[7..8], "-", "got {s:?}");
        assert_eq!(&s[10..11], " ", "got {s:?}");
        assert_eq!(&s[13..14], ":", "got {s:?}");
        assert!(s.as_bytes().iter().enumerate().all(|(i, b)| {
            [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15].contains(&i)
                == b.is_ascii_digit()
        }));
    }

    #[test]
    fn load_doc_meta_reflects_file_properties() {
        let dir = scratch_dir("meta_roundtrip");
        let path = dir.join("notes.md");
        let content = "line one\n\nline two\nline three";
        fs::write(&path, content).unwrap();
        // Pin the mtime to a known whole-second so the assertion is exact.
        let times = std::fs::FileTimes::new()
            .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000));
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(times)
            .expect("set scratch mtime");

        let view = load_doc(&path, 80);

        match view {
            DocView::Rendered { meta, .. } => {
                assert_eq!(meta.size, content.len() as u64);
                assert_eq!(meta.lines, 4, "blank line must count as a line");
                let secs = meta
                    .modified
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("mtime after epoch")
                    .as_secs();
                assert_eq!(secs, 1_700_000_000, "mtime must come from the file");
            }
            other => panic!("expected Rendered, got {other:?}"),
        }

        fs::remove_dir_all(&dir).unwrap();
    }
}
