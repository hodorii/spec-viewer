//! GitHub spec-kit(`.specify/` + `specs/<NNN-이름>/`) 프로젝트 인식.

use std::fs;
use std::path::{Path, PathBuf};

use crate::spec::progress::count_progress;
use crate::spec::{DocEntry, DocKind, DocStatus, Milestone, Spec};

/// `.kiro::find_root`와 짝을 이루되, `.specify/`가 아니라 **그 부모
/// 디렉터리(프로젝트 루트)** 를 반환한다 — spec-kit의 `specs/`가
/// `.specify/`의 형제 디렉터리라서, 호출부가 `.kiro`든 spec-kit이든 항상
/// `root.join("specs")`로 같은 방식으로 접근할 수 있게 하기 위함
/// (research.md "spec-kit 루트는 프로젝트 루트를 반환").
///
/// 같은 디렉터리에 `.kiro/`와 `.specify/`가 함께 있으면 `.kiro`가 우선이므로
/// (요구사항 1.2, 기존 사용자 하위호환), 그 경우 이 함수는 `None`을 반환해
/// 호출부(main.rs, 이번 태스크 범위 밖)가 `.kiro` 경로를 타도록 한다 —
/// 즉 이 함수 자체가 "같은 디렉터리 공존 시 .kiro 우선"까지 책임진다.
pub fn find_spec_kit_root(start: &Path) -> Option<PathBuf> {
    let marker_dir = find_specify_marker(start)?;
    let candidate_root = marker_dir.parent()?.to_path_buf();

    if candidate_root.join(".kiro").is_dir() {
        return None;
    }

    Some(candidate_root)
}

/// `.kiro::find_root`와 동일한 3단계 탐색 순서를 `.specify` 이름으로 수행해,
/// 마커 디렉터리(`.specify` 자신) 경로를 반환한다.
fn find_specify_marker(start: &Path) -> Option<PathBuf> {
    let child = start.join(".specify");
    if child.is_dir() {
        return Some(child);
    }

    if start.file_name().map(|n| n == ".specify").unwrap_or(false) && start.is_dir() {
        return Some(start.to_path_buf());
    }

    for ancestor in start.ancestors().skip(1) {
        let candidate = ancestor.join(".specify");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }

    None
}

/// Canonical spec-kit document order (design.md/research.md): every entry
/// here except `tasks.md` is a plain existence check (no content read,
/// requirement 8.1). `contracts/` is handled separately since it is a
/// directory, not a file.
const CANONICAL_SPEC_KIT_FILES: [&str; 6] = [
    "spec.md",
    "plan.md",
    "tasks.md",
    "research.md",
    "data-model.md",
    "quickstart.md",
];

/// Assemble one [`Spec`] per immediate subdirectory of `specs_dir` (a
/// spec-kit feature directory), read-only except for `tasks.md`'s checkbox
/// progress (requirement 8.1, 6.1/6.2).
///
/// Returns an empty `Vec` (never panics) when `specs_dir` does not exist or
/// is not a directory (requirement 2.4).
pub fn build(specs_dir: &Path) -> Vec<Spec> {
    if !specs_dir.is_dir() {
        return Vec::new();
    }

    let Ok(read_dir) = fs::read_dir(specs_dir) else {
        return Vec::new();
    };

    let mut feature_dirs: Vec<PathBuf> = read_dir
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();

    feature_dirs.sort_by(|a, b| feature_dir_name(a).cmp(&feature_dir_name(b)));

    feature_dirs.iter().map(|dir| build_feature(dir)).collect()
}

/// A feature directory's own name (last path component), used as both
/// `Spec.name` and the sort key (string order, per spec-kit's `NNN-이름`
/// convention -- see `build`'s docs).
fn feature_dir_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// List a directory's immediate file entries' names, sorted ascending.
/// Returns an empty `Vec` (never panics) if the directory cannot be read.
fn sorted_file_names(dir: &Path) -> Vec<String> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut names: Vec<String> = read_dir
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().to_str().map(|s| s.to_string()))
        .collect();
    names.sort();
    names
}

/// A plain existence-only `DocEntry` for one canonical (non-`tasks.md`) file.
fn plain_doc_entry(dir: &Path, filename: &str) -> DocEntry {
    DocEntry {
        kind: DocKind::Other(filename.to_string()),
        path: dir.join(filename),
        exists: true,
        status: DocStatus::NotTracked,
        progress: None,
    }
}

