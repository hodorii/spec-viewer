pub mod fs_tree;
pub mod meta;
pub mod progress;
pub mod sort;

pub use fs_tree::{FsEntry, FsTree};
pub use meta::{parse_meta, Approval, DocKind, MetaError, SpecMeta};
pub use progress::{count_progress, Progress};
pub use sort::{sort_specs, SortKey};

use std::path::{Path, PathBuf};

/// Locate the `.kiro` root directory starting from `start`.
///
/// Resolution order:
/// 1. `start/.kiro` if it exists and is a directory.
/// 2. `start` itself, if its file name is exactly `.kiro` and it exists as a directory.
/// 3. Walking upward through `start`'s ancestors (parent, grandparent, ...), the first
///    ancestor whose `.kiro` child exists and is a directory.
/// 4. `None` if none of the above match.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let child = start.join(".kiro");
    if child.is_dir() {
        return Some(child);
    }

    if start.file_name().map(|n| n == ".kiro").unwrap_or(false) && start.is_dir() {
        return Some(start.to_path_buf());
    }

    for ancestor in start.ancestors().skip(1) {
        let candidate = ancestor.join(".kiro");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }

    None
}

/// In-memory snapshot of a directory tree under `.kiro/`, sufficient for
/// [`build`] to assemble a [`SpecRoot`] without touching the filesystem
/// itself.
///
/// Placement note: design.md's File Structure Plan originally assigned this
/// type to `app/loader.rs`, but `build()`'s signature needs it here in
/// `spec`, and `spec` must never depend on `app`. Resolved (task 5.1):
/// `app::loader` re-exports this type and its real disk-reading
/// `load_snapshot()` constructs one and hands it to `build()`.
pub struct DirSnapshot {
    pub specs: Vec<SpecDirSnapshot>,
    pub steering: Vec<FileSnapshot>,
}

/// One spec directory's flat file listing (no recursion — spec directories
/// are flat in practice).
pub struct SpecDirSnapshot {
    pub name: String,
    pub dir: PathBuf,
    pub files: Vec<FileSnapshot>,
}

/// A single file's name, path, and content as read from disk (or, in tests,
/// constructed in memory).
pub struct FileSnapshot {
    pub name: String,
    pub path: PathBuf,
    pub content: String,
}

/// The fully assembled domain model: every spec under `.kiro/specs/` plus
/// every steering doc under `.kiro/steering/`.
pub struct SpecRoot {
    pub specs: Vec<Spec>,
    pub steering: Vec<SteeringDoc>,
}

/// A single steering document: its display name, path, and parsed
/// `inclusion` front-matter value (requirement 2.8).
pub struct SteeringDoc {
    pub name: String,
    pub path: PathBuf,
    pub inclusion: Inclusion,
}

/// Parsed `inclusion` front-matter value of a steering doc (requirement 2.8).
/// Defaults to `Always` when absent or unrecognized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inclusion {
    Always,
    Manual,
    FileMatch,
    Auto,
}

/// One spec directory's assembled domain model.
pub struct Spec {
    pub name: String,
    pub dir: PathBuf,
    pub meta: Result<SpecMeta, MetaError>,
    pub docs: Vec<DocEntry>,
    pub definition: Option<String>,
}

/// One document slot within a spec's canonical document order (requirement
/// 2.2).
pub struct DocEntry {
    pub kind: DocKind,
    pub path: PathBuf,
    pub exists: bool,
    pub status: DocStatus,
    pub progress: Option<Progress>,
}

/// Approval/tracking status of a single document node (requirements 3.2,
/// 3.4). `NotTracked` covers `research` and `Other` documents, which never
/// carry an approval badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocStatus {
    Missing,
    Generated,
    Approved,
    NoRecord,
    NotTracked,
}

/// Stable identifier for a tree node, used by the (later) `app`/`ui` layers
/// to track selection/expansion across rebuilds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeId {
    Spec(String),
    Doc(String, DocKind),
    SteeringGroup,
    Steering(String),
    /// `--all` mode only (requirement 1.8): a folder, fold/unfold-only.
    Dir(PathBuf),
    /// `--all` mode only (requirement 1.8): a markdown file, loads on select.
    File(PathBuf),
}

