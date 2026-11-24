import { tauriInvoke, type Result } from "./tauri";
import type {
  CheckMcpStatusInput,
  CheckMcpStatusResult,
  CreateMcpServerInput,
  CreateMcpServerResult,
  ExternalMcpServer,
  ImportExternalMcpInput,
  InstallMcpGlobalInput,
  InstallMcpResult,
  InstallMcpToProjectInput,
  McpServer,
  UpdateMcpServerInput,
} from "../types";

export async function mcpList(): Promise<Result<McpServer[]>> {
  return tauriInvoke<McpServer[]>("mcp_list");
}

export async function mcpGet(id: string): Promise<Result<McpServer | null>> {
  return tauriInvoke<McpServer | null>("mcp_get", { id });
}

export async function mcpCreate(
  input: CreateMcpServerInput
): Promise<Result<CreateMcpServerResult>> {
  return tauriInvoke<CreateMcpServerResult>("mcp_create", { input });
}

export async function mcpUpdate(
  input: UpdateMcpServerInput
): Promise<Result<McpServer>> {
  return tauriInvoke<McpServer>("mcp_update", { input });
}

export async function mcpDelete(id: string): Promise<Result<void>> {
  return tauriInvoke<void>("mcp_delete", { id });
}

export async function mcpInstallGlobal(
  input: InstallMcpGlobalInput
): Promise<Result<InstallMcpResult>> {
  return tauriInvoke<InstallMcpResult>("mcp_install_global", { input });
}

export async function mcpUninstallGlobal(
  input: InstallMcpGlobalInput
): Promise<Result<InstallMcpResult>> {
  return tauriInvoke<InstallMcpResult>("mcp_uninstall_global", { input });
}

export async function mcpInstallToProject(
  input: InstallMcpToProjectInput
): Promise<Result<InstallMcpResult>> {
  return tauriInvoke<InstallMcpResult>("mcp_install_to_project", { input });
}

export async function mcpUninstallFromProject(
  input: InstallMcpToProjectInput
): Promise<Result<InstallMcpResult>> {
  return tauriInvoke<InstallMcpResult>("mcp_uninstall_from_project", { input });
}

export async function mcpCheckStatus(
  input: CheckMcpStatusInput
): Promise<Result<CheckMcpStatusResult>> {
  return tauriInvoke<CheckMcpStatusResult>("mcp_check_status", { input });
}

export async function mcpListGlobalEnabledApps(): Promise<
  Result<Record<string, string[]>>
> {
  return tauriInvoke<Record<string, string[]>>("mcp_list_global_enabled_apps");
}

export async function scanExternalMcpServers(): Promise<
  Result<ExternalMcpServer[]>
> {
  return tauriInvoke<ExternalMcpServer[]>("scan_external_mcp_servers");
}

export async function mcpImportExternal(
  input: ImportExternalMcpInput
): Promise<Result<CreateMcpServerResult>> {
  return tauriInvoke<CreateMcpServerResult>("mcp_import_external", { input });
}
