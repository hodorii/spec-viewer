//! Spec-tree sort order (requirement 2.9): a cyclable key plus the pure
//! sort function that orders a `SpecRoot`'s specs by it.

use super::Spec;

/// Requirement 2.9's four sort keys, in the fixed cycle order the hotkey
/// (and `--sort`) advance through: `Name -> Phase -> Updated -> Progress ->
/// Name`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Name,
    Phase,
    Updated,
    Progress,
}

impl SortKey {
    /// Advance to the next key in the fixed cycle (requirement 2.9 "정렬
    /// 키를 핫키로 순환").
    pub fn cycle(self) -> SortKey {
        match self {
            SortKey::Name => SortKey::Phase,
            SortKey::Phase => SortKey::Updated,
            SortKey::Updated => SortKey::Progress,
            SortKey::Progress => SortKey::Name,
        }
    }

    /// Short label for the tree panel title (design.md "현재 정렬 키가
    /// 트리 패널 제목에 표시").
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "이름",
            SortKey::Phase => "단계",
            SortKey::Updated => "최근 갱신",
            SortKey::Progress => "진행률",
        }
    }

    /// Parse a `--sort` CLI value (case-insensitive). `None` for anything
    /// unrecognized -- the caller decides the fallback.
    pub fn from_cli(s: &str) -> Option<SortKey> {
        match s.to_ascii_lowercase().as_str() {
            "name" => Some(SortKey::Name),
            "phase" => Some(SortKey::Phase),
            "updated" => Some(SortKey::Updated),
            "progress" => Some(SortKey::Progress),
            _ => None,
        }
    }
}

/// A spec's overall completion ratio, from its `Tasks` doc's `Progress` (the
/// only doc slot that ever carries one -- see `spec::build`). `None` (no
/// checkboxes counted, or spec.json failed to parse and no doc carries
/// progress either) sorts as the lowest possible value, i.e. always last
/// under `SortKey::Progress`'s descending order.
fn progress_ratio(spec: &Spec) -> f64 {
    spec.docs
        .iter()
        .find_map(|d| d.progress)
        .map(|p| p.done as f64 / p.total.max(1) as f64)
        .unwrap_or(-1.0)
}

fn phase_key(spec: &Spec) -> &str {
    spec.meta.as_ref().map(|m| m.phase.as_str()).unwrap_or("")
}

/// `updated_at` as its raw ISO-8601 string (lexicographically sortable);
/// `""` when missing or `spec.json` failed to parse, which always sorts
/// before any real date -- last under `SortKey::Updated`'s descending
/// (most-recent-first) order.
fn updated_key(spec: &Spec) -> &str {
    spec.meta
        .as_ref()
        .ok()
        .and_then(|m| m.updated_at.as_deref())
        .unwrap_or("")
}

