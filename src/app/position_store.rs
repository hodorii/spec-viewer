//! Tree cursor/fold positions kept between runs, so reopening spec-viewer
//! over the same project lands where the last run left off.
//!
//! Stored per user under the XDG state directory, never inside the project:
//! the positions are personal and machine-local, so they must not show up
//! in a repository's tracked or untracked files. Every read problem (no
//! file, unreadable, unparsable, wrong version) yields an empty store, and
//! write problems are returned for the caller to ignore -- remembering a
//! cursor is never worth interrupting the viewer.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{RememberedTreePosition, TreePositionKey};
use crate::spec::{NodeId, TreeSourceKind};

const STORE_VERSION: u32 = 1;
const STATE_DIR_NAME: &str = "spec-viewer";
const STATE_FILE_NAME: &str = "tree-positions.json";

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    positions: Vec<PositionRecord>,
}

#[derive(Serialize, Deserialize)]
struct PositionRecord {
    kind: TreeSourceKind,
    root: PathBuf,
    /// Absent in files written before scan roots were part of the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scan_root: Option<PathBuf>,
    selected: Vec<NodeId>,
    opened: Vec<Vec<NodeId>>,
}

/// `$XDG_STATE_HOME/spec-viewer/tree-positions.json`, or under
/// `$HOME/.local/state` when `XDG_STATE_HOME` is unset or relative (the XDG
/// spec says relative values must be ignored). `None` when neither is
/// usable.
pub fn default_state_file() -> Option<PathBuf> {
    state_file_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

fn state_file_from(xdg_state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let absolute = |value: OsString| Some(PathBuf::from(value)).filter(|p| p.is_absolute());
    let state_home = xdg_state_home
        .and_then(absolute)
        .or_else(|| home.and_then(absolute).map(|h| h.join(".local/state")))?;
    Some(state_home.join(STATE_DIR_NAME).join(STATE_FILE_NAME))
}

pub fn load(path: &Path) -> HashMap<TreePositionKey, RememberedTreePosition> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(store) = serde_json::from_str::<StoreFile>(&text) else {
        return HashMap::new();
    };
    if store.version != STORE_VERSION {
        return HashMap::new();
    }
    store
        .positions
        .into_iter()
        .map(|record| {
            (
                (record.kind, record.root, record.scan_root),
                RememberedTreePosition { selected: record.selected, opened: record.opened },
            )
        })
        .collect()
}

/// Write via a temp file and rename, so a crash mid-write never leaves a
/// half-written store behind.
pub fn save(
    path: &Path,
    positions: &HashMap<TreePositionKey, RememberedTreePosition>,
) -> std::io::Result<()> {
    let mut records: Vec<PositionRecord> = positions
        .iter()
        .map(|((kind, root, scan_root), position)| PositionRecord {
            kind: *kind,
            root: root.clone(),
            scan_root: scan_root.clone(),
            selected: position.selected.clone(),
            opened: position.opened.clone(),
        })
        .collect();
    records.sort_by(|a, b| a.root.cmp(&b.root));
    let text = serde_json::to_string(&StoreFile { version: STORE_VERSION, positions: records })
        .map_err(std::io::Error::other)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, text)?;
    std::fs::rename(&temp_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::DocKind;

    fn scratch_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("spec_viewer_store_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join(STATE_FILE_NAME)
    }

    fn sample_positions() -> HashMap<TreePositionKey, RememberedTreePosition> {
        let mut positions = HashMap::new();
        positions.insert(
            (TreeSourceKind::Kiro, PathBuf::from("/project/.kiro"), None),
            RememberedTreePosition {
                selected: vec![
                    NodeId::Spec("alpha".to_string()),
                    NodeId::Doc("alpha".to_string(), DocKind::Design),
                ],
                opened: vec![vec![NodeId::Spec("alpha".to_string())]],
            },
        );
        positions.insert(
            (TreeSourceKind::Files, PathBuf::from("/project"), None),
            RememberedTreePosition {
                selected: vec![NodeId::Dir(PathBuf::from("/project/docs")), NodeId::File(PathBuf::from("/project/docs/a.md"))],
                opened: vec![],
            },
        );
        positions.insert(
            (TreeSourceKind::Files, PathBuf::from("/project"), Some(PathBuf::from("/project/docs"))),
            RememberedTreePosition {
                selected: vec![NodeId::File(PathBuf::from("/project/docs/b.md"))],
                opened: vec![],
            },
        );
        positions
    }

    #[test]
    fn saved_positions_load_back_identically_creating_missing_directories() {
        let file = scratch_file("round_trip");

        save(&file, &sample_positions()).expect("save creates the directory and file");

        assert_eq!(load(&file), sample_positions());
        std::fs::remove_dir_all(file.parent().unwrap().parent().unwrap()).ok();
    }

    #[test]
    fn records_saved_without_a_scan_root_still_load_under_a_key_without_one() {
        let file = scratch_file("legacy_record");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            r#"{"version":1,"positions":[{"kind":"files","root":"/project","selected":[],"opened":[]}]}"#,
        )
        .unwrap();

        let loaded = load(&file);

        assert!(loaded.contains_key(&(TreeSourceKind::Files, PathBuf::from("/project"), None)));
        std::fs::remove_dir_all(file.parent().unwrap().parent().unwrap()).ok();
    }

    #[test]
    fn missing_unparsable_or_foreign_version_files_load_as_empty() {
        let file = scratch_file("bad_inputs");
        assert!(load(&file).is_empty(), "missing file");

        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "not json").unwrap();
        assert!(load(&file).is_empty(), "unparsable file");

        std::fs::write(&file, r#"{"version":999,"positions":[]}"#).unwrap();
        assert!(load(&file).is_empty(), "unknown version");
        std::fs::remove_dir_all(file.parent().unwrap().parent().unwrap()).ok();
    }

    #[test]
    fn state_file_prefers_absolute_xdg_state_home_then_home_local_state() {
        let expected = |base: &str| PathBuf::from(base).join("spec-viewer/tree-positions.json");

        assert_eq!(
            state_file_from(Some("/xdg".into()), Some("/home/u".into())),
            Some(expected("/xdg"))
        );
        assert_eq!(
            state_file_from(None, Some("/home/u".into())),
            Some(expected("/home/u/.local/state"))
        );
        assert_eq!(
            state_file_from(Some("relative/dir".into()), Some("/home/u".into())),
            Some(expected("/home/u/.local/state")),
            "a relative XDG_STATE_HOME is ignored"
        );
        assert_eq!(state_file_from(None, None), None);
    }
}
