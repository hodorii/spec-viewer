use crate::spec::methodology::MethodologySettings;
use crate::spec::progress::parse_checkbox_marker;
pub use crate::spec::progress::TaskState;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TasksDocument {
    pub blocks: Vec<TasksBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TasksBlock {
    Heading { level: u8, text: String },
    Text(String),
    Task(TaskItem),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskItem {
    pub number: Option<String>,
    pub state: TaskState,
    pub is_parallel: bool,
    pub title: String,
    pub tags: Vec<TaskTag>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTag {
    pub name: String,
    pub value: String,
}

const PARALLEL_MARKER: &str = "(P)";
const MAX_HEADING_LEVEL: usize = 6;

pub fn parse_tasks(source: &str, settings: &MethodologySettings) -> TasksDocument {
    let mut blocks = Vec::new();
    let mut open_task: Option<TaskItem> = None;
    for line in source.lines() {
        if let Some((state, after_marker)) = parse_checkbox_marker(line) {
            close_task(&mut open_task, &mut blocks);
            open_task = Some(task_from_marker_text(state, after_marker));
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let is_indented = line.starts_with(char::is_whitespace);
        match open_task.as_mut() {
            Some(task) if is_indented => attach_detail(task, trimmed, settings),
            _ => {
                close_task(&mut open_task, &mut blocks);
                blocks.push(heading_or_text(trimmed));
            }
        }
    }
    close_task(&mut open_task, &mut blocks);
    TasksDocument { blocks }
}

fn close_task(open_task: &mut Option<TaskItem>, blocks: &mut Vec<TasksBlock>) {
    if let Some(task) = open_task.take() {
        blocks.push(TasksBlock::Task(task));
    }
}

fn task_from_marker_text(state: TaskState, after_marker: &str) -> TaskItem {
    let mut rest = after_marker.trim();
    let number = match rest.split_once(char::is_whitespace) {
        Some((first, tail)) if is_task_number(first) => {
            rest = tail.trim_start();
            Some(first.trim_end_matches('.').to_string())
        }
        None if is_task_number(rest) => {
            let number = rest.trim_end_matches('.').to_string();
            rest = "";
            Some(number)
        }
        _ => None,
    };
    let is_parallel = match rest.strip_prefix(PARALLEL_MARKER) {
        Some(tail) if tail.is_empty() || tail.starts_with(char::is_whitespace) => {
            rest = tail.trim_start();
            true
        }
        _ => false,
    };
    TaskItem {
        number,
        state,
        is_parallel,
        title: rest.to_string(),
        tags: Vec::new(),
        notes: Vec::new(),
    }
}

/// Digits separated by dots (`1`, `2.1`), optionally with one trailing dot (`4.`).
pub(crate) fn is_task_number(token: &str) -> bool {
    let body = token.strip_suffix('.').unwrap_or(token);
    !body.is_empty()
        && body
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

fn attach_detail(task: &mut TaskItem, trimmed_line: &str, settings: &MethodologySettings) {
    let detail = trimmed_line.strip_prefix("- ").unwrap_or(trimmed_line);
    match parse_tag(detail, settings) {
        Some(tag) => task.tags.push(tag),
        None => task.notes.push(detail.to_string()),
    }
}

/// Recognizes `_Name: value_` and the bare `Name: value` form, the latter only
/// for names or aliases the settings declare (so ordinary prose with a colon stays a note).
fn parse_tag(detail: &str, settings: &MethodologySettings) -> Option<TaskTag> {
    let underscored = detail
        .strip_prefix('_')
        .and_then(|inner| inner.strip_suffix('_'));
    let (raw_name, raw_value) = underscored.unwrap_or(detail).split_once(':')?;
    let name = raw_name.trim();
    let rule = settings.rule_for(name);
    let is_tag_shape = if underscored.is_some() {
        !name.is_empty() && !name.contains(char::is_whitespace)
    } else {
        rule.is_some()
    };
    if !is_tag_shape {
        return None;
    }
    Some(TaskTag {
        name: rule.map_or_else(|| name.to_string(), |rule| rule.name.clone()),
        value: raw_value.trim().to_string(),
    })
}

fn heading_or_text(trimmed_line: &str) -> TasksBlock {
    let level = trimmed_line.chars().take_while(|&c| c == '#').count();
    if (1..=MAX_HEADING_LEVEL).contains(&level) {
        if let Some(text) = trimmed_line[level..].strip_prefix(' ') {
            return TasksBlock::Heading { level: level as u8, text: text.trim().to_string() };
        }
    }
    TasksBlock::Text(trimmed_line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::count_progress;

    const SAMPLE: &str =
        include_str!("../../tests/fixtures/kiro-tasks-view/specs/tasks-view-sample/tasks.md");

    fn parse(source: &str) -> TasksDocument {
        parse_tasks(source, &MethodologySettings::kiro_default())
    }

    fn tasks_of(document: &TasksDocument) -> Vec<&TaskItem> {
        document
            .blocks
            .iter()
            .filter_map(|block| match block {
                TasksBlock::Task(task) => Some(task),
                _ => None,
            })
            .collect()
    }

    fn tag<'a>(task: &'a TaskItem, name: &str) -> Option<&'a str> {
        task.tags.iter().find(|t| t.name == name).map(|t| t.value.as_str())
    }

    #[test]
    fn parses_number_state_and_title() {
        let document = parse("- [x] 1.1 가입 양식\n- [ ] 2 로그인\n- [ ]* 3.1 탈퇴\n");
        let tasks = tasks_of(&document);
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].number.as_deref(), Some("1.1"));
        assert_eq!(tasks[0].state, TaskState::Done);
        assert_eq!(tasks[0].title, "가입 양식");
        assert_eq!(tasks[1].number.as_deref(), Some("2"));
        assert_eq!(tasks[1].state, TaskState::Open);
        assert_eq!(tasks[2].state, TaskState::OptionalOpen);
    }

    #[test]
    fn trailing_dot_in_major_number_is_dropped() {
        let document = parse("- [ ] 4. 검증\n");
        assert_eq!(tasks_of(&document)[0].number.as_deref(), Some("4"));
        assert_eq!(tasks_of(&document)[0].title, "검증");
    }

    #[test]
    fn parallel_marker_is_flag_not_title() {
        let document = parse("- [ ] 1.1 (P) 병렬 작업\n- [ ] 1.2 직렬 작업\n");
        let tasks = tasks_of(&document);
        assert!(tasks[0].is_parallel);
        assert_eq!(tasks[0].title, "병렬 작업");
        assert!(!tasks[1].is_parallel);
    }

    #[test]
    fn task_without_number_keeps_whole_text_as_title() {
        let document = parse("- [ ] 번호 없는 작업\n");
        let task = tasks_of(&document)[0].clone();
        assert_eq!(task.number, None);
        assert_eq!(task.title, "번호 없는 작업");
    }

    #[test]
    fn underscore_tags_attach_to_preceding_task() {
        let document = parse(
            "- [ ] 1.1 a\n  - _Requirements: 1.1, 1.2_\n  - _Boundary: app::mod_\n- [ ] 1.2 b\n  - _Depends: 1.1_\n",
        );
        let tasks = tasks_of(&document);
        assert_eq!(tag(tasks[0], "Requirements"), Some("1.1, 1.2"));
        assert_eq!(tag(tasks[0], "Boundary"), Some("app::mod"));
        assert_eq!(tag(tasks[1], "Depends"), Some("1.1"));
        assert_eq!(tasks[1].tags.len(), 1);
    }

    #[test]
    fn done_alias_is_normalized_to_done_when() {
        let document = parse("- [ ] 1 a\n  - DONE: 끝나면 보인다\n  - _DoneWhen: 또 하나_\n");
        let task = tasks_of(&document)[0].clone();
        assert_eq!(
            task.tags,
            vec![
                TaskTag { name: "DoneWhen".into(), value: "끝나면 보인다".into() },
                TaskTag { name: "DoneWhen".into(), value: "또 하나".into() },
            ]
        );
        assert!(task.notes.is_empty());
    }

    #[test]
    fn indented_plain_lines_are_notes() {
        let document = parse("- [ ] 1 a\n  - 이메일 규칙을 검사한다\n  설명 줄\n");
        let task = tasks_of(&document)[0].clone();
        assert_eq!(task.notes, vec!["이메일 규칙을 검사한다".to_string(), "설명 줄".to_string()]);
        assert!(task.tags.is_empty());
    }

    #[test]
    fn unindented_non_checkbox_line_closes_the_task() {
        let document = parse("- [ ] 1 a\n  - _Verify: x_\n## 다음 절\n본문\n  - _Verify: 고아_\n");
        assert_eq!(document.blocks.len(), 4);
        assert_eq!(tag(tasks_of(&document)[0], "Verify"), Some("x"));
        assert_eq!(document.blocks[1], TasksBlock::Heading { level: 2, text: "다음 절".into() });
        assert_eq!(document.blocks[2], TasksBlock::Text("본문".into()));
        assert!(matches!(document.blocks[3], TasksBlock::Text(_)));
    }

    #[test]
    fn headings_and_text_before_tasks_become_blocks() {
        let document = parse("# 제목\n\n설명 한 줄\n\n- [ ] 1 a\n");
        assert_eq!(document.blocks[0], TasksBlock::Heading { level: 1, text: "제목".into() });
        assert_eq!(document.blocks[1], TasksBlock::Text("설명 한 줄".into()));
        assert!(matches!(document.blocks[2], TasksBlock::Task(_)));
    }

    #[test]
    fn unknown_underscore_tag_is_kept_with_its_own_name() {
        let document = parse("- [ ] 1 a\n  - _Custom: v_\n");
        assert_eq!(tag(tasks_of(&document)[0], "Custom"), Some("v"));
    }

    #[test]
    fn no_checkboxes_yields_no_tasks_and_empty_input_yields_empty_document() {
        assert!(tasks_of(&parse("# 제목\n본문\n")).is_empty());
        assert_eq!(parse(""), TasksDocument::default());
        assert!(parse("\n\n  \n").blocks.is_empty());
    }

    #[test]
    fn settings_supply_alias_names() {
        let settings = MethodologySettings::parse_overrides(
            r#"{"tags":[{"name":"Goal","aliases":["AIM"]}]}"#,
        )
        .unwrap();
        let document = parse_tasks("- [ ] 1 a\n  - AIM: 목표\n", &settings);
        assert_eq!(tag(tasks_of(&document)[0], "Goal"), Some("목표"));
    }

    #[test]
    fn sample_task_count_matches_progress_total() {
        let document = parse(SAMPLE);
        let progress = count_progress(SAMPLE).expect("sample has checkboxes");
        assert_eq!(tasks_of(&document).len(), progress.total as usize);
        assert_eq!(tasks_of(&document).len(), 7);
    }

    #[test]
    fn sample_parallel_and_alias_survive() {
        let document = parse(SAMPLE);
        let tasks = tasks_of(&document);
        let second = tasks.iter().find(|t| t.number.as_deref() == Some("1.2")).unwrap();
        assert!(second.is_parallel);
        assert_eq!(tag(second, "DoneWhen"), Some("새 계정이 저장소에 기록된다"));
    }
}