/// Sort `specs` in place by `key` (requirement 2.9). Stable
/// ([`slice::sort_by`]) with `name` as a secondary tie-break on every key
/// but `Name` itself, so equal-ranked specs land in a deterministic order
/// rather than whatever order they happened to arrive in.
///
/// Direction: `Name`/`Phase` ascending (alphabetical); `Updated`/`Progress`
/// descending (most recently updated / most complete first) -- the reading
/// one most often wants from either.
pub fn sort_specs(specs: &mut [Spec], key: SortKey) {
    match key {
        SortKey::Name => specs.sort_by(|a, b| a.name.cmp(&b.name)),
        SortKey::Phase => {
            specs.sort_by(|a, b| phase_key(a).cmp(phase_key(b)).then_with(|| a.name.cmp(&b.name)))
        }
        SortKey::Updated => specs.sort_by(|a, b| {
            updated_key(b).cmp(updated_key(a)).then_with(|| a.name.cmp(&b.name))
        }),
        SortKey::Progress => specs.sort_by(|a, b| {
            progress_ratio(b)
                .partial_cmp(&progress_ratio(a))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.name.cmp(&b.name))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{DocEntry, DocKind, DocStatus, MetaError, Progress, SpecMeta};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn spec(name: &str, phase: &str, updated_at: Option<&str>, progress: Option<(u32, u32)>) -> Spec {
        Spec {
            name: name.to_string(),
            dir: PathBuf::from(format!("/does/not/matter/{name}")),
            meta: Ok(SpecMeta {
                name: name.to_string(),
                phase: phase.to_string(),
                approvals: BTreeMap::new(),
                updated_at: updated_at.map(str::to_string),
            }),
            docs: vec![DocEntry {
                kind: DocKind::Tasks,
                path: PathBuf::from("/does/not/matter/tasks.md"),
                exists: true,
                status: DocStatus::NotTracked,
                progress: progress.map(|(done, total)| Progress { done, total }),
            }],
            definition: None,
        }
    }

    fn broken_meta_spec(name: &str) -> Spec {
        Spec {
            name: name.to_string(),
            dir: PathBuf::from(format!("/does/not/matter/{name}")),
            meta: Err(MetaError::InvalidJson("n/a".to_string())),
            docs: vec![],
            definition: None,
        }
    }

    fn names(specs: &[Spec]) -> Vec<&str> {
        specs.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn cycle_advances_through_all_four_keys_and_wraps() {
        assert_eq!(SortKey::Name.cycle(), SortKey::Phase);
        assert_eq!(SortKey::Phase.cycle(), SortKey::Updated);
        assert_eq!(SortKey::Updated.cycle(), SortKey::Progress);
        assert_eq!(SortKey::Progress.cycle(), SortKey::Name);
    }

    #[test]
    fn from_cli_parses_case_insensitively_and_rejects_unknown() {
        assert_eq!(SortKey::from_cli("Progress"), Some(SortKey::Progress));
        assert_eq!(SortKey::from_cli("updated"), Some(SortKey::Updated));
        assert_eq!(SortKey::from_cli("bogus"), None);
    }

    #[test]
    fn sort_by_name_is_alphabetical() {
        let mut specs = vec![spec("charlie", "", None, None), spec("alpha", "", None, None), spec("bravo", "", None, None)];
        sort_specs(&mut specs, SortKey::Name);
        assert_eq!(names(&specs), vec!["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn sort_by_phase_is_alphabetical_with_name_tiebreak() {
        let mut specs = vec![
            spec("z-spec", "design", None, None),
            spec("a-spec", "design", None, None),
            spec("b-spec", "discovery", None, None),
        ];
        sort_specs(&mut specs, SortKey::Phase);
        assert_eq!(names(&specs), vec!["a-spec", "z-spec", "b-spec"]);
    }

    #[test]
    fn sort_by_updated_is_most_recent_first_missing_sorts_last() {
        let mut specs = vec![
            spec("old", "", Some("2026-01-01T00:00:00Z"), None),
            spec("new", "", Some("2026-03-01T00:00:00Z"), None),
            spec("no-date", "", None, None),
            spec("mid", "", Some("2026-02-01T00:00:00Z"), None),
        ];
        sort_specs(&mut specs, SortKey::Updated);
        assert_eq!(names(&specs), vec!["new", "mid", "old", "no-date"]);
    }

    #[test]
    fn sort_by_progress_is_most_complete_first_missing_sorts_last() {
        let mut specs = vec![
            spec("half", "", None, Some((1, 2))),
            spec("done", "", None, Some((5, 5))),
            spec("none", "", None, None),
            spec("quarter", "", None, Some((1, 4))),
        ];
        sort_specs(&mut specs, SortKey::Progress);
        assert_eq!(names(&specs), vec!["done", "half", "quarter", "none"]);
    }

    #[test]
    fn broken_spec_json_does_not_panic_and_sorts_deterministically() {
        // requirement 3.4/3.5's tolerance extends to sorting: a spec whose
        // spec.json failed to parse (`meta: Err`) must never panic any of
        // the four sort keys, and its missing phase/updated_at/progress
        // must resolve to a deterministic (if arbitrary) position rather
        // than an unwrap panic.
        let mut specs = vec![
            spec("a-normal", "design", Some("2026-01-01T00:00:00Z"), None),
            broken_meta_spec("z-broken"),
        ];
        // `Phase`: an empty phase key ("") sorts *before* any real phase
        // alphabetically -- ascending order has no "missing sorts last"
        // special case, unlike Updated/Progress below.
        sort_specs(&mut specs, SortKey::Phase);
        assert_eq!(names(&specs), vec!["z-broken", "a-normal"]);

        // `Updated`: a real date always outranks a missing one under
        // descending (most-recent-first) order.
        sort_specs(&mut specs, SortKey::Updated);
        assert_eq!(names(&specs), vec!["a-normal", "z-broken"]);
    }
}
