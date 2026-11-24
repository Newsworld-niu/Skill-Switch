import { tauriInvoke, type Result } from "./tauri";
import type {
  DiscoveredSshHost,
  SshAppSyncLedger,
  SshConnectionTestResult,
  SshSyncSummaryResult,
  SshTargetConfig,
} from "../types";

export async function sshDiscoverHosts(): Promise<Result<DiscoveredSshHost[]>> {
  return tauriInvoke<DiscoveredSshHost[]>("ssh_discover_hosts");
}

export async function sshTestConnection(
  ssh: SshTargetConfig
): Promise<Result<SshConnectionTestResult>> {
  return tauriInvoke<SshConnectionTestResult>("ssh_test_connection", { ssh });
}

export async function sshCalibrateTarget(
  appId: string
): Promise<Result<SshAppSyncLedger>> {
  return tauriInvoke<SshAppSyncLedger>("ssh_calibrate_target", { appId });
}

export async function sshSyncAllEnabled(
  appId: string
): Promise<Result<SshSyncSummaryResult>> {
  return tauriInvoke<SshSyncSummaryResult>("ssh_sync_all_enabled", { appId });
}