/// `tasks.md`'s `DocEntry`: the one exception to read-only existence checks
/// (requirement 6.1/6.2) -- its content is read to compute checkbox
/// progress. A read failure (e.g. permissions) is swallowed into
/// `progress: None` rather than panicking.
fn tasks_doc_entry(dir: &Path) -> DocEntry {
    let path = dir.join("tasks.md");
    let progress = fs::read_to_string(&path)
        .ok()
        .and_then(|content| count_progress(&content));
    DocEntry {
        kind: DocKind::Other("tasks.md".to_string()),
        path,
        exists: true,
        status: DocStatus::NotTracked,
        progress,
    }
}

/// Assemble a single spec-kit feature directory into a [`Spec`].
fn build_feature(dir: &Path) -> Spec {
    let mut docs = Vec::new();

    for filename in CANONICAL_SPEC_KIT_FILES {
        if !dir.join(filename).is_file() {
            continue;
        }
        docs.push(if filename == "tasks.md" {
            tasks_doc_entry(dir)
        } else {
            plain_doc_entry(dir, filename)
        });
    }

    let contracts_dir = dir.join("contracts");
    if contracts_dir.is_dir() {
        for filename in sorted_file_names(&contracts_dir) {
            docs.push(DocEntry {
                kind: DocKind::Other(format!("contracts/{filename}")),
                path: contracts_dir.join(&filename),
                exists: true,
                status: DocStatus::NotTracked,
                progress: None,
            });
        }
    }

    // Non-canonical top-level files (e.g. `notes.md`), appended after the
    // canonical order + contracts (requirement 2.3).
    for filename in sorted_file_names(dir) {
        if CANONICAL_SPEC_KIT_FILES.contains(&filename.as_str()) {
            continue;
        }
        docs.push(plain_doc_entry(dir, &filename));
    }

    let milestones = vec![
        Milestone {
            name: "명세".to_string(),
            done: dir.join("spec.md").is_file(),
        },
        Milestone {
            name: "설계".to_string(),
            done: dir.join("plan.md").is_file(),
        },
        Milestone {
            name: "작업 분해".to_string(),
            done: dir.join("tasks.md").is_file(),
        },
    ];

    Spec {
        name: feature_dir_name(dir),
        dir: dir.to_path_buf(),
        kiro_meta: None,
        milestones,
        warning: None,
        docs,
        definition: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 임시 디렉터리를 만들고, 테스트가 끝나면 자동으로 정리하는 헬퍼.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let mut path = std::env::temp_dir();
            let unique = format!(
                "spec_kit_test_{}_{}_{}",
                name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            path.push(unique);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn finds_root_from_deep_descendant_when_only_specify_exists() {
        let tmp = TempDir::new("only_specify");
        let root = tmp.path();
        fs::create_dir_all(root.join(".specify")).unwrap();
        let deep = root.join("some/deep/path");
        fs::create_dir_all(&deep).unwrap();

        assert_eq!(find_spec_kit_root(&deep), Some(root.to_path_buf()));
    }

    #[test]
    fn returns_none_when_kiro_and_specify_coexist_in_same_dir() {
        let tmp = TempDir::new("coexist");
        let root = tmp.path();
        fs::create_dir_all(root.join(".specify")).unwrap();
        fs::create_dir_all(root.join(".kiro")).unwrap();
        let deep = root.join("some/deep/path");
        fs::create_dir_all(&deep).unwrap();

        assert_eq!(find_spec_kit_root(root), None);
        assert_eq!(find_spec_kit_root(&deep), None);
    }

    #[test]
    fn returns_parent_when_start_itself_is_the_specify_dir() {
        let tmp = TempDir::new("start_is_specify");
        let root = tmp.path();
        let specify = root.join(".specify");
        fs::create_dir_all(&specify).unwrap();

        assert_eq!(find_spec_kit_root(&specify), Some(root.to_path_buf()));
    }

    #[test]
    fn returns_none_when_specify_is_nowhere_to_be_found() {
        let tmp = TempDir::new("no_specify");
        let deep = tmp.path().join("some/deep/path");
        fs::create_dir_all(&deep).unwrap();

        assert_eq!(find_spec_kit_root(&deep), None);
    }

    #[test]
    fn finds_root_across_many_ancestor_levels_without_kiro_present() {
        let tmp = TempDir::new("far_ancestor");
        let root = tmp.path();
        fs::create_dir_all(root.join(".specify")).unwrap();
        let deep = root.join("a/b/c/d/e/f/g");
        fs::create_dir_all(&deep).unwrap();

        assert_eq!(find_spec_kit_root(&deep), Some(root.to_path_buf()));
    }
}

#[cfg(test)]
mod build_tests {
    use super::*;
    use crate::spec::progress::Progress;
    use std::fs;

    /// Local copy of the `TempDir` RAII guard from `mod tests` above --
    /// kept private to this module per the task's boundary (no cross-module
    /// sharing needed for this small a test surface).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let mut path = std::env::temp_dir();
            let unique = format!(
                "spec_kit_build_test_{}_{}_{}",
                name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            path.push(unique);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn feature_with_spec_plan_tasks_has_canonical_order_and_all_milestones_done() {
        let tmp = TempDir::new("full_feature");
        let specs_dir = tmp.path().join("specs");
        let feature = specs_dir.join("001-login");
        write(&feature.join("spec.md"), "# spec\n");
        write(&feature.join("plan.md"), "# plan\n");
        write(&feature.join("tasks.md"), "- [x] a\n- [ ] b\n");

        let specs = build(&specs_dir);

        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(spec.name, "001-login");
        assert_eq!(spec.dir, feature);
        assert!(spec.kiro_meta.is_none());
        assert!(spec.warning.is_none());
        assert!(spec.definition.is_none());

        let kinds: Vec<&DocKind> = spec.docs.iter().map(|d| &d.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &DocKind::Other("spec.md".to_string()),
                &DocKind::Other("plan.md".to_string()),
                &DocKind::Other("tasks.md".to_string()),
            ]
        );

        assert_eq!(
            spec.milestones,
            vec![
                Milestone { name: "명세".to_string(), done: true },
                Milestone { name: "설계".to_string(), done: true },
                Milestone { name: "작업 분해".to_string(), done: true },
            ]
        );
    }

    #[test]
    fn feature_with_only_spec_md_has_only_first_milestone_done() {
        let tmp = TempDir::new("spec_only");
        let specs_dir = tmp.path().join("specs");
        let feature = specs_dir.join("002-only-spec");
        write(&feature.join("spec.md"), "# spec\n");

        let specs = build(&specs_dir);
        let spec = &specs[0];

        assert_eq!(
            spec.milestones,
            vec![
                Milestone { name: "명세".to_string(), done: true },
                Milestone { name: "설계".to_string(), done: false },
                Milestone { name: "작업 분해".to_string(), done: false },
            ]
        );

        let kinds: Vec<&DocKind> = spec.docs.iter().map(|d| &d.kind).collect();
        assert_eq!(kinds, vec![&DocKind::Other("spec.md".to_string())]);
    }

    #[test]
    fn tasks_md_with_checkboxes_computes_progress() {
        let tmp = TempDir::new("tasks_checkboxes");
        let specs_dir = tmp.path().join("specs");
        let feature = specs_dir.join("003-progress");
        write(&feature.join("tasks.md"), "- [x] a\n- [ ] b\n");

        let specs = build(&specs_dir);
        let spec = &specs[0];

        let tasks = spec
            .docs
            .iter()
            .find(|d| d.kind == DocKind::Other("tasks.md".to_string()))
            .expect("tasks.md doc entry present");

        assert_eq!(tasks.progress, Some(Progress { done: 1, total: 2 }));
    }

    #[test]
    fn tasks_md_without_checkboxes_has_no_progress() {
        let tmp = TempDir::new("tasks_no_checkboxes");
        let specs_dir = tmp.path().join("specs");
        let feature = specs_dir.join("004-no-checkboxes");
        write(&feature.join("tasks.md"), "just prose, no checkboxes here\n");

        let specs = build(&specs_dir);
        let spec = &specs[0];

        let tasks = spec
            .docs
            .iter()
            .find(|d| d.kind == DocKind::Other("tasks.md".to_string()))
            .expect("tasks.md doc entry present");

        assert_eq!(tasks.progress, None);
    }

    #[test]
    fn non_canonical_file_appears_after_canonical_order() {
        let tmp = TempDir::new("extra_file");
        let specs_dir = tmp.path().join("specs");
        let feature = specs_dir.join("005-extra");
        write(&feature.join("spec.md"), "# spec\n");
        write(&feature.join("tasks.md"), "- [ ] a\n");
        write(&feature.join("notes.md"), "some notes\n");

        let specs = build(&specs_dir);
        let spec = &specs[0];

        let kinds: Vec<&DocKind> = spec.docs.iter().map(|d| &d.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &DocKind::Other("spec.md".to_string()),
                &DocKind::Other("tasks.md".to_string()),
                &DocKind::Other("notes.md".to_string()),
            ]
        );
    }

    #[test]
    fn contracts_dir_files_appear_in_ascending_order() {
        let tmp = TempDir::new("contracts");
        let specs_dir = tmp.path().join("specs");
        let feature = specs_dir.join("006-contracts");
        write(&feature.join("spec.md"), "# spec\n");
        write(&feature.join("contracts").join("z-last.yaml"), "z\n");
        write(&feature.join("contracts").join("a-first.yaml"), "a\n");

        let specs = build(&specs_dir);
        let spec = &specs[0];

        let kinds: Vec<&DocKind> = spec.docs.iter().map(|d| &d.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &DocKind::Other("spec.md".to_string()),
                &DocKind::Other("contracts/a-first.yaml".to_string()),
                &DocKind::Other("contracts/z-last.yaml".to_string()),
            ]
        );
    }

    #[test]
    fn missing_specs_dir_returns_empty_vec_without_panicking() {
        let tmp = TempDir::new("missing_specs_dir");
        let specs_dir = tmp.path().join("does-not-exist");

        let specs = build(&specs_dir);

        assert!(specs.is_empty());
    }

    #[test]
    fn empty_feature_directory_has_no_docs_and_no_panic() {
        let tmp = TempDir::new("empty_feature");
        let specs_dir = tmp.path().join("specs");
        fs::create_dir_all(specs_dir.join("007-empty")).unwrap();

        let specs = build(&specs_dir);
        let spec = &specs[0];

        assert_eq!(spec.name, "007-empty");
        assert!(spec.docs.is_empty());
        assert_eq!(
            spec.milestones,
            vec![
                Milestone { name: "명세".to_string(), done: false },
                Milestone { name: "설계".to_string(), done: false },
                Milestone { name: "작업 분해".to_string(), done: false },
            ]
        );
    }

    #[test]
    fn two_features_with_different_progress_are_each_computed_independently() {
        let tmp = TempDir::new("two_features");
        let specs_dir = tmp.path().join("specs");
        let far_along = specs_dir.join("001-far-along");
        write(&far_along.join("spec.md"), "# spec\n");
        write(&far_along.join("plan.md"), "# plan\n");
        write(&far_along.join("tasks.md"), "- [x] a\n- [x] b\n- [ ] c\n");

        let just_started = specs_dir.join("002-just-started");
        write(&just_started.join("spec.md"), "# spec\n");

        let specs = build(&specs_dir);

        assert_eq!(specs.len(), 2);
        // Ascending directory-name order.
        assert_eq!(specs[0].name, "001-far-along");
        assert_eq!(specs[1].name, "002-just-started");

        assert!(specs[0].milestones.iter().all(|m| m.name != "설계" || m.done));
        assert_eq!(
            specs[0].milestones,
            vec![
                Milestone { name: "명세".to_string(), done: true },
                Milestone { name: "설계".to_string(), done: true },
                Milestone { name: "작업 분해".to_string(), done: true },
            ]
        );
        let far_tasks = specs[0]
            .docs
            .iter()
            .find(|d| d.kind == DocKind::Other("tasks.md".to_string()))
            .unwrap();
        assert_eq!(far_tasks.progress, Some(Progress { done: 2, total: 3 }));

        assert_eq!(
            specs[1].milestones,
            vec![
                Milestone { name: "명세".to_string(), done: true },
                Milestone { name: "설계".to_string(), done: false },
                Milestone { name: "작업 분해".to_string(), done: false },
            ]
        );
    }
}
