import { tauriInvoke, type Result } from "./tauri";
import type {
  CheckPluginStatusInput,
  CheckPluginStatusResult,
  ImportPluginBatchResult,
  ImportPluginFromFolderInput,
  ImportPluginFromGitInput,
  ImportPluginOutcome,
  ImportStagedPluginsInput,
  InstallPluginGlobalInput,
  InstallPluginResult,
  InstallPluginToProjectInput,
  Plugin,
  UpdatePluginConfigInput,
} from "../types";

export async function pluginList(): Promise<Result<Plugin[]>> {
  return tauriInvoke<Plugin[]>("plugin_list");
}

export async function pluginGet(id: string): Promise<Result<Plugin | null>> {
  return tauriInvoke<Plugin | null>("plugin_get", { id });
}

export async function pluginImportFromFolder(
  input: ImportPluginFromFolderInput
): Promise<Result<ImportPluginOutcome>> {
  return tauriInvoke<ImportPluginOutcome>("plugin_import_from_folder", {
    input,
  });
}

export async function pluginImportFromDialog(): Promise<
  Result<ImportPluginOutcome | null>
> {
  return tauriInvoke<ImportPluginOutcome | null>("plugin_import_from_dialog");
}

export async function pluginImportFromZip(
  zipPath: string
): Promise<Result<ImportPluginOutcome>> {
  return tauriInvoke<ImportPluginOutcome>("plugin_import_from_zip", {
    zipPath,
  });
}

export async function pluginImportFromGit(
  input: ImportPluginFromGitInput
): Promise<Result<ImportPluginOutcome>> {
  return tauriInvoke<ImportPluginOutcome>("plugin_import_from_git", { input });
}

export async function pluginImportStaged(
  input: ImportStagedPluginsInput
): Promise<Result<ImportPluginBatchResult>> {
  return tauriInvoke<ImportPluginBatchResult>("plugin_import_staged", {
    input,
  });
}

export async function pluginDiscardStaged(
  stagingId: string
): Promise<Result<void>> {
  return tauriInvoke<void>("plugin_discard_staged", { stagingId });
}

export async function pluginUpdateConfig(
  input: UpdatePluginConfigInput
): Promise<Result<Plugin>> {
  return tauriInvoke<Plugin>("plugin_update_config", { input });
}

export async function pluginDelete(id: string): Promise<Result<void>> {
  return tauriInvoke<void>("plugin_delete", { id });
}

export async function pluginInstallGlobal(
  input: InstallPluginGlobalInput
): Promise<Result<InstallPluginResult>> {
  return tauriInvoke<InstallPluginResult>("plugin_install_global", { input });
}

export async function pluginUninstallGlobal(
  input: InstallPluginGlobalInput
): Promise<Result<InstallPluginResult>> {
  return tauriInvoke<InstallPluginResult>("plugin_uninstall_global", { input });
}

export async function pluginInstallToProject(
  input: InstallPluginToProjectInput
): Promise<Result<InstallPluginResult>> {
  return tauriInvoke<InstallPluginResult>("plugin_install_to_project", {
    input,
  });
}

export async function pluginUninstallFromProject(
  input: InstallPluginToProjectInput
): Promise<Result<InstallPluginResult>> {
  return tauriInvoke<InstallPluginResult>("plugin_uninstall_from_project", {
    input,
  });
}

export async function pluginCheckStatus(
  input: CheckPluginStatusInput
): Promise<Result<CheckPluginStatusResult>> {
  return tauriInvoke<CheckPluginStatusResult>("plugin_check_status", { input });
}

export async function pluginListGlobalEnabledApps(): Promise<
  Result<Record<string, string[]>>
> {
  return tauriInvoke<Record<string, string[]>>(
    "plugin_list_global_enabled_apps"
  );
}
