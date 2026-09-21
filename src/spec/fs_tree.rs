//! `--all` mode's tree source (requirement 1.8): a plain directory tree of
//! every markdown file under some root, with no `.kiro`/spec schema at all.
//! Reference: `~/dev/tools/mdview/src/source.rs`'s `find_markdown_files`
//! (walk options only — that function returns a flat file list, not a
//! tree, so the directory-pruning/depth-tagging below is this module's
//! own).
//!
//! Unlike [`crate::spec::build`], this reads the real filesystem directly
//! (`ignore::WalkBuilder` has no injectable-snapshot equivalent) --
//! [`FsTree::scan`]'s own tests use real temporary directories, the same
//! pattern `watch`'s tests already use for this crate's other I/O
//! boundaries.

use std::path::{Path, PathBuf};

/// One directory or markdown-file entry in an [`FsTree`], already filtered
/// and pruned by [`FsTree::scan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    /// Distance below `root` in path components (a direct child of `root`
    /// is depth 1), matching `ignore::WalkBuilder::max_depth`'s own units.
    pub depth: u8,
}

/// A `--all`-mode directory tree: `root` plus every markdown file beneath
/// it (up to depth 6, `.gitignore`/hidden-file rules respected) and the
/// directories needed to reach them. `entries` is sorted by full path,
/// which — since a directory's path is always a strict prefix of its own
/// descendants' paths — already puts every directory immediately before
/// its children and groups each subtree together, in name order at every
/// level. That is the order the tree_panel needs to rebuild a nested
/// widget tree from this flat list via `depth` alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsTree {
    pub root: PathBuf,
    pub entries: Vec<FsEntry>,
}

