//! Single source of truth for key -> reducer action mapping, and for the
//! `?` help popup listing (design.md: "키맵은 keymap.rs의 표 하나에서 동작과
//! 도움말을 함께 생성"). `app::update` looks up an action name from
//! [`action_for_key`] rather than hand-matching keys itself, so the reducer
//! and the help popup can never silently drift apart.
//!
//! Key choices (requirements.md dictates only `[`/`]` for heading-jump and
//! `?` for help at the letter level; everything else below is this task's
//! own sensible pager/vi-like convention, deliberately chosen so document
//! scrolling and tree navigation never share a physical key):
//! - Quit: `q` (requirement 1.5)
//! - Switch panel: `Tab` (requirement 2.7)
//! - Line down/up: `j` / `k`
//! - Half-page down/up: `d` / `u`
//! - Full-page down/up: `PageDown`/`f`, `PageUp`/`b`
//! - Top/bottom: `g`/`Home`, `G`/`End`
//! - Prev/next heading: `[` / `]` (requirement 6.2, explicit)
//! - Open TOC popup: `t` (requirement 6.3, explicit per task 7.2's DONE text)
//! - Open search-input popup: `/` (requirement 6.5, explicit per task 7.2's
//!   DONE text)
//! - Confirm the current tree selection into the doc panel: `Enter`
//!   (exercises requirement 2.6 without needing real tree-row navigation,
//!   which needs a `Tree` render pass that does not exist until task 6.x)
//! - Help: `?` (requirement 6.11, explicit)
//! - Toggle tree panel visibility: `T` (requirement 1.7; ignored under
//!   `--tree always`, where the tree is pinned on)
//! - Layout mode (requirement 1.10, explicit "네 모드를 핫키로 전환"):
//!   `1` auto, `2` fold (tree hidden, doc only), `3` expand (tree+doc
//!   pinned), `4` single (one panel at a time, switched by selecting a doc
//!   / `Esc`). Digits, since every letter with an obvious mnemonic (a/f/e)
//!   was already spoken for by an existing binding.
//! - Cycle spec-tree sort key (requirement 2.9): `s` (이름 -> phase -> 최근
//!   갱신 -> 진행률 -> 이름).
//! - Dismiss popup / clear an active tree search / clear a doc-panel text
//!   selection: `Esc` — handled directly in `app::update` (popup-open
//!   branch, then the tree-search-clear check, then the selection check,
//!   all in `handle_key`; requirements 2.10, 9.9), not via this table,
//!   since it is a global escape hatch rather than a normal action
//! - Tree navigation (forward-compatible only; `TreeState::key_*` need a
//!   `Tree` widget render pass to have any visible effect — task 6.x):
//!   arrow keys `Up` / `Down` / `Left` / `Right`.

use crossterm::event::{KeyCode, KeyEvent};

/// One key binding: the physical keys that trigger it, the reducer action
/// name it dispatches to, and its help-popup description.
pub struct Binding {
    pub keys: &'static [KeyCode],
    pub action: &'static str,
    pub help: &'static str,
}

