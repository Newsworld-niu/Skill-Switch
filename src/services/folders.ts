import { tauriInvoke, type Result } from "./tauri";
import type { LibraryFolder, LibraryFolderKind } from "../types";

export async function libraryFoldersList(): Promise<Result<LibraryFolder[]>> {
  return tauriInvoke<LibraryFolder[]>("library_folders_list");
}

/** Replaces every folder of `kind`; returns all folders after the change. */
export async function libraryFoldersSet(
  kind: LibraryFolderKind,
  folders: LibraryFolder[]
): Promise<Result<LibraryFolder[]>> {
  return tauriInvoke<LibraryFolder[]>("library_folders_set", {
    kind,
    folders,
  });
}