const MAX_DEPTH: usize = 6;
const MD_EXTS: &[&str] = &["md", "markdown"];

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| MD_EXTS.iter().any(|m| m.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

impl FsTree {
    /// Scan `root` for markdown files (requirement 1.8): hidden
    /// files/directories and anything `.gitignore`-excluded are skipped,
    /// depth is capped at 6, and only `.md`/`.markdown` files are kept as
    /// leaves. A directory is included only if at least one markdown file
    /// exists somewhere beneath it — an otherwise-empty branch would be a
    /// dead end with nothing to select.
    pub fn scan(root: &Path) -> FsTree {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();

        let walker = ignore::WalkBuilder::new(root)
            .hidden(true)
            .max_depth(Some(MAX_DEPTH))
            .build();

        for result in walker {
            let Ok(entry) = result else { continue };
            let path = entry.path();
            if path == root {
                continue;
            }
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                dirs.push(path.to_path_buf());
            } else if is_markdown(path) {
                files.push(path.to_path_buf());
            }
        }

        let kept_dirs: Vec<PathBuf> = dirs
            .into_iter()
            .filter(|d| files.iter().any(|f| f.starts_with(d)))
            .collect();

        let depth_of = |path: &Path| -> u8 {
            path.strip_prefix(root)
                .map(|rel| rel.components().count() as u8)
                .unwrap_or(1)
        };

        let mut entries: Vec<FsEntry> = kept_dirs
            .into_iter()
            .map(|path| {
                let depth = depth_of(&path);
                FsEntry { path, is_dir: true, depth }
            })
            .chain(files.into_iter().map(|path| {
                let depth = depth_of(&path);
                FsEntry { path, is_dir: false, depth }
            }))
            .collect();

        entries.sort_by(|a, b| a.path.cmp(&b.path));

        FsTree { root: root.to_path_buf(), entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("spec_viewer_fs_tree_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_markdown_files_and_their_ancestor_directories() {
        let root = scratch_dir("basic");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.md"), "x").unwrap();
        fs::write(root.join("sub/b.markdown"), "x").unwrap();
        fs::write(root.join("not-markdown.txt"), "x").unwrap();

        let tree = FsTree::scan(&root);

        let paths: Vec<&PathBuf> = tree.entries.iter().map(|e| &e.path).collect();
        assert!(paths.contains(&&root.join("a.md")));
        assert!(paths.contains(&&root.join("sub")));
        assert!(paths.contains(&&root.join("sub/b.markdown")));
        assert!(
            !paths.contains(&&root.join("not-markdown.txt")),
            "non-markdown files must be excluded"
        );

        let a_md = tree.entries.iter().find(|e| e.path == root.join("a.md")).unwrap();
        assert!(!a_md.is_dir);
        assert_eq!(a_md.depth, 1);

        let sub = tree.entries.iter().find(|e| e.path == root.join("sub")).unwrap();
        assert!(sub.is_dir);
        assert_eq!(sub.depth, 1);

        let b_md = tree.entries.iter().find(|e| e.path == root.join("sub/b.markdown")).unwrap();
        assert!(!b_md.is_dir);
        assert_eq!(b_md.depth, 2);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prunes_directories_with_no_markdown_descendant() {
        let root = scratch_dir("prune");
        fs::create_dir_all(root.join("empty-of-markdown")).unwrap();
        fs::write(root.join("empty-of-markdown/notes.txt"), "x").unwrap();
        fs::write(root.join("keep.md"), "x").unwrap();

        let tree = FsTree::scan(&root);

        assert!(
            !tree.entries.iter().any(|e| e.path == root.join("empty-of-markdown")),
            "a directory with no markdown descendant must be pruned, got: {:?}",
            tree.entries
        );
        assert!(tree.entries.iter().any(|e| e.path == root.join("keep.md")));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn excludes_hidden_files_and_directories() {
        let root = scratch_dir("hidden");
        fs::create_dir_all(root.join(".hidden-dir")).unwrap();
        fs::write(root.join(".hidden-dir/secret.md"), "x").unwrap();
        fs::write(root.join(".hidden-file.md"), "x").unwrap();
        fs::write(root.join("visible.md"), "x").unwrap();

        let tree = FsTree::scan(&root);
        let paths: Vec<&PathBuf> = tree.entries.iter().map(|e| &e.path).collect();

        assert!(paths.contains(&&root.join("visible.md")));
        assert!(!paths.iter().any(|p| p.starts_with(root.join(".hidden-dir"))));
        assert!(!paths.contains(&&root.join(".hidden-file.md")));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn respects_gitignore() {
        let root = scratch_dir("gitignore");
        // `ignore::WalkBuilder` only honors .gitignore inside a real git
        // repository (or with `.require_git(false)`, which isn't set here,
        // matching design.md's plain "ignore 크레이트" default) -- so this
        // scratch dir must actually be a git repo for the rule to apply.
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "ignored.md\n").unwrap();
        fs::write(root.join("ignored.md"), "x").unwrap();
        fs::write(root.join("kept.md"), "x").unwrap();

        let tree = FsTree::scan(&root);
        let paths: Vec<&PathBuf> = tree.entries.iter().map(|e| &e.path).collect();

        assert!(paths.contains(&&root.join("kept.md")));
        assert!(!paths.contains(&&root.join("ignored.md")));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn caps_at_max_depth_six() {
        let root = scratch_dir("deep");
        // `ignore::WalkBuilder` (built on `walkdir`) counts `root` itself as
        // depth 0, so a file needs 5 nested directories (d1..d5, depths
        // 1..5) above it to sit at depth 6 exactly, and 6 (d1..d6) to sit
        // one past the limit at depth 7.
        let mut dir = root.clone();
        for i in 1..=5 {
            dir = dir.join(format!("d{i}"));
            fs::create_dir_all(&dir).unwrap();
        }
        fs::write(dir.join("shallow-enough.md"), "x").unwrap(); // depth 6

        let too_deep_dir = dir.join("d6");
        fs::create_dir_all(&too_deep_dir).unwrap();
        fs::write(too_deep_dir.join("too-deep.md"), "x").unwrap(); // depth 7

        let tree = FsTree::scan(&root);
        let paths: Vec<&PathBuf> = tree.entries.iter().map(|e| &e.path).collect();

        assert!(paths.iter().any(|p| p.ends_with("shallow-enough.md")));
        assert!(
            !paths.iter().any(|p| p.ends_with("too-deep.md")),
            "a file past max_depth 6 must not appear, got: {tree:?}"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn entries_are_sorted_so_each_directorys_subtree_stays_grouped() {
        let root = scratch_dir("sort_order");
        fs::create_dir_all(root.join("alpha")).unwrap();
        fs::write(root.join("alpha/one.md"), "x").unwrap();
        fs::write(root.join("zeta.md"), "x").unwrap();
        fs::write(root.join("beta.md"), "x").unwrap();

        let tree = FsTree::scan(&root);
        let names: Vec<String> = tree
            .entries
            .iter()
            .map(|e| e.path.strip_prefix(&root).unwrap().to_string_lossy().into_owned())
            .collect();

        // "alpha" (and everything under it) sorts before "beta.md" and
        // "zeta.md" purely because "alpha" < "beta.md" < "zeta.md" as path
        // components -- and "alpha/one.md" is grouped immediately after
        // "alpha" itself, not scattered.
        assert_eq!(names, vec!["alpha", "alpha/one.md", "beta.md", "zeta.md"]);

        fs::remove_dir_all(&root).ok();
    }
}