/// What a tree panel/reducer/watcher is actually browsing (requirement
/// 1.8): the existing `.kiro`-schema-aware `SpecRoot`, or a plain markdown
/// directory tree for `--all` mode. Everything above `spec` (app's reducer,
/// ui's tree_panel) depends on this enum only, never on `SpecRoot` or
/// `FsTree` directly, so the two modes can never quietly diverge.
/// Badges/progress/`## 정의` (3.x, 4.x) only ever come from `Kiro` --
/// `Files` has no equivalent concept (requirement 1.9).
pub enum TreeSource {
    Kiro(SpecRoot),
    Files(FsTree),
}

impl TreeSource {
    /// `Some` only for `Kiro` -- a small accessor for the (still common)
    /// call sites that only make sense in `.kiro` mode, e.g. `Definition`
    /// lookups by spec name.
    pub fn as_kiro(&self) -> Option<&SpecRoot> {
        match self {
            TreeSource::Kiro(root) => Some(root),
            TreeSource::Files(_) => None,
        }
    }
}

/// Standard filenames handled by the canonical per-slot rules; anything else
/// in a spec directory becomes a `DocKind::Other` entry.
const STANDARD_SPEC_FILES: [&str; 7] = [
    "spec.json",
    "requirements.md",
    "bugfix.md",
    "biz-process.md",
    "design.md",
    "tasks.md",
    "research.md",
];

/// Look up a file by exact name within a spec dir's (unsorted) file list.
fn find_file<'a>(files: &'a [FileSnapshot], name: &str) -> Option<&'a FileSnapshot> {
    files.iter().find(|f| f.name == name)
}

/// Assemble a [`SpecRoot`] from an in-memory directory [`DirSnapshot`].
///
/// Pure aggregation only: no filesystem I/O here, and caller-given ordering
/// of `snapshot.specs` / `snapshot.steering` / each spec's `files` is
/// preserved (listing-order policy belongs to the real loader, not to this
/// function) — except for the *output* `Spec.docs` ordering, which this
/// function does impose (requirement 2.2, 2.3).
pub fn build(snapshot: &DirSnapshot) -> SpecRoot {
    let specs = snapshot.specs.iter().map(build_spec).collect();
    let steering = snapshot
        .steering
        .iter()
        .map(|f| SteeringDoc {
            name: f.name.clone(),
            path: f.path.clone(),
            inclusion: inclusion(&f.content),
        })
        .collect();

    SpecRoot { specs, steering }
}

