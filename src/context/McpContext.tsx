import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  type ReactNode,
} from "react";
import type {
  CheckMcpStatusInput,
  CheckMcpStatusResult,
  CreateMcpServerInput,
  CreateMcpServerResult,
  ExternalMcpServer,
  ImportExternalMcpInput,
  InstallMcpResult,
  InstallMcpToProjectInput,
  McpServer,
  UpdateMcpServerInput,
} from "../types";
import {
  mcpCheckStatus,
  mcpCreate,
  mcpDelete,
  mcpImportExternal,
  mcpInstallGlobal,
  mcpInstallToProject,
  mcpList,
  mcpListGlobalEnabledApps,
  mcpUninstallFromProject,
  mcpUninstallGlobal,
  mcpUpdate,
  scanExternalMcpServers,
} from "../services/mcp";
import { useApps } from "./AppContext";
import { useToast } from "../components/ui/Toast";
import type { Result } from "../services/tauri";

interface McpContextValue {
  servers: McpServer[];
  externalServers: ExternalMcpServer[];
  globalEnabledByServer: Record<string, string[]>;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  refreshGlobalStatuses: () => Promise<void>;
  toggleServerGlobalApp: (serverId: string, appId: string) => Promise<boolean>;
  create: (
    input: CreateMcpServerInput
  ) => Promise<Result<CreateMcpServerResult>>;
  update: (input: UpdateMcpServerInput) => Promise<Result<McpServer>>;
  remove: (id: string) => Promise<boolean>;
  importExternal: (
    input: ImportExternalMcpInput
  ) => Promise<Result<CreateMcpServerResult>>;
  installToProject: (
    input: InstallMcpToProjectInput
  ) => Promise<Result<InstallMcpResult>>;
  uninstallFromProject: (
    input: InstallMcpToProjectInput
  ) => Promise<Result<InstallMcpResult>>;
  checkStatus: (
    input: CheckMcpStatusInput
  ) => Promise<Result<CheckMcpStatusResult>>;
}

const McpContext = createContext<McpContextValue | null>(null);

export function McpProvider({ children }: { children: ReactNode }) {
  const { getAppMeta } = useApps();
  const toast = useToast();
  const [servers, setServers] = useState<McpServer[]>([]);
  const [externalServers, setExternalServers] = useState<ExternalMcpServer[]>(
    []
  );
  const [globalEnabledByServer, setGlobalEnabledByServer] = useState<
    Record<string, string[]>
  >({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refreshGlobalStatuses = useCallback(async () => {
    const res = await mcpListGlobalEnabledApps();
    if (res.ok) {
      setGlobalEnabledByServer(res.value);
    }
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);

    const [managedRes, enabledRes, extRes] = await Promise.all([
      mcpList(),
      mcpListGlobalEnabledApps(),
      scanExternalMcpServers(),
    ]);

    if (managedRes.ok) {
      setServers(managedRes.value);
    } else {
      setError(managedRes.error);
    }

    if (enabledRes.ok) {
      setGlobalEnabledByServer(enabledRes.value);
    }

    if (extRes.ok) {
      setExternalServers(extRes.value.filter((item) => !item.isManaged));
    }

    setLoading(false);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggleServerGlobalApp = useCallback(
    async (serverId: string, appId: string): Promise<boolean> => {
      const currentList = globalEnabledByServer[serverId] ?? [];
      const isEnabled = currentList.includes(appId);
      const nextEnabled = !isEnabled;

      setGlobalEnabledByServer((prev) => {
        const list = prev[serverId] ?? [];
        const updated = nextEnabled
          ? Array.from(new Set([...list, appId]))
          : list.filter((id) => id !== appId);
        return { ...prev, [serverId]: updated };
      });

      const appLabel = getAppMeta(appId)?.label ?? appId;
      const tid = toast.loading(
        nextEnabled
          ? `正在写入全局 ${appLabel} MCP 配置…`
          : `正在从全局 ${appLabel} 移除 MCP…`
      );

      const res = nextEnabled
        ? await mcpInstallGlobal({ serverId, apps: [appId] })
        : await mcpUninstallGlobal({ serverId, apps: [appId] });

      if (res.ok && res.value.installedApps.includes(appId)) {
        toast.resolve(
          tid,
          "success",
          nextEnabled
            ? `已在全局 ${appLabel} 启用 MCP`
            : `已从全局 ${appLabel} 移除 MCP`
        );
        if (res.value.warnings.length > 0) {
          toast.info(res.value.warnings[0], 4000);
        }
        return true;
      }

      const errMsg = !res.ok
        ? res.error
        : (res.value.warnings[0] ?? `操作 ${appLabel} 失败`);
      toast.resolve(tid, "error", errMsg);
      setGlobalEnabledByServer((prev) => ({
        ...prev,
        [serverId]: currentList,
      }));
      return false;
    },
    [getAppMeta, globalEnabledByServer, toast]
  );

  const create = useCallback(
    async (
      input: CreateMcpServerInput
    ): Promise<Result<CreateMcpServerResult>> => {
      const res = await mcpCreate(input);
      if (res.ok) {
        setServers((prev) => [res.value.server, ...prev]);
        setGlobalEnabledByServer((prev) => ({
          ...prev,
          [res.value.server.id]: [],
        }));
        return res;
      }
      setError(res.error);
      return res;
    },
    []
  );

  const update = useCallback(
    async (input: UpdateMcpServerInput): Promise<Result<McpServer>> => {
      const res = await mcpUpdate(input);
      if (res.ok) {
        setServers((prev) =>
          prev.map((item) => (item.id === input.id ? res.value : item))
        );
        return res;
      }
      setError(res.error);
      return res;
    },
    []
  );

  const remove = useCallback(
    async (id: string): Promise<boolean> => {
      const res = await mcpDelete(id);
      if (res.ok) {
        setServers((prev) => prev.filter((item) => item.id !== id));
        setGlobalEnabledByServer((prev) => {
          const next = { ...prev };
          delete next[id];
          return next;
        });
        void refresh();
        return true;
      }
      toast.error(`删除 MCP 失败：${res.error}`);
      return false;
    },
    [refresh, toast]
  );

  const importExternal = useCallback(
    async (
      input: ImportExternalMcpInput
    ): Promise<Result<CreateMcpServerResult>> => {
      const res = await mcpImportExternal(input);
      if (res.ok) {
        await refresh();
      }
      return res;
    },
    [refresh]
  );

  const installToProject = useCallback(
    async (
      input: InstallMcpToProjectInput
    ): Promise<Result<InstallMcpResult>> => {
      return mcpInstallToProject(input);
    },
    []
  );

  const uninstallFromProject = useCallback(
    async (
      input: InstallMcpToProjectInput
    ): Promise<Result<InstallMcpResult>> => {
      return mcpUninstallFromProject(input);
    },
    []
  );

  const checkStatus = useCallback(
    async (
      input: CheckMcpStatusInput
    ): Promise<Result<CheckMcpStatusResult>> => {
      return mcpCheckStatus(input);
    },
    []
  );

  return (
    <McpContext.Provider
      value={{
        servers,
        externalServers,
        globalEnabledByServer,
        loading,
        error,
        refresh,
        refreshGlobalStatuses,
        toggleServerGlobalApp,
        create,
        update,
        remove,
        importExternal,
        installToProject,
        uninstallFromProject,
        checkStatus,
      }}
    >
      {children}
    </McpContext.Provider>
  );
}

export function useMcp(): McpContextValue {
  const ctx = useContext(McpContext);
  if (!ctx) {
    throw new Error("useMcp must be used within McpProvider");
  }
  return ctx;
}
