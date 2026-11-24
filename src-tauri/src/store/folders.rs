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
/// when missing, and an item stays only in the first folder that lists it.
pub(crate) fn replace_folders_of_kind(
    all: &mut Vec<LibraryFolder>,
    kind: LibraryFolderKind,
    incoming: Vec<LibraryFolder>,
) -> bool {
    let mut seen_items = HashSet::new();
    let normalized: Vec<LibraryFolder> = incoming
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
            Some(LibraryFolder {
                id,
                name,
                kind,
                item_ids,
            })
        })
        .collect();

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

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, name: &str, kind: LibraryFolderKind, items: &[&str]) -> LibraryFolder {
        LibraryFolder {
            id: id.into(),
            name: name.into(),
            kind,
            item_ids: items.iter().map(|s| s.to_string()).collect(),
        }
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