fn build_spec(sd: &SpecDirSnapshot) -> Spec {
    let dir = &sd.dir;

    let meta = match find_file(&sd.files, "spec.json") {
        Some(f) => parse_meta(&f.content),
        None => Err(MetaError::InvalidJson("spec.json not found".to_string())),
    };

    let mut docs = Vec::new();

    // Slot 1: Requirements or Bugfix (mutually exclusive).
    let bugfix_file = find_file(&sd.files, "bugfix.md");
    let has_bugfix_approval =
        matches!(&meta, Ok(m) if m.approvals.contains_key(&DocKind::Bugfix));
    let use_bugfix = bugfix_file.is_some() || has_bugfix_approval;

    let (first_kind, first_filename) = if use_bugfix {
        (DocKind::Bugfix, "bugfix.md")
    } else {
        (DocKind::Requirements, "requirements.md")
    };
    let first_file = find_file(&sd.files, first_filename);
    let first_status = doc_status(&meta, &first_kind);
    docs.push(DocEntry {
        kind: first_kind,
        path: dir.join(first_filename),
        exists: first_file.is_some(),
        status: first_status,
        progress: None,
    });

    let definition = first_file.and_then(|f| definition(&f.content));

    // Slot 2: BizProcess (always emitted).
    let biz_process_file = find_file(&sd.files, "biz-process.md");
    let biz_process_status = doc_status(&meta, &DocKind::BizProcess);
    docs.push(DocEntry {
        kind: DocKind::BizProcess,
        path: dir.join("biz-process.md"),
        exists: biz_process_file.is_some(),
        status: biz_process_status,
        progress: None,
    });

    // Slot 3: Design (always emitted).
    let design_file = find_file(&sd.files, "design.md");
    let design_status = doc_status(&meta, &DocKind::Design);
    docs.push(DocEntry {
        kind: DocKind::Design,
        path: dir.join("design.md"),
        exists: design_file.is_some(),
        status: design_status,
        progress: None,
    });

    // Slot 4: Tasks (always emitted; carries the only real `progress`).
    let tasks_file = find_file(&sd.files, "tasks.md");
    let tasks_status = doc_status(&meta, &DocKind::Tasks);
    docs.push(DocEntry {
        kind: DocKind::Tasks,
        path: dir.join("tasks.md"),
        exists: tasks_file.is_some(),
        status: tasks_status,
        progress: tasks_file.and_then(|f| count_progress(&f.content)),
    });

    // Slot 5: Research — only emitted if research.md actually exists.
    if find_file(&sd.files, "research.md").is_some() {
        docs.push(DocEntry {
            kind: DocKind::Research,
            path: dir.join("research.md"),
            exists: true,
            status: DocStatus::NotTracked,
            progress: None,
        });
    }

    // Remaining files: DocKind::Other, sorted by filename ascending.
    let mut others: Vec<&FileSnapshot> = sd
        .files
        .iter()
        .filter(|f| !STANDARD_SPEC_FILES.contains(&f.name.as_str()))
        .collect();
    others.sort_by(|a, b| a.name.cmp(&b.name));
    for f in others {
        docs.push(DocEntry {
            kind: DocKind::Other(f.name.clone()),
            path: dir.join(&f.name),
            exists: true,
            status: DocStatus::NotTracked,
            progress: None,
        });
    }

    Spec {
        name: sd.name.clone(),
        dir: sd.dir.clone(),
        meta,
        docs,
        definition,
    }
}

/// Approval-derived status for one of the four `approvals`-tracked slots
/// (Requirements/Bugfix/BizProcess/Design/Tasks).
fn doc_status(meta: &Result<SpecMeta, MetaError>, kind: &DocKind) -> DocStatus {
    match meta {
        Err(_) => DocStatus::NoRecord,
        Ok(m) => match m.approvals.get(kind) {
            None => DocStatus::NoRecord,
            Some(a) => {
                if a.approved {
                    DocStatus::Approved
                } else if a.generated {
                    DocStatus::Generated
                } else {
                    DocStatus::Missing
                }
            }
        },
    }
}

/// Extract the body of a doc's `## 정의` section (requirement 3.7).
///
/// Looks for a line that, after trimming, is exactly `"## 정의"`. Everything
/// from the next line up to (but not including) the next h2 heading (a
/// trimmed line starting with `"## "`) or end-of-input is collected, with
/// leading/trailing blank lines trimmed off. Returns `None` if the heading
/// is absent, or if the collected body is empty/whitespace-only.
pub fn definition(doc_md: &str) -> Option<String> {
    let lines: Vec<&str> = doc_md.lines().collect();
    let start = lines.iter().position(|l| l.trim() == "## 정의")?;

    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        if line.trim_start().starts_with("## ") {
            end = i;
            break;
        }
    }

    let body = &lines[start + 1..end];
    let mut s = 0;
    let mut e = body.len();
    while s < e && body[s].trim().is_empty() {
        s += 1;
    }
    while e > s && body[e - 1].trim().is_empty() {
        e -= 1;
    }

    if s == e {
        None
    } else {
        Some(body[s..e].join("\n"))
    }
}

