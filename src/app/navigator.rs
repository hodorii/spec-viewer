use crate::markdown::Rendered;
use crate::spec::methodology::{LinkRule, LinkTarget, LocateBy, ValueSplit};
use std::path::{Path, PathBuf};

/// Separates the two ends of a range value such as `2.1~2.5`.
pub(crate) const NUMBER_RANGE_SEPARATOR: char = '~';

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationError {
    /// None of the rule's candidate file names exists; carries the names tried.
    TargetFileMissing(Vec<String>),
    /// The target exists but the caller could not read it.
    TargetUnreadable(PathBuf),
    /// The target has no line for this value.
    ReferenceNotFound(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationTarget {
    pub file: PathBuf,
    pub is_same_file: bool,
}

/// Splits a raw tag value into the pieces the rule treats as separate targets.
pub fn link_values(rule: &LinkRule, raw_value: &str) -> Vec<String> {
    match rule.values {
        ValueSplit::CommaList => raw_value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(String::from)
            .collect(),
        ValueSplit::Whole => {
            let whole = raw_value.trim();
            if whole.is_empty() { Vec::new() } else { vec![whole.to_string()] }
        }
    }
}

/// Picks the first candidate target that exists next to the task list.
pub fn resolve_target_file(rule: &LinkRule, task_list: &Path) -> Result<NavigationTarget, NavigationError> {
    let sibling_dir = task_list.parent().unwrap_or_else(|| Path::new(""));
    let mut tried_names = Vec::new();
    for target in &rule.targets {
        match target {
            LinkTarget::SameFile => {
                return Ok(NavigationTarget { file: task_list.to_path_buf(), is_same_file: true });
            }
            LinkTarget::File(name) => {
                let candidate = sibling_dir.join(name);
                if candidate.is_file() {
                    return Ok(NavigationTarget { file: candidate, is_same_file: false });
                }
                tried_names.push(name.clone());
            }
        }
    }
    Err(NavigationError::TargetFileMissing(tried_names))
}

/// Finds the rendered line a link value points at.
pub fn locate_line(rendered: &Rendered, locate: LocateBy, value: &str) -> Result<usize, NavigationError> {
    let found = match locate {
        LocateBy::LeadingNumber => locate_by_number(rendered, range_start(value)),
        LocateBy::TitleText => locate_by_title(rendered, value),
    };
    found.ok_or_else(|| NavigationError::ReferenceNotFound(value.to_string()))
}

fn range_start(value: &str) -> &str {
    value.split(NUMBER_RANGE_SEPARATOR).next().unwrap_or(value).trim()
}

fn locate_by_number(rendered: &Rendered, number: &str) -> Option<usize> {
    if number.is_empty() {
        return None;
    }
    rendered.plain.iter().position(|line| line_starts_with_number(line, number))
}

/// Skips leading decoration (bullets, `#`, a `[x]` box) and then requires the
/// number to end there, so `3` never matches a `3.1` or `31` line.
fn line_starts_with_number(line: &str, number: &str) -> bool {
    let body = skip_decoration(line);
    let Some(after) = body.strip_prefix(number) else {
        return false;
    };
    let mut rest = after.chars();
    match rest.next() {
        None => true,
        Some(next) if next.is_ascii_digit() => false,
        Some('.') => !rest.next().is_some_and(|digit| digit.is_ascii_digit()),
        Some(_) => true,
    }
}

fn skip_decoration(line: &str) -> &str {
    let mut rest = line;
    loop {
        let trimmed = rest.trim_start();
        if let Some(after_box) = strip_checkbox(trimmed) {
            rest = after_box;
            continue;
        }
        match trimmed.chars().next() {
            Some(first) if !first.is_alphanumeric() => rest = &trimmed[first.len_utf8()..],
            _ => return trimmed,
        }
    }
}

fn strip_checkbox(text: &str) -> Option<&str> {
    let mut chars = text.chars();
    (chars.next()? == '[').then_some(())?;
    chars.next()?;
    (chars.next()? == ']').then_some(chars.as_str())
}

fn locate_by_title(rendered: &Rendered, title: &str) -> Option<usize> {
    if title.is_empty() {
        return None;
    }
    rendered
        .headings
        .iter()
        .find(|heading| heading.text.contains(title))
        .map(|heading| heading.line)
        .or_else(|| rendered.plain.iter().position(|line| line.contains(title)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown;
    use crate::spec::methodology::MethodologySettings;
    use std::fs;

    fn rendered_of(lines: &[&str]) -> Rendered {
        Rendered {
            lines: Vec::new(),
            plain: lines.iter().map(|line| line.to_string()).collect(),
            headings: Vec::new(),
            footnotes: Vec::new(),
        }
    }

    fn rule_named(name: &str) -> LinkRule {
        MethodologySettings::kiro_default().rule_for(name).unwrap().link.clone().unwrap()
    }

    fn fixture_dir() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro-tasks-view/specs/tasks-view-sample"))
    }

    fn render_fixture(file: &str) -> Rendered {
        markdown::render(&fs::read_to_string(fixture_dir().join(file)).unwrap(), 100)
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("spec_viewer_navigator_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn comma_list_splits_and_trims() {
        assert_eq!(link_values(&rule_named("Requirements"), "1.1, 2.3 ,,3"), vec!["1.1", "2.3", "3"]);
    }

    #[test]
    fn whole_value_is_kept_as_one_trimmed_piece() {
        assert_eq!(link_values(&rule_named("BizProcess"), "  a, b  "), vec!["a, b"]);
        assert!(link_values(&rule_named("BizProcess"), "   ").is_empty());
    }

    #[test]
    fn number_does_not_match_a_longer_number_line() {
        let rendered = rendered_of(&["3.1 sub clause", "3.10 other", "3 exact"]);
        assert_eq!(locate_line(&rendered, LocateBy::LeadingNumber, "3"), Ok(2));
        assert_eq!(locate_line(&rendered, LocateBy::LeadingNumber, "3.1"), Ok(0));
    }

    #[test]
    fn number_missing_when_only_longer_numbers_exist() {
        let rendered = rendered_of(&["3.1 a", "3.2 b"]);
        assert_eq!(
            locate_line(&rendered, LocateBy::LeadingNumber, "3"),
            Err(NavigationError::ReferenceNotFound("3".into()))
        );
    }

    #[test]
    fn number_skips_bullet_checkbox_and_heading_decoration() {
        for line in ["- 2.1 clause", "  * 2.1 clause", "- [ ] 2.1 clause", "- [x] 2.1 clause", "### 2.1 clause", "• 2.1. clause"] {
            let rendered = rendered_of(&["intro", line]);
            assert_eq!(locate_line(&rendered, LocateBy::LeadingNumber, "2.1"), Ok(1), "{line}");
        }
    }

    #[test]
    fn number_in_the_middle_of_a_line_is_not_a_match() {
        let rendered = rendered_of(&["see 2.1 for details", "task 2.1"]);
        assert!(locate_line(&rendered, LocateBy::LeadingNumber, "2.1").is_err());
    }

    #[test]
    fn range_value_goes_to_its_start() {
        let rendered = rendered_of(&["2.1 first", "2.5 last"]);
        assert_eq!(locate_line(&rendered, LocateBy::LeadingNumber, "2.1~2.5"), Ok(0));
    }

    #[test]
    fn title_prefers_headings_over_body_lines() {
        let mut rendered = rendered_of(&["mentions 세션을 관리한다 in prose", "heading line"]);
        rendered.headings.push(markdown::HeadingAnchor { level: 3, text: "세션을 관리한다".into(), line: 1 });
        assert_eq!(locate_line(&rendered, LocateBy::TitleText, "세션을 관리한다"), Ok(1));
    }

    #[test]
    fn title_falls_back_to_plain_lines_as_substring() {
        let rendered = rendered_of(&["", "L2-A1 Activity: 탈퇴를 처리한다  (3.1)"]);
        assert_eq!(locate_line(&rendered, LocateBy::TitleText, "탈퇴를 처리한다"), Ok(1));
    }

    #[test]
    fn title_not_found_reports_the_value() {
        let rendered = rendered_of(&["nothing here"]);
        assert_eq!(
            locate_line(&rendered, LocateBy::TitleText, "없는 제목"),
            Err(NavigationError::ReferenceNotFound("없는 제목".into()))
        );
        assert_eq!(
            locate_line(&rendered, LocateBy::TitleText, ""),
            Err(NavigationError::ReferenceNotFound(String::new()))
        );
    }

    #[test]
    fn fixture_biz_process_new_style_heading_is_found() {
        let rendered = render_fixture("biz-process.md");
        let line = locate_line(&rendered, LocateBy::TitleText, "세션을 관리한다").unwrap();
        assert!(rendered.plain[line].contains("세션을 관리한다"));
    }

    #[test]
    fn fixture_biz_process_old_style_indented_id_is_found() {
        let rendered = render_fixture("biz-process.md");
        let line = locate_line(&rendered, LocateBy::TitleText, "탈퇴를 처리한다").unwrap();
        assert!(rendered.plain[line].contains("탈퇴를 처리한다"));
    }

    #[test]
    fn fixture_requirement_number_is_found() {
        let rendered = render_fixture("requirements.md");
        let line = locate_line(&rendered, LocateBy::LeadingNumber, "1.1").unwrap();
        assert!(rendered.plain[line].contains("1.1"));
    }

    #[test]
    fn same_file_target_is_the_task_list_itself() {
        let task_list = fixture_dir().join("tasks.md");
        let target = resolve_target_file(&rule_named("Depends"), &task_list).unwrap();
        assert_eq!(target, NavigationTarget { file: task_list, is_same_file: true });
    }

    #[test]
    fn first_existing_candidate_wins() {
        let dir = scratch_dir("first_wins");
        fs::write(dir.join("tasks.md"), "").unwrap();
        fs::write(dir.join("bugfix.md"), "").unwrap();
        let target = resolve_target_file(&rule_named("Requirements"), &dir.join("tasks.md")).unwrap();
        assert_eq!(target, NavigationTarget { file: dir.join("bugfix.md"), is_same_file: false });
        fs::write(dir.join("requirements.md"), "").unwrap();
        let target = resolve_target_file(&rule_named("Requirements"), &dir.join("tasks.md")).unwrap();
        assert_eq!(target.file, dir.join("requirements.md"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_existing_candidate_lists_the_names_tried() {
        let dir = scratch_dir("none_exist");
        fs::write(dir.join("tasks.md"), "").unwrap();
        assert_eq!(
            resolve_target_file(&rule_named("Requirements"), &dir.join("tasks.md")),
            Err(NavigationError::TargetFileMissing(vec!["requirements.md".into(), "bugfix.md".into()]))
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
