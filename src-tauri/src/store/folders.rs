//! User-defined display folders (organisation only; never touches install paths).

use super::*;
use crate::domain::{LibraryFolder, LibraryFolderKind};

pub fn list_library_folders(app: &tauri::AppHandle) -> Result<Vec<LibraryFolder>, String> {
    let repo_root = connected_repo_root(app)?;
    Ok(load_repo_library(&repo_root)?.folders)
}

/// Replaces every folder of `kind` with `folders`, then commits to the backup.
pub fn set_library_folders(
    app: &tauri::AppHandle,
    kind: LibraryFolderKind,
    folders: Vec<LibraryFolder>,
) -> Result<Vec<LibraryFolder>, String> {
    let repo_root = connected_repo_root(app)?;
    let mut library = load_repo_library(&repo_root)?;
    if !replace_folders_of_kind(&mut library.folders, kind, folders) {
        return Ok(library.folders);
    }
    save_repo_library(&repo_root, &library)?;
    let _ = sync_after_mutation(app, "Update folders");
    Ok(library.folders)
}

/// Returns whether anything changed. Blank names are dropped, ids are generated
/// when missing, an item stays only in the first folder that lists it, and a
/// parent that is missing or would form a cycle is reset to top level.
pub(crate) fn replace_folders_of_kind(
    all: &mut Vec<LibraryFolder>,
    kind: LibraryFolderKind,
    incoming: Vec<LibraryFolder>,
) -> bool {
    let mut seen_items = HashSet::new();
    let mut normalized: Vec<LibraryFolder> = incoming
        .into_iter()
        .filter_map(|folder| {
            let name = folder.name.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let id = if folder.id.trim().is_empty() {
                Uuid::new_v4().to_string()
            } else {
                folder.id
            };
            let item_ids = folder
                .item_ids
                .into_iter()
                .filter(|item| seen_items.insert(item.clone()))
                .collect();
            let parent_id = folder.parent_id.filter(|p| !p.trim().is_empty());
            Some(LibraryFolder {
                id,
                name,
                kind,
                parent_id,
                item_ids,
            })
        })
        .collect();
    fix_parents(&mut normalized);

    let before = serde_json::to_string(&*all).unwrap_or_default();
    let insert_at = all
        .iter()
        .position(|folder| folder.kind == kind)
        .unwrap_or(all.len());
    all.retain(|folder| folder.kind != kind);
    let insert_at = insert_at.min(all.len());
    all.splice(insert_at..insert_at, normalized);
    serde_json::to_string(&*all).unwrap_or_default() != before
}

fn fix_parents(folders: &mut [LibraryFolder]) {
    let ids: HashSet<String> = folders.iter().map(|f| f.id.clone()).collect();
    for folder in folders.iter_mut() {
        if folder
            .parent_id
            .as_ref()
            .is_some_and(|p| p == &folder.id || !ids.contains(p))
        {
            folder.parent_id = None;
        }
    }
    // Break cycles: walk up from each folder; if we revisit it, detach it.
    for index in 0..folders.len() {
        let start = folders[index].id.clone();
        let mut current = folders[index].parent_id.clone();
        let mut steps = 0;
        while let Some(parent) = current {
            if parent == start || steps > folders.len() {
                folders[index].parent_id = None;
                break;
            }
            current = folders
                .iter()
                .find(|f| f.id == parent)
                .and_then(|f| f.parent_id.clone());
            steps += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, name: &str, kind: LibraryFolderKind, items: &[&str]) -> LibraryFolder {
        LibraryFolder {
            id: id.into(),
            name: name.into(),
            kind,
            parent_id: None,
            item_ids: items.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn child(id: &str, parent: &str) -> LibraryFolder {
        LibraryFolder {
            parent_id: Some(parent.into()),
            ..folder(id, id, LibraryFolderKind::Skill, &[])
        }
    }

    #[test]
    fn keeps_valid_nesting_and_drops_bad_parents() {
        let mut all = Vec::new();
        replace_folders_of_kind(
            &mut all,
            LibraryFolderKind::Skill,
            vec![
                folder("a", "a", LibraryFolderKind::Skill, &[]),
                child("b", "a"),
                child("c", "missing"),
                child("d", "d"),
                child("e", "f"),
                child("f", "e"),
            ],
        );
        let parent = |id: &str| all.iter().find(|f| f.id == id).unwrap().parent_id.clone();
        assert_eq!(parent("b").as_deref(), Some("a"));
        assert_eq!(parent("c"), None);
        assert_eq!(parent("d"), None);
        // The e <-> f cycle is broken; at least one is at top level.
        assert!(parent("e").is_none() || parent("f").is_none());
    }

    #[test]
    fn top_level_folder_serializes_without_parent() {
        let json = serde_json::to_string(&folder("a", "A", LibraryFolderKind::Skill, &[])).unwrap();
        assert!(!json.contains("parentId"));
    }

    #[test]
    fn replaces_only_the_given_kind() {
        let mut all = vec![
            folder("a", "A", LibraryFolderKind::Skill, &["1"]),
            folder("m", "M", LibraryFolderKind::Mcp, &["x"]),
        ];
        let changed = replace_folders_of_kind(
            &mut all,
            LibraryFolderKind::Skill,
            vec![folder("b", "B", LibraryFolderKind::Skill, &["2"])],
        );
        assert!(changed);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "b");
        assert_eq!(all[1].id, "m");
    }

    #[test]
    fn normalizes_names_ids_and_duplicate_items() {
        let mut all = Vec::new();
        replace_folders_of_kind(
            &mut all,
            LibraryFolderKind::Plugin,
            vec![
                folder("", "  Web  ", LibraryFolderKind::Skill, &["1", "2"]),
                folder("x", "   ", LibraryFolderKind::Plugin, &["3"]),
                folder("y", "Data", LibraryFolderKind::Plugin, &["2", "4"]),
            ],
        );
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "Web");
        assert!(!all[0].id.is_empty());
        assert_eq!(all[0].kind, LibraryFolderKind::Plugin);
        assert_eq!(all[1].item_ids, vec!["4".to_string()]);
    }

    #[test]
    fn reports_no_change_for_identical_input() {
        let mut all = vec![folder("a", "A", LibraryFolderKind::Skill, &["1"])];
        let same = all.clone();
        assert!(!replace_folders_of_kind(
            &mut all,
            LibraryFolderKind::Skill,
            same
        ));
    }

    #[test]
    fn old_library_without_folders_still_parses() {
        let library: RepoLibrary =
            serde_json::from_str(r#"{"version":"2","resources":[],"projectProfiles":[]}"#).unwrap();
        assert!(library.folders.is_empty());
    }
}
