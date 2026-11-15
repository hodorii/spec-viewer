/// Aggregate checkbox counts from a `tasks.md` document.
///
/// Progress is a flat count of every recognized checkbox line in the
/// document — no attempt is made to parse task numbering (`3.1`, etc.) or
/// nesting (requirement 4.1's "체크박스 집계").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: u32,
    pub total: u32,
}

/// Count `[x]`/`[X]`/`[ ]`/`[ ]*` checkbox lines in `tasks_md`.
///
/// Recognition rule (requirement 4.2): a line counts as a checkbox line if,
/// after stripping leading whitespace and an optional leading `- ` or `-`
/// list marker, it starts with `[x]`, `[X]`, `[ ]`, or the optional-task
/// variant `[ ]*`. `[ ]*` is treated as an ordinary unchecked checkbox for
/// counting purposes: it contributes to `total` but not `done`.
///
/// Returns `None` when no checkbox lines are found anywhere in the document
/// (requirement 4.3's "체크박스 없음 → 진행률 없음 표시"); otherwise returns
/// `Some(Progress { done, total })`.
pub fn count_progress(tasks_md: &str) -> Option<Progress> {
    let mut done = 0u32;
    let mut total = 0u32;

    for line in tasks_md.lines() {
        // `[ ]*` (optional task) is an ordinary unchecked checkbox here: it
        // contributes to `total` but not `done`.
        if let Some((state, _)) = parse_checkbox_marker(line) {
            total += 1;
            if state == TaskState::Done {
                done += 1;
            }
        }
    }

    if total == 0 {
        None
    } else {
        Some(Progress { done, total })
    }
}

/// Checkbox state of one task line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Done,
    Open,
    OptionalOpen,
}

/// Recognize a checkbox line and split it into its state and the text after the marker.
pub(crate) fn parse_checkbox_marker(line: &str) -> Option<(TaskState, &str)> {
    let after_marker = strip_list_marker(line.trim_start());
    let (state, after_checkbox) = if let Some(rest) = after_marker
        .strip_prefix("[x]")
        .or_else(|| after_marker.strip_prefix("[X]"))
    {
        (TaskState::Done, rest)
    } else if let Some(rest) = after_marker.strip_prefix("[ ]*") {
        (TaskState::OptionalOpen, rest)
    } else {
        (TaskState::Open, after_marker.strip_prefix("[ ]")?)
    };
    Some((state, after_checkbox.trim_start()))
}

/// Strip an optional leading `- ` or `-` Markdown list marker, if present.
fn strip_list_marker(trimmed: &str) -> &str {
    trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix('-'))
        .unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SIGNUP: &str =
        include_str!("../../tests/fixtures/kiro/specs/sample-signup/tasks.md");
    const NO_CHECKBOXES: &str =
        include_str!("../../tests/fixtures/kiro/specs/no-checkboxes/tasks.md");

    #[test]
    fn sample_signup_counts_mixed_checkboxes() {
        // Fixture (hand-counted):
        // - [x] 1.1 First task done
        // - [x] 1.2 Second task done
        // - [ ] 2.1 Third task pending
        // - [ ] 2.2 Fourth task pending
        // - [X] 3.1 Fifth task done (capital X)
        // done = 3 (two lowercase [x], one uppercase [X]); total = 5.
        let progress = count_progress(SAMPLE_SIGNUP).expect("should find checkboxes");

        assert_eq!(progress, Progress { done: 3, total: 5 });
    }

    #[test]
    fn no_checkboxes_returns_none() {
        assert_eq!(count_progress(NO_CHECKBOXES), None);
    }

    #[test]
    fn optional_task_marker_counts_toward_total_not_done() {
        let md = "\
- [x] 1 done
- [ ]* 2 optional pending
- [ ] 3 pending
";
        let progress = count_progress(md).expect("should find checkboxes");

        assert_eq!(progress, Progress { done: 1, total: 3 });
    }

    #[test]
    fn mixed_case_x_both_count_toward_done() {
        let md = "\
- [x] a lowercase done
- [X] b uppercase done
- [ ] c pending
";
        let progress = count_progress(md).expect("should find checkboxes");

        assert_eq!(progress, Progress { done: 2, total: 3 });
    }

    #[test]
    fn non_checkbox_bullets_do_not_inflate_total() {
        let md = "\
- [x] 1.1 do the thing
  - DONE: told you so
  - _Requirements: 1.1_
- [ ] 1.2 pending thing
";
        let progress = count_progress(md).expect("should find checkboxes");

        assert_eq!(progress, Progress { done: 1, total: 2 });
    }

    #[test]
    fn all_done_reports_done_equal_total() {
        let md = "\
- [x] 1 done
- [X] 2 done
";
        let progress = count_progress(md).expect("should find checkboxes");

        assert_eq!(progress, Progress { done: 2, total: 2 });
        assert_eq!(progress.done, progress.total);
    }

    #[test]
    fn marker_done_variants_with_rest() {
        assert_eq!(parse_checkbox_marker("- [x] 1.1 title"), Some((TaskState::Done, "1.1 title")));
        assert_eq!(parse_checkbox_marker("  - [X] 2 t"), Some((TaskState::Done, "2 t")));
    }

    #[test]
    fn marker_open_and_optional_open() {
        assert_eq!(parse_checkbox_marker("- [ ] 3 t"), Some((TaskState::Open, "3 t")));
        assert_eq!(parse_checkbox_marker("- [ ]* 4 t"), Some((TaskState::OptionalOpen, "4 t")));
    }

    #[test]
    fn marker_accepts_bare_checkbox_and_dash_without_space() {
        assert_eq!(parse_checkbox_marker("[x] a"), Some((TaskState::Done, "a")));
        assert_eq!(parse_checkbox_marker("-[ ] b"), Some((TaskState::Open, "b")));
    }

    #[test]
    fn marker_rejects_non_checkbox_lines() {
        assert_eq!(parse_checkbox_marker("  - DONE: x"), None);
        assert_eq!(parse_checkbox_marker("# heading"), None);
        assert_eq!(parse_checkbox_marker(""), None);
    }
}
