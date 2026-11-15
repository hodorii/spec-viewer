//! Seeded property checks for the structured task list view and its no-regression
//! guarantees. No property-testing dependency: a small xorshift generator makes a
//! seed alone reproduce a case, and a failing seed is printed before the panic
//! propagates.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use spec_viewer::app::navigator::locate_line;
use spec_viewer::app::{self, Action, AppState, DocView, Panel, Popup, TreeMode, WatchStatus};
use spec_viewer::markdown::Rendered;
use spec_viewer::spec::methodology::{LocateBy, MethodologySettings};
use spec_viewer::spec::progress::count_progress;
use spec_viewer::spec::tasks::{parse_tasks, TasksBlock};
use spec_viewer::spec::{self, spec_kit, DocKind, FsTree, NodeId, TreeSource};
use std::fs;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use support::{buffer_rows, find_text_cell, render_app_to_buffer};

const SEEDS_PER_PROPERTY: u64 = 200;
const SLOW_SEEDS_PER_PROPERTY: u64 = 40;

/// xorshift64*: deterministic and tiny, so a seed alone reproduces a case.
struct SeededRng(u64);

impl SeededRng {
    fn new(seed: u64) -> Self {
        // A zero state would stay zero forever.
        SeededRng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }

    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

fn for_each_seed(property: &str, seed_count: u64, check: impl Fn(u64)) {
    for seed in 0..seed_count {
        if let Err(failure) = catch_unwind(AssertUnwindSafe(|| check(seed))) {
            eprintln!("property {property} failed; reproduce with seed {seed}");
            resume_unwind(failure);
        }
    }
}

fn key(code: KeyCode) -> Action {
    Action::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn fixtures(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn kiro_state(root: &Path, size: (u16, u16)) -> AppState {
    let tree = spec::build(&app::load_snapshot(root));
    AppState::new(TreeSource::Kiro(tree), root.to_path_buf(), size, WatchStatus::Live, TreeMode::Auto, true)
}

// --- P1: the parser never panics and finds as many tasks as the progress counter ---

const LINE_FRAGMENTS: [&str; 22] = [
    "- [x] 1. 완료",
    "- [ ] 2.1 (P) 병렬",
    "- [ ]* 3.1 선택",
    "[X] 4 번호만",
    "  - [ ] 들여쓴 체크박스",
    "-[x]붙은",
    "- [ ]",
    "[ ]*",
    "  - _Requirements: 1.1, 1.2_",
    "  - _Depends: 2.1~2.3_",
    "  - DONE: 상태 메모",
    "  - 일반 메모: 콜론 포함",
    "    - _Boundary: a::b_",
    "## 제목",
    "####### 일곱 단계",
    "#해시뿐",
    "본문 줄",
    "",
    "   ",
    "- [y] 잘못된 상자",
    "- [x]\t탭 뒤",
    "_: 빈 이름_",
];

fn random_markdown(rng: &mut SeededRng) -> String {
    let line_count = rng.below(30);
    (0..line_count)
        .map(|_| rng.pick(&LINE_FRAGMENTS).to_string())
        .collect::<Vec<_>>()
        .join(if rng.chance(5) { "\r\n" } else { "\n" })
}

#[test]
fn p1_parser_never_panics_and_task_count_matches_the_progress_total() {
    let settings = MethodologySettings::kiro_default();
    for_each_seed("P1", SEEDS_PER_PROPERTY, |seed| {
        let source = random_markdown(&mut SeededRng::new(seed));
        let document = parse_tasks(&source, &settings);
        let task_count = document.blocks.iter().filter(|block| matches!(block, TasksBlock::Task(_))).count() as u32;
        let expected = count_progress(&source).map_or(0, |progress| progress.total);
        assert_eq!(task_count, expected, "input: {source:?}");
    });
}

// --- P2: a number never matches a longer number ------------------------------------

const NUMBERS: [&str; 7] = ["1", "2", "12", "1.1", "1.12", "2.1", "10.2"];
const DECORATIONS: [&str; 6] = ["", "- ", "### ", "- [x] ", "  * ", "> "];
const SUFFIXES: [&str; 8] = ["", " 제목", ". 제목", ".", ": 본문", " 3", "5", ".7 뒤"];

struct NumberedLine {
    decoration: &'static str,
    number: &'static str,
    suffix: &'static str,
}

impl NumberedLine {
    fn text(&self) -> String {
        format!("{}{}{}", self.decoration, self.number, self.suffix)
    }

    /// Independent oracle: the line is about this number only when the number
    /// is not the start of a longer one.
    fn is_exactly(&self, number: &str) -> bool {
        let continues_number = self.suffix.starts_with(|c: char| c.is_ascii_digit())
            || self.suffix.strip_prefix('.').is_some_and(|after| after.starts_with(|c: char| c.is_ascii_digit()));
        self.number == number && !continues_number
    }
}

fn rendered_from(lines: &[String]) -> Rendered {
    Rendered { lines: Vec::new(), plain: lines.to_vec(), headings: Vec::new(), footnotes: Vec::new() }
}

#[test]
fn p2_number_lookup_never_lands_on_a_longer_number() {
    for_each_seed("P2", SEEDS_PER_PROPERTY, |seed| {
        let mut rng = SeededRng::new(seed);
        let lines: Vec<NumberedLine> = (0..rng.below(14) + 1)
            .map(|_| NumberedLine {
                decoration: rng.pick(&DECORATIONS),
                number: rng.pick(&NUMBERS),
                suffix: rng.pick(&SUFFIXES),
            })
            .collect();
        let wanted = *rng.pick(&NUMBERS);
        let texts: Vec<String> = lines.iter().map(NumberedLine::text).collect();
        let expected = lines.iter().position(|line| line.is_exactly(wanted));
        let found = locate_line(&rendered_from(&texts), LocateBy::LeadingNumber, wanted).ok();
        assert_eq!(found, expected, "number {wanted:?} in lines {texts:?}");
    });
}

// --- P3: documents the structured view cannot apply to look exactly as before -------

#[derive(Clone, Copy, Debug)]
enum SourceShape {
    KiroSamples,
    KiroTasksView,
    FilesTasksView,
    FilesKiroSamples,
    SpecKit,
}

const SOURCE_SHAPES: [SourceShape; 5] = [
    SourceShape::KiroSamples,
    SourceShape::KiroTasksView,
    SourceShape::FilesTasksView,
    SourceShape::FilesKiroSamples,
    SourceShape::SpecKit,
];

fn state_for(shape: SourceShape, is_structured_view_enabled: bool, size: (u16, u16)) -> AppState {
    let mut state = match shape {
        SourceShape::KiroSamples => kiro_state(&fixtures("kiro"), size),
        SourceShape::KiroTasksView => kiro_state(&fixtures("kiro-tasks-view"), size),
        SourceShape::FilesTasksView => files_state(&fixtures("kiro-tasks-view"), size),
        SourceShape::FilesKiroSamples => files_state(&fixtures("kiro"), size),
        SourceShape::SpecKit => {
            let root = fixtures("spec-kit");
            AppState::new(
                TreeSource::SpecKit(spec_kit::build(&root.join("specs"))),
                root,
                size,
                WatchStatus::Live,
                TreeMode::Auto,
                true,
            )
        }
    };
    state.is_tasks_view_enabled = is_structured_view_enabled;
    state
}

fn files_state(root: &Path, size: (u16, u16)) -> AppState {
    AppState::new(
        TreeSource::Files(FsTree::scan(root)),
        root.to_path_buf(),
        size,
        WatchStatus::Live,
        TreeMode::Auto,
        true,
    )
}

fn is_kiro_task_list_open(state: &AppState) -> bool {
    matches!(state.root, TreeSource::Kiro(_))
        && matches!(&state.doc, DocView::Rendered { path, .. } if path.file_name().is_some_and(|name| name == "tasks.md"))
}

const WALK_KEYS: [KeyCode; 7] =
    [KeyCode::Down, KeyCode::Up, KeyCode::Right, KeyCode::Left, KeyCode::Enter, KeyCode::Tab, KeyCode::Char('j')];

#[test]
fn p3_documents_without_a_structured_view_render_the_same_with_the_view_enabled() {
    for_each_seed("P3", SLOW_SEEDS_PER_PROPERTY, |seed| {
        let mut rng = SeededRng::new(seed);
        let shape = *rng.pick(&SOURCE_SHAPES);
        let size = (60 + rng.below(80) as u16, 20 + rng.below(20) as u16);
        let mut with_view = state_for(shape, true, size);
        let mut without_view = state_for(shape, false, size);
        // Unfolding everything first lets short walks reach deep documents such as a files-tree tasks.md.
        let mut keys: Vec<KeyCode> = if rng.chance(2) { vec![KeyCode::Char('o')] } else { Vec::new() };
        keys.extend((0..rng.below(25) + 5).map(|_| *rng.pick(&WALK_KEYS)));
        for (step, code) in keys.iter().enumerate() {
            app::update(&mut with_view, key(*code));
            app::update(&mut without_view, key(*code));
            assert!(without_view.tasks_view.is_none(), "shape {shape:?} size {size:?} keys {keys:?} step {step}");
            if is_kiro_task_list_open(&with_view) {
                continue;
            }
            assert!(with_view.tasks_view.is_none(), "shape {shape:?} size {size:?} keys {keys:?} step {step}");
            let framed_with = buffer_rows(&render_app_to_buffer(&mut with_view, size.0, size.1));
            let framed_without = buffer_rows(&render_app_to_buffer(&mut without_view, size.0, size.1));
            assert_eq!(framed_with, framed_without, "shape {shape:?} size {size:?} keys {keys:?} step {step}");
        }
    });
}

// --- P4: a link that cannot be followed leaves the viewer exactly as it was --------

#[derive(Debug, PartialEq)]
struct ViewerPosition {
    tree_selection: Vec<NodeId>,
    doc_path: Option<PathBuf>,
    scroll: usize,
    selected_link: Option<(usize, String, String)>,
    shown_lines: Vec<String>,
}

fn viewer_position(state: &AppState) -> ViewerPosition {
    let (doc_path, shown_lines) = match &state.doc {
        DocView::Rendered { path, r, .. } => (Some(path.clone()), r.plain.clone()),
        _ => (None, Vec::new()),
    };
    ViewerPosition {
        tree_selection: state.tree.selected().to_vec(),
        doc_path,
        scroll: state.scroll,
        selected_link: state
            .tasks_view
            .as_ref()
            .and_then(|view| view.selected())
            .map(|link| (link.line, link.tag_name.clone(), link.value.clone())),
        shown_lines,
    }
}

/// Values no sibling document or task list contains, so following them must fail.
fn unreachable_value(tag_name: &str, rng: &mut SeededRng) -> String {
    match tag_name {
        "BizProcess" => format!("없는 제목 {}", rng.below(1000)),
        _ => (0..rng.below(3) + 1)
            .map(|_| format!("{}.{}", 90 + rng.below(9), rng.below(9)))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn write_unlinkable_spec(root: &Path, rng: &mut SeededRng) {
    let dir = root.join("specs/gen");
    fs::create_dir_all(&dir).expect("spec dir");
    fs::write(dir.join("spec.json"), r#"{"name":"gen","phase":"design","approvals":{}}"#).expect("spec.json");
    if !rng.chance(3) {
        fs::write(dir.join("requirements.md"), "# 요구사항\n\n### 1. 가입\n- 1.1: [가입한다] 계정이 생긴다\n")
            .expect("requirements");
    }
    if !rng.chance(3) {
        fs::write(dir.join("biz-process.md"), "# 업무 프로세스\n\n### L2 Activity: 가입한다  (1.1)\n")
            .expect("biz-process");
    }
    let mut tasks = String::from("# 구현 계획\n\n");
    for number in 1..=rng.below(4) + 1 {
        tasks.push_str(&format!("- [ ] {number}. 작업 {number}\n"));
        for tag_name in ["Requirements", "BizProcess", "Depends"] {
            if rng.chance(2) {
                tasks.push_str(&format!("  - _{tag_name}: {}_\n", unreachable_value(tag_name, rng)));
            }
        }
        tasks.push_str("  - _Boundary: gen::part_\n");
    }
    fs::write(dir.join("tasks.md"), tasks).expect("tasks.md");
}

#[test]
fn p4_a_failed_link_follow_changes_nothing_but_shows_a_message() {
    let mut followed_cases = 0;
    for_each_seed("P4", SLOW_SEEDS_PER_PROPERTY, |seed| {
        let mut rng = SeededRng::new(seed);
        let root = std::env::temp_dir().join(format!("spec_viewer_tasks_p4_{}_{seed}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        write_unlinkable_spec(&root, &mut rng);

        let size = (60 + rng.below(80) as u16, 20 + rng.below(20) as u16);
        let mut state = kiro_state(&root, size);
        state.tree.select(vec![NodeId::Spec("gen".into()), NodeId::Doc("gen".into(), DocKind::Tasks)]);
        app::update(&mut state, key(KeyCode::Enter));
        state.focus = Panel::Doc;
        for _ in 0..rng.below(6) + 1 {
            app::update(&mut state, key(KeyCode::Char(if rng.chance(4) { 'L' } else { 'l' })));
        }
        let before = viewer_position(&state);
        let has_selected_link = before.selected_link.is_some();
        app::update(&mut state, key(KeyCode::Enter));
        let after = viewer_position(&state);
        fs::remove_dir_all(&root).ok();

        assert_eq!(after, before, "seed {seed}");
        if has_selected_link {
            assert!(matches!(state.popup, Some(Popup::Message(_))), "seed {seed}: expected a failure message");
        }
    });
    // The generator must reach the failure path, or the property proves nothing.
    for seed in 0..SLOW_SEEDS_PER_PROPERTY {
        let mut rng = SeededRng::new(seed);
        let root = std::env::temp_dir().join(format!("spec_viewer_tasks_p4_probe_{}_{seed}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        write_unlinkable_spec(&root, &mut rng);
        let mut state = kiro_state(&root, (120, 30));
        state.tree.select(vec![NodeId::Spec("gen".into()), NodeId::Doc("gen".into(), DocKind::Tasks)]);
        app::update(&mut state, key(KeyCode::Enter));
        state.focus = Panel::Doc;
        app::update(&mut state, key(KeyCode::Char('l')));
        if viewer_position(&state).selected_link.is_some() {
            followed_cases += 1;
        }
        fs::remove_dir_all(&root).ok();
    }
    assert!(followed_cases > SLOW_SEEDS_PER_PROPERTY / 2, "only {followed_cases} generated cases had a link to follow");
}

// --- no regression: badge, search, other documents ---------------------------------

fn kiro_tasks_view_with_task_list_open(is_structured_view_enabled: bool, size: (u16, u16)) -> AppState {
    let mut state = state_for(SourceShape::KiroTasksView, is_structured_view_enabled, size);
    state.tree.select(vec![NodeId::Spec("tasks-view-sample".into()), NodeId::Doc("tasks-view-sample".into(), DocKind::Tasks)]);
    app::update(&mut state, key(KeyCode::Enter));
    state.tree.open(vec![NodeId::Spec("tasks-view-sample".into())]);
    state
}

#[test]
fn tree_panel_progress_badge_is_the_same_with_and_without_the_structured_view() {
    let size = (120, 30);
    let mut structured = kiro_tasks_view_with_task_list_open(true, size);
    let mut plain = kiro_tasks_view_with_task_list_open(false, size);
    assert!(structured.tasks_view.is_some() && plain.tasks_view.is_none());

    let structured_rows = buffer_rows(&render_app_to_buffer(&mut structured, size.0, size.1));
    let plain_rows = buffer_rows(&render_app_to_buffer(&mut plain, size.0, size.1));
    let tree_width = structured.layout.tree.width as usize;
    let tree_part = |rows: &[String]| -> Vec<String> { rows.iter().map(|row| row.chars().take(tree_width).collect()).collect() };
    assert_eq!(tree_part(&structured_rows), tree_part(&plain_rows));

    let source = fs::read_to_string(fixtures("kiro-tasks-view").join("specs/tasks-view-sample/tasks.md")).unwrap();
    let progress = count_progress(&source).expect("sample has checkboxes");
    let badge = format!("{}/{}", progress.done, progress.total);
    let tree_rows = tree_part(&structured_rows);
    assert!(
        tree_rows.iter().any(|row| row.contains("tasks.md") && row.contains(&badge)),
        "task list badge {badge} missing from the tree panel:\n{}",
        tree_rows.join("\n")
    );
    assert!(tree_rows.iter().any(|row| row.contains("tasks-view-sample") && row.contains("3/4")), "spec milestone badge");
}

#[test]
fn search_in_the_structured_view_highlights_the_matching_line_without_an_error() {
    let size = (120, 30);
    let mut state = kiro_tasks_view_with_task_list_open(true, size);
    state.focus = Panel::Doc;
    app::update(&mut state, key(KeyCode::Char('/')));
    for typed in "세션 발급".chars() {
        app::update(&mut state, key(KeyCode::Char(typed)));
    }
    app::update(&mut state, key(KeyCode::Enter));

    assert!(state.popup.is_none(), "search must close without an error popup: {:?}", state.popup);
    assert!(!state.search.matches.is_empty(), "the structured view's lines must be searchable");
    let buffer = render_app_to_buffer(&mut state, size.0, size.1);
    let (row, col) = find_text_cell(&buffer, "세션 발급").expect("matching line is on screen");
    let (_, background, _) = support::cell_style(&buffer, col, row);
    assert_ne!(background, ratatui::style::Color::Reset, "matching line must carry the highlight style");
}

#[test]
fn documents_without_a_structured_view_keep_their_frames() {
    let size = (120, 30);
    let cases: [(SourceShape, &str); 3] = [
        (SourceShape::KiroTasksView, "requirements.md"),
        (SourceShape::KiroTasksView, "biz-process.md"),
        (SourceShape::KiroSamples, "design.md"),
    ];
    for (shape, doc_name) in cases {
        let kind = match doc_name {
            "requirements.md" => DocKind::Requirements,
            "biz-process.md" => DocKind::BizProcess,
            _ => DocKind::Design,
        };
        let spec_name = if matches!(shape, SourceShape::KiroTasksView) { "tasks-view-sample" } else { "sample-signup" };
        let frame_with = |is_structured_view_enabled: bool| {
            let mut state = state_for(shape, is_structured_view_enabled, size);
            state.tree.select(vec![NodeId::Spec(spec_name.into()), NodeId::Doc(spec_name.into(), kind.clone())]);
            app::update(&mut state, key(KeyCode::Enter));
            assert!(state.tasks_view.is_none(), "{doc_name}");
            buffer_rows(&render_app_to_buffer(&mut state, size.0, size.1))
        };
        assert_eq!(frame_with(true), frame_with(false), "{shape:?} {doc_name}");
    }
}