pub const BINDINGS: &[Binding] = &[
    Binding {
        keys: &[KeyCode::Char('q')],
        action: "quit",
        help: "종료",
    },
    Binding {
        keys: &[KeyCode::Tab],
        action: "switch_panel",
        help: "패널 전환 (트리 <-> 문서)",
    },
    Binding {
        keys: &[KeyCode::Char('j')],
        action: "line_down",
        help: "한 줄 아래로 스크롤",
    },
    Binding {
        keys: &[KeyCode::Char('k')],
        action: "line_up",
        help: "한 줄 위로 스크롤",
    },
    Binding {
        keys: &[KeyCode::Char('d')],
        action: "half_page_down",
        help: "반 화면 아래로 스크롤",
    },
    Binding {
        keys: &[KeyCode::Char('u')],
        action: "half_page_up",
        help: "반 화면 위로 스크롤",
    },
    Binding {
        keys: &[KeyCode::PageDown, KeyCode::Char('f')],
        action: "page_down",
        help: "한 화면 아래로 스크롤",
    },
    Binding {
        keys: &[KeyCode::PageUp, KeyCode::Char('b')],
        action: "page_up",
        help: "한 화면 위로 스크롤",
    },
    Binding {
        keys: &[KeyCode::Char('g'), KeyCode::Home],
        action: "top",
        help: "문서 처음으로 이동",
    },
    Binding {
        keys: &[KeyCode::Char('G'), KeyCode::End],
        action: "bottom",
        help: "문서 끝으로 이동",
    },
    Binding {
        keys: &[KeyCode::Char('[')],
        action: "prev_heading",
        help: "이전 헤딩으로 이동",
    },
    Binding {
        keys: &[KeyCode::Char(']')],
        action: "next_heading",
        help: "다음 헤딩으로 이동",
    },
    Binding {
        keys: &[KeyCode::Enter],
        action: "select",
        help: "선택한 트리 노드를 문서 패널에 로드",
    },
    Binding {
        keys: &[KeyCode::Char('?')],
        action: "help",
        help: "도움말 표시",
    },
    Binding {
        keys: &[KeyCode::Char('n')],
        action: "next_match",
        help: "다음 검색 결과로 이동 (포커스한 패널의 검색)",
    },
    Binding {
        keys: &[KeyCode::Char('N')],
        action: "prev_match",
        help: "이전 검색 결과로 이동 (포커스한 패널의 검색)",
    },
    Binding {
        keys: &[KeyCode::Up],
        action: "nav_up",
        help: "트리: 위 노드로 이동 / 문서: 한 줄 위로 스크롤 (활성 패널 기준)",
    },
    Binding {
        keys: &[KeyCode::Down],
        action: "nav_down",
        help: "트리: 아래 노드로 이동 / 문서: 한 줄 아래로 스크롤 (활성 패널 기준)",
    },
    Binding {
        keys: &[KeyCode::Left],
        action: "nav_left",
        help: "트리 노드 접기 / 상위 노드로 이동",
    },
    Binding {
        keys: &[KeyCode::Right],
        action: "nav_right",
        help: "트리 노드 펼치기",
    },
    Binding {
        keys: &[KeyCode::Char('r')],
        action: "refresh",
        help: "수동으로 새로고침 (감시 불가 시 사용)",
    },
    Binding {
        keys: &[KeyCode::Char('t')],
        action: "open_toc",
        help: "목차(TOC) 열기",
    },
    Binding {
        keys: &[KeyCode::Char('/')],
        action: "open_search",
        help: "검색 (포커스한 패널 기준: 문서 내용 또는 트리 노드 이름)",
    },
    Binding {
        keys: &[KeyCode::Char('T')],
        action: "toggle_tree",
        help: "트리 패널 표시/숨김 전환 (--tree always 에서는 무시됨)",
    },
    Binding {
        keys: &[KeyCode::Char('1')],
        action: "layout_auto",
        help: "레이아웃: 자동 (폭에 따라 2패널/단일)",
    },
    Binding {
        keys: &[KeyCode::Char('2')],
        action: "layout_fold",
        help: "레이아웃: 좌측 접기 (문서만)",
    },
    Binding {
        keys: &[KeyCode::Char('3')],
        action: "layout_expand",
        help: "레이아웃: 좌측 펼치기 (트리+문서 고정)",
    },
    Binding {
        keys: &[KeyCode::Char('4')],
        action: "layout_single",
        help: "레이아웃: 단일 모드 (문서 선택 시 문서 전체 폭, Esc 로 트리 복귀)",
    },
    Binding {
        keys: &[KeyCode::Char('s')],
        action: "cycle_sort",
        help: "스펙 트리 정렬 키 순환 (이름 -> phase -> 최근 갱신 -> 진행률)",
    },
];

/// Resolve a key event to a [`BINDINGS`] action name, ignoring modifiers —
/// a stray Shift/Ctrl reported alongside an otherwise-bound key (terminal
/// modifier reporting is inconsistent across platforms) still matches.
pub fn action_for_key(key: KeyEvent) -> Option<&'static str> {
    BINDINGS
        .iter()
        .find(|b| b.keys.contains(&key.code))
        .map(|b| b.action)
}

/// Render [`BINDINGS`] as a `(key label, help text)` listing, one row per
/// binding, for `Popup::Help` (requirement 6.11).
pub fn help_entries() -> Vec<(String, String)> {
    BINDINGS
        .iter()
        .map(|b| (key_label(b), b.help.to_string()))
        .collect()
}

fn key_label(b: &Binding) -> String {
    b.keys
        .iter()
        .map(|k| key_code_label(*k))
        .collect::<Vec<_>>()
        .join("/")
}

fn key_code_label(k: KeyCode) -> String {
    match k {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[test]
    fn quit_key_resolves_to_quit_action() {
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(action_for_key(key), Some("quit"));
    }

    #[test]
    fn unrecognized_key_resolves_to_none() {
        let key = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
        assert_eq!(action_for_key(key), None);
    }

    #[test]
    fn help_entries_cover_quit_switch_panel_and_heading_jump() {
        let entries = help_entries();
        assert!(entries.iter().any(|(k, h)| k == "q" && h.contains("종료")));
        assert!(entries.iter().any(|(_, h)| h.contains("전환")));
        assert!(entries.iter().any(|(k, _)| k == "["));
        assert!(entries.iter().any(|(k, _)| k == "]"));
    }
}
