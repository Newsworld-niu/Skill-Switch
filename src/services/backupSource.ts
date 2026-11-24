import { listen } from "@tauri-apps/api/event";
import { tauriInvoke, type Result } from "./tauri";
import type {
  BackupSourceStatus,
  BackupSyncState,
  BootstrapBackupInput,
  SkillMutationResult,
  ImportRepoSourceSkillInput,
} from "../types";

export const BACKUP_SOURCE_REPO_ID = "__backup_source__";

export async function backupSourceStatus(): Promise<
  Result<BackupSourceStatus>
> {
  return tauriInvoke<BackupSourceStatus>("backup_source_status");
}

export async function backupSourceConnect(): Promise<
  Result<BackupSourceStatus>
> {
  return tauriInvoke<BackupSourceStatus>("backup_source_connect");
}

export async function backupSourcePull(): Promise<Result<BackupSourceStatus>> {
  return tauriInvoke<BackupSourceStatus>("backup_source_pull");
}

export async function backupSourcePush(): Promise<Result<BackupSourceStatus>> {
  return tauriInvoke<BackupSourceStatus>("backup_source_push");
}

export async function backupSourceBootstrap(
  input: BootstrapBackupInput
): Promise<Result<BackupSourceStatus>> {
  return tauriInvoke<BackupSourceStatus>("backup_source_bootstrap", { input });
}

export async function skillImportFromRepoSource(
  input: ImportRepoSourceSkillInput
): Promise<Result<SkillMutationResult>> {
  return tauriInvoke<SkillMutationResult>("skill_import_from_repo_source", {
    input,
  });
}

export async function backupSyncStateGet(): Promise<Result<BackupSyncState>> {
  return tauriInvoke<BackupSyncState>("backup_sync_state_get");
}

export async function backupSyncRetry(): Promise<Result<void>> {
  return tauriInvoke<void>("backup_sync_retry");
}

/** Subscribes to background push state changes. Resolves to an unsubscribe function. */
export function onBackupSyncState(
  handler: (state: BackupSyncState) => void
): Promise<() => void> {
  return listen<BackupSyncState>("backup-sync-state", (event) =>
    handler(event.payload)
  );
}

/** Fired after the startup pull, when library contents may have changed. */
export function onLibraryChanged(handler: () => void): Promise<() => void> {
  return listen("library-changed", () => handler());
}