/// Parse a steering file's `inclusion` front-matter value (requirement 2.8).
///
/// Uses the same `---`-delimiter convention as `markdown::strip_frontmatter`
/// (must start with `"---\n"`). Defaults to `Inclusion::Always` when there is
/// no front matter, no `inclusion` key inside it, or an unrecognized value.
pub fn inclusion(steering_md: &str) -> Inclusion {
    if let Some(front) = front_matter_block(steering_md) {
        for line in front.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("inclusion") else {
                continue;
            };
            let rest = rest.trim_start();
            let Some(value) = rest.strip_prefix(':') else {
                continue;
            };
            let value = value.trim();
            let value = value.trim_matches('"').trim_matches('\'');
            return match value {
                "always" => Inclusion::Always,
                "manual" => Inclusion::Manual,
                "fileMatch" => Inclusion::FileMatch,
                "auto" => Inclusion::Auto,
                _ => Inclusion::Always,
            };
        }
    }

    Inclusion::Always
}

/// Return the content strictly between a leading `"---\n"` delimiter and the
/// next `"\n---"` delimiter, or `None` if there is no such front-matter
/// block (same delimiter convention as `markdown::strip_frontmatter`).
fn front_matter_block(src: &str) -> Option<&str> {
    if !src.starts_with("---\n") {
        return None;
    }
    let body = &src[4..];
    let pos = body.find("\n---")?;
    Some(&body[..pos])
}

