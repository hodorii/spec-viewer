//! Status bar: one line composed of path · file info (modified date · size ·
//! line count, requirement 6.12) · scroll% · active search · watch status
//! (design.md "ui — Panels": `status_bar: 경로 · NN% · /검색어 (i/n) · watch
//! off`; requirements 6.10, 6.12, 6.7 (incidental), 7.6). On narrow status
//! bars (< 80 columns) the file info is omitted first, keeping path · % —
//! the task's explicit `경로 · 2026-09-15 18:48 · 12.3KB · 214행 · 37%` order
//! (file info before the percent) overrides design.md's `경로 · NN% · 파일정보`.

use crate::app::{format_modified, format_size, DocView, SearchState, WatchStatus};
use crate::markdown::Rendered;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Extract the `Rendered` payload carried by a `DocView`, if any (same
/// local-match approach as `doc_panel::rendered_of` — `app::rendered_of` is
/// private to `app`).
fn rendered_of(doc: &DocView) -> Option<&Rendered> {
    match doc {
        DocView::Rendered { r, .. } => Some(r),
        DocView::Definition { text, .. } => Some(text),
        _ => None,
    }
}

fn path_segment(doc: &DocView) -> String {
    match doc {
        DocView::Empty => "(선택 없음)".to_string(),
        DocView::Rendered { path, .. } => path.display().to_string(),
        DocView::Definition { spec, .. } => format!("{spec} (정의)"),
        DocView::Missing(p) | DocView::Deleted(p) => p.display().to_string(),
        DocView::ReadError { path, .. } => path.display().to_string(),
        DocView::MetaError { spec, .. } => spec.clone(),
    }
}

/// File info for a rendered doc (requirement 6.12): `수정일 · 크기 · 행수` in
/// one segment. `None` for every non-file view — design.md attaches
/// `FileInfo` to `DocView::Rendered` only (a `Definition` is a synthesized
/// spec view, not a file).
fn file_info_segment(doc: &DocView) -> Option<String> {
    match doc {
        DocView::Rendered { meta, .. } => Some(format!(
            "{} · {} · {}행",
            format_modified(meta.modified),
            format_size(meta.size),
            meta.lines
        )),
        _ => None,
    }
}

/// Scroll position as a percentage. A doc with 0 or 1 renderable lines is
/// trivially "100%" (nothing left to scroll to) rather than "0%" — chosen
/// so a single-screen doc reads as fully in view, not as if stuck at the
/// top with more below. Guards divide-by-zero via `saturating_sub`/`max`.
fn percent_segment(doc: &DocView, scroll: usize) -> String {
    match rendered_of(doc) {
        Some(r) if r.lines.len() > 1 => {
            let total = r.lines.len();
            let clamped = scroll.min(total - 1);
            let pct = (clamped * 100) / total.saturating_sub(1).max(1);
            format!("{}%", pct.min(100))
        }
        Some(_) => "100%".to_string(),
        None => "0%".to_string(),
    }
}

/// `None` when there is nothing to show (empty query) so the caller can
/// omit the segment entirely rather than emitting stray separators.
fn search_segment(search: &SearchState) -> Option<String> {
    if search.query.is_empty() {
        return None;
    }
    if search.matches.is_empty() {
        return Some("일치 없음".to_string());
    }
    let i = search.current.map(|c| c + 1).unwrap_or(0);
    let n = search.matches.len();
    Some(format!("/{} ({i}/{n})", search.query))
}

/// `None` when live (nothing worth showing); `Some("watch off")` verbatim
/// (design.md's own untranslated string) when manual.
fn watch_segment(watch: &WatchStatus) -> Option<String> {
    match watch {
        WatchStatus::Live => None,
        WatchStatus::Manual { .. } => Some("watch off".to_string()),
    }
}

/// Render the status bar into `area` (row 0 of whatever `area` is given —
/// callers typically pass a single-row `Rect`, but this does not assume
/// `area.height == 1`).
pub fn render(
    frame: &mut Frame,
    area: Rect,
    doc: &DocView,
    scroll: usize,
    search: &SearchState,
    watch: &WatchStatus,
) {
    let mut segments = vec![path_segment(doc)];
    // Narrow status bars (< 80 columns) drop the file info first, keeping
    // path · % (the % below is always pushed after this gate).
    if area.width >= crate::app::NARROW_WIDTH_THRESHOLD {
        if let Some(s) = file_info_segment(doc) {
            segments.push(s);
        }
    }
    segments.push(percent_segment(doc, scroll));
    if let Some(s) = search_segment(search) {
        segments.push(s);
    }
    if let Some(s) = watch_segment(watch) {
        segments.push(s);
    }
    let line = segments.join(" · ");

    let para = Paragraph::new(line);
    frame.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{self, DocView, FileInfo, SearchState, WatchStatus};
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;
    use std::path::PathBuf;
    use unicode_width::UnicodeWidthStr;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
    }

    fn requirements_doc_path() -> PathBuf {
        fixtures_root().join("specs/sample-signup/requirements.md")
    }

    // See the matching comment in `doc_panel`'s test module: wide glyphs'
    // hidden continuation cells also read as a plain space, so columns are
    // walked by each symbol's display width rather than one at a time.
    fn joined(buffer: &Buffer) -> String {
        let area = buffer.area();
        (0..area.height)
            .map(|y| {
                let mut row = String::new();
                let mut x = 0u16;
                while x < area.width {
                    let symbol = buffer.get(x, y).symbol();
                    row.push_str(symbol);
                    let w = UnicodeWidthStr::width(symbol).max(1) as u16;
                    x += w;
                }
                row
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn draw(
        doc: &DocView,
        scroll: usize,
        search: &SearchState,
        watch: &WatchStatus,
        w: u16,
    ) -> Buffer {
        let backend = TestBackend::new(w, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, doc, scroll, search, watch);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn path_segment_shows_rendered_path() {
        let doc = app::load_doc(&requirements_doc_path(), 60);
        let search = SearchState::default();
        let watch = WatchStatus::Live;
        let buf = draw(&doc, 0, &search, &watch, 200);
        let text = joined(&buf);
        assert!(
            text.contains("sample-signup/requirements.md")
                || text.contains(&requirements_doc_path().display().to_string()),
            "expected path in: {text}"
        );
    }

    #[test]
    fn percent_segment_reflects_scroll() {
        let doc = app::load_doc(&requirements_doc_path(), 60);
        let total = match &doc {
            DocView::Rendered { r, .. } => r.lines.len(),
            _ => panic!("expected Rendered"),
        };
        assert!(total > 1, "fixture doc too short for this test");
        let scroll = total / 2;
        let expected_pct = (scroll.min(total - 1) * 100) / (total - 1).max(1);
        let search = SearchState::default();
        let watch = WatchStatus::Live;
        let buf = draw(&doc, scroll, &search, &watch, 200);
        let text = joined(&buf);
        assert!(
            text.contains(&format!("{expected_pct}%")),
            "expected {expected_pct}% in: {text}"
        );
    }

    #[test]
    fn empty_query_has_no_stray_search_segment() {
        let doc = DocView::Empty;
        let search = SearchState::default();
        let watch = WatchStatus::Live;
        let buf = draw(&doc, 0, &search, &watch, 200);
        let text = joined(&buf);
        assert!(!text.contains("()"), "unexpected empty-count artifact in: {text}");
        assert!(!text.contains("/ ("), "unexpected stray search segment in: {text}");
    }

    #[test]
    fn nonempty_query_with_matches_shows_index_and_count() {
        let doc = DocView::Empty;
        let search = SearchState {
            query: "needle".to_string(),
            matches: vec![0, 3, 7],
            current: Some(1),
        };
        let watch = WatchStatus::Live;
        let buf = draw(&doc, 0, &search, &watch, 200);
        let text = joined(&buf);
        assert!(
            text.contains("/needle (2/3)"),
            "expected search segment in: {text}"
        );
    }

    #[test]
    fn nonempty_query_zero_matches_shows_no_match_literal() {
        let doc = DocView::Empty;
        let search = SearchState {
            query: "nope".to_string(),
            matches: vec![],
            current: None,
        };
        let watch = WatchStatus::Live;
        let buf = draw(&doc, 0, &search, &watch, 200);
        let text = joined(&buf);
        assert!(text.contains("일치 없음"), "expected 일치 없음 in: {text}");
    }

    #[test]
    fn manual_watch_shows_watch_off_live_does_not() {
        let doc = DocView::Empty;
        let search = SearchState::default();

        let manual = WatchStatus::Manual {
            reason: "폴링 불가".to_string(),
        };
        let buf_manual = draw(&doc, 0, &search, &manual, 200);
        assert!(joined(&buf_manual).contains("watch off"));

        let live = WatchStatus::Live;
        let buf_live = draw(&doc, 0, &search, &live, 200);
        assert!(!joined(&buf_live).contains("watch off"));
    }

    /// "YYYY-MM-DD HH:MM" shape check (requirement 6.12's modified-date
    /// format). The value itself is timezone-dependent, so tests assert the
    /// shape rather than an exact wall-clock string.
    fn is_date_segment(s: &str) -> bool {
        let b = s.as_bytes();
        b.len() == 16
            && b[4] == b'-'
            && b[7] == b'-'
            && b[10] == b' '
            && b[13] == b':'
            && b
                .iter()
                .enumerate()
                .all(|(i, &c)| (i == 4 || i == 7 || i == 10 || i == 13) || c.is_ascii_digit())
    }

    /// "512B" / "12.3KB" / "1.5MB" shape check (requirement 6.12's size
    /// format: digits with at most one '.', ended by 'B').
    fn is_size_segment(s: &str) -> bool {
        let Some(rest) = s.strip_suffix('B') else {
            return false;
        };
        let (num, frac) = rest
            .rsplit_once('.')
            .map_or((rest, None), |(a, b)| (a, Some(b)));
        !num.is_empty()
            && num.chars().all(|c| c.is_ascii_digit())
            && frac.is_none_or(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()))
    }

    /// "214행" shape check (requirement 6.12's line-count format).
    fn is_line_count_segment(s: &str) -> bool {
        let Some(digits) = s.strip_suffix('행') else {
            return false;
        };
        !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
    }

    /// A rendered doc with a known `FileInfo`, for the width-gating frame
    /// tests: a short path so every segment fits the 120-column frame the
    /// requirement targets (the real fixture's absolute path alone is ~88
    /// columns). Known values make size/line-count assertions exact; the
    /// modified date is asserted by shape via [`is_date_segment`].
    fn file_info_doc() -> DocView {
        DocView::Rendered {
            path: PathBuf::from("requirements.md"),
            r: crate::markdown::render("head\n\nbody\n\ntail\n", 60),
            meta: FileInfo {
                modified: std::time::SystemTime::UNIX_EPOCH
                    + std::time::Duration::from_secs(1_789_498_080),
                size: 12_595,
                lines: 214,
            },
        }
    }

    #[test]
    fn file_info_shown_on_wide_status_bar() {
        let doc = file_info_doc();
        let search = SearchState::default();
        let watch = WatchStatus::Live;
        let buf = draw(&doc, 0, &search, &watch, 120);
        let text = joined(&buf);
        let segments: Vec<&str> = text.trim_end().split(" · ").collect();
        assert!(segments.len() >= 5, "expected ≥ 5 segments in: {text}");
        assert_eq!(segments[0], "requirements.md");
        assert!(is_date_segment(segments[1]), "date segment in: {segments:?}");
        assert_eq!(segments[2], "12.3KB");
        assert_eq!(segments[3], "214행");
        assert_eq!(segments[segments.len() - 1], "0%");
    }

    #[test]
    fn file_info_omitted_narrow_status_bar_keeps_path_and_percent() {
        let doc = file_info_doc();
        let search = SearchState::default();
        let watch = WatchStatus::Live;
        let buf = draw(&doc, 0, &search, &watch, 60);
        let text = joined(&buf);
        assert_eq!(text.trim_end(), "requirements.md · 0%");
        assert!(!text.split(" · ").any(is_date_segment), "no date in: {text}");
        assert!(!text.split(" · ").any(is_size_segment), "no size in: {text}");
        assert!(
            !text.split(" · ").any(is_line_count_segment),
            "no line count in: {text}"
        );
    }
}