#[cfg(test)]
mod tests {
    use super::find_root;
    use std::fs;
    use std::path::PathBuf;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("spec_viewer_find_root_{}", name));
        // Clean up any leftovers from a previous failed run.
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn ancestor_search_finds_kiro_above_nested_dir() {
        let root = scratch_dir("ancestor_search");
        fs::create_dir_all(root.join(".kiro")).unwrap();
        fs::create_dir_all(root.join("a/b")).unwrap();

        let found = find_root(&root.join("a/b"));

        assert_eq!(found, Some(root.join(".kiro")));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn explicit_path_with_kiro_child_returns_child() {
        let root = scratch_dir("explicit_child");
        fs::create_dir_all(root.join(".kiro")).unwrap();

        let found = find_root(&root);

        assert_eq!(found, Some(root.join(".kiro")));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn explicit_path_is_kiro_dir_itself() {
        let root = scratch_dir("explicit_is_kiro");
        let kiro = root.join(".kiro");
        fs::create_dir_all(&kiro).unwrap();

        let found = find_root(&kiro);

        assert_eq!(found, Some(kiro.clone()));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn no_kiro_anywhere_returns_none() {
        let root = scratch_dir("none_found");
        fs::create_dir_all(root.join("x/y")).unwrap();

        let found = find_root(&root.join("x/y"));

        // NOTE: this asserts there is no `.kiro` anywhere from `root/x/y` up through
        // this constructed subtree's ancestors. It cannot guarantee the real filesystem
        // ancestors above `std::env::temp_dir()` (e.g. `/tmp`, `/`) are also free of a
        // stray `.kiro` directory — see CONCERNS in the task report.
        assert_eq!(found, None);

        fs::remove_dir_all(&root).unwrap();
    }
}

#[cfg(test)]
mod build_tests {
    use super::*;
    use std::fs;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kiro"))
    }

    fn read_file_snapshot(path: &Path) -> FileSnapshot {
        let name = path
            .file_name()
            .expect("file has a name")
            .to_string_lossy()
            .to_string();
        let content = fs::read_to_string(path).expect("fixture file is readable UTF-8");
        FileSnapshot {
            name,
            path: path.to_path_buf(),
            content,
        }
    }

    fn spec_snapshot(name: &str) -> SpecDirSnapshot {
        let dir = fixtures_root().join("specs").join(name);
        let mut files = Vec::new();
        for entry in fs::read_dir(&dir).expect("spec fixture dir exists") {
            let entry = entry.expect("dir entry readable");
            if entry.file_type().expect("file type readable").is_file() {
                files.push(read_file_snapshot(&entry.path()));
            }
        }
        SpecDirSnapshot {
            name: name.to_string(),
            dir,
            files,
        }
    }

    /// Build a `DirSnapshot` from real fixtures on disk. `specs` names the
    /// `tests/fixtures/kiro/specs/<name>` directories to include (in the
    /// given order); when `steering` is true, all three steering fixtures
    /// (`product.md`, `tech.md`, `domain-terms.md`) are included.
    fn snapshot_of(specs: &[&str], steering: bool) -> DirSnapshot {
        let specs = specs.iter().map(|s| spec_snapshot(s)).collect();
        let steering = if steering {
            let dir = fixtures_root().join("steering");
            ["product.md", "tech.md", "domain-terms.md"]
                .iter()
                .map(|name| read_file_snapshot(&dir.join(name)))
                .collect()
        } else {
            Vec::new()
        };
        DirSnapshot { specs, steering }
    }

    fn doc<'a>(spec: &'a Spec, kind: &DocKind) -> &'a DocEntry {
        spec.docs
            .iter()
            .find(|d| &d.kind == kind)
            .unwrap_or_else(|| panic!("expected a doc entry for {kind:?}"))
    }

    #[test]
    fn sample_signup_standard_order_and_definition_and_progress() {
        let snapshot = snapshot_of(&["sample-signup"], false);
        let root = build(&snapshot);

        assert_eq!(root.specs.len(), 1);
        let spec = &root.specs[0];
        assert_eq!(spec.name, "sample-signup");

        // No research.md and no extra files in this fixture, so the
        // canonical order is exactly Requirements -> BizProcess -> Design ->
        // Tasks (no Research slot, since research.md does not exist here).
        let kinds: Vec<&DocKind> = spec.docs.iter().map(|d| &d.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &DocKind::Requirements,
                &DocKind::BizProcess,
                &DocKind::Design,
                &DocKind::Tasks,
            ]
        );

        let definition = spec.definition.as_ref().expect("definition present");
        assert!(definition.contains("회원가입 절차를 검증하는 요구사항 스펙이다"));

        let tasks = doc(spec, &DocKind::Tasks);
        assert_eq!(tasks.progress, Some(Progress { done: 3, total: 5 }));
        assert!(tasks.exists);
        // No "tasks" key in sample-signup's spec.json approvals.
        assert_eq!(tasks.status, DocStatus::NoRecord);

        let requirements = doc(spec, &DocKind::Requirements);
        assert!(requirements.exists);
        assert_eq!(requirements.status, DocStatus::Approved);

        let biz_process = doc(spec, &DocKind::BizProcess);
        assert!(biz_process.exists);
        assert_eq!(biz_process.status, DocStatus::Generated);

        let design = doc(spec, &DocKind::Design);
        assert!(design.exists);
        assert_eq!(design.status, DocStatus::Generated);
    }

    #[test]
    fn bugfix_only_uses_bugfix_slot_with_missing_siblings() {
        let snapshot = snapshot_of(&["bugfix-only"], false);
        let root = build(&snapshot);
        let spec = &root.specs[0];

        let first = &spec.docs[0];
        assert_eq!(first.kind, DocKind::Bugfix);
        assert!(first.exists);
        assert_eq!(first.status, DocStatus::Approved);

        // bugfix.md also has a `## 정의` section.
        let definition = spec.definition.as_ref().expect("definition present");
        assert!(definition.contains("버그픽스 스펙이다"));

        let biz_process = doc(spec, &DocKind::BizProcess);
        assert!(!biz_process.exists);
        assert_eq!(biz_process.status, DocStatus::NoRecord);

        let design = doc(spec, &DocKind::Design);
        assert!(!design.exists);
        // spec.json records design as generated but not approved, even
        // though the file itself does not exist in this fixture — exists
        // and status are allowed to disagree.
        assert_eq!(design.status, DocStatus::Generated);

        let tasks = doc(spec, &DocKind::Tasks);
        assert!(!tasks.exists);
        assert_eq!(tasks.status, DocStatus::Missing);
        assert_eq!(tasks.progress, None);
    }

    #[test]
    fn with_extra_other_file_sorts_after_research() {
        let snapshot = snapshot_of(&["with-extra"], false);
        let root = build(&snapshot);
        let spec = &root.specs[0];

        let kinds: Vec<&DocKind> = spec.docs.iter().map(|d| &d.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &DocKind::Requirements,
                &DocKind::BizProcess,
                &DocKind::Design,
                &DocKind::Tasks,
                &DocKind::Research,
                &DocKind::Other("notes.md".to_string()),
            ]
        );

        let biz_process = doc(spec, &DocKind::BizProcess);
        assert!(!biz_process.exists);

        let research = doc(spec, &DocKind::Research);
        assert!(research.exists);
        assert_eq!(research.status, DocStatus::NotTracked);

        let other = doc(spec, &DocKind::Other("notes.md".to_string()));
        assert!(other.exists);
        assert_eq!(other.status, DocStatus::NotTracked);
        assert_eq!(other.progress, None);
    }

    #[test]
    fn no_approvals_yields_no_record_not_missing() {
        let snapshot = snapshot_of(&["no-approvals"], false);
        let root = build(&snapshot);
        let spec = &root.specs[0];

        assert!(spec.meta.is_ok());

        let requirements = doc(spec, &DocKind::Requirements);
        assert!(requirements.exists);
        assert_eq!(requirements.status, DocStatus::NoRecord);

        let design = doc(spec, &DocKind::Design);
        assert!(!design.exists);
        assert_eq!(design.status, DocStatus::NoRecord);

        let tasks = doc(spec, &DocKind::Tasks);
        assert!(!tasks.exists);
        assert_eq!(tasks.status, DocStatus::NoRecord);
        assert_eq!(tasks.progress, None);

        let biz_process = doc(spec, &DocKind::BizProcess);
        assert!(!biz_process.exists);
        assert_eq!(biz_process.status, DocStatus::NoRecord);
    }

    #[test]
    fn broken_json_yields_no_record_but_docs_still_shown() {
        let snapshot = snapshot_of(&["broken-json"], false);
        let root = build(&snapshot);
        let spec = &root.specs[0];

        assert!(spec.meta.is_err());

        let requirements = doc(spec, &DocKind::Requirements);
        assert!(requirements.exists);
        assert_eq!(requirements.path, spec.dir.join("requirements.md"));
        assert_eq!(requirements.status, DocStatus::NoRecord);

        let design = doc(spec, &DocKind::Design);
        assert!(!design.exists);
        assert_eq!(design.path, spec.dir.join("design.md"));
        assert_eq!(design.status, DocStatus::NoRecord);

        let tasks = doc(spec, &DocKind::Tasks);
        assert!(!tasks.exists);
        assert_eq!(tasks.status, DocStatus::NoRecord);

        let biz_process = doc(spec, &DocKind::BizProcess);
        assert!(!biz_process.exists);
        assert_eq!(biz_process.status, DocStatus::NoRecord);
    }

    #[test]
    fn sample_billing_has_no_definition() {
        let snapshot = snapshot_of(&["sample-billing"], false);
        let root = build(&snapshot);
        let spec = &root.specs[0];

        assert_eq!(spec.definition, None);
    }

    #[test]
    fn steering_docs_get_expected_inclusion_values() {
        let snapshot = snapshot_of(&[], true);
        let root = build(&snapshot);

        assert_eq!(root.specs.len(), 0);
        assert_eq!(root.steering.len(), 3);

        let find = |name: &str| {
            root.steering
                .iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("expected steering doc {name}"))
        };

        assert_eq!(find("product.md").inclusion, Inclusion::Always);
        assert_eq!(find("tech.md").inclusion, Inclusion::Always);
        assert_eq!(find("domain-terms.md").inclusion, Inclusion::Manual);
    }

    #[test]
    fn definition_stops_at_next_h2_and_trims_blank_lines() {
        let doc_md = "\
# Title

## 정의

Line one.
Line two.

## Boundary Context

Should not be included.
";

        let result = definition(doc_md).expect("definition present");
        assert_eq!(result, "Line one.\nLine two.");
    }

    #[test]
    fn inclusion_with_no_front_matter_defaults_to_always() {
        let steering_md = "# Product\nNo front matter here.\n";
        assert_eq!(inclusion(steering_md), Inclusion::Always);
    }
}
