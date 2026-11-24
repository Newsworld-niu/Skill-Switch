import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  type ReactNode,
} from "react";
import type {
  CheckPluginStatusInput,
  CheckPluginStatusResult,
  ImportPluginBatchResult,
  ImportPluginFromFolderInput,
  ImportPluginFromGitInput,
  ImportPluginOutcome,
  ImportStagedPluginsInput,
  InstallPluginResult,
  InstallPluginToProjectInput,
  Plugin,
  UpdatePluginConfigInput,
} from "../types";
import {
  pluginCheckStatus,
  pluginDelete,
  pluginDiscardStaged,
  pluginImportFromDialog,
  pluginImportFromFolder,
  pluginImportFromGit,
  pluginImportFromZip,
  pluginImportStaged,
  pluginInstallGlobal,
  pluginInstallToProject,
  pluginList,
  pluginListGlobalEnabledApps,
  pluginUninstallFromProject,
  pluginUninstallGlobal,
  pluginUpdateConfig,
} from "../services/plugin";
import { useApps } from "./AppContext";
import { useSkills } from "./SkillContext";
import { useSlashCommands } from "./SlashCommandContext";
import { useMcp } from "./McpContext";
import { useToast } from "../components/ui/Toast";
import type { Result } from "../services/tauri";

interface PluginContextValue {
  plugins: Plugin[];
  globalEnabledByPlugin: Record<string, string[]>;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  togglePluginGlobalApp: (pluginId: string, appId: string) => Promise<boolean>;
  importFromDialog: () => Promise<Result<ImportPluginOutcome | null>>;
  importFromFolder: (
    input: ImportPluginFromFolderInput
  ) => Promise<Result<ImportPluginOutcome>>;
  importFromZip: (zipPath: string) => Promise<Result<ImportPluginOutcome>>;
  importFromGit: (
    input: ImportPluginFromGitInput
  ) => Promise<Result<ImportPluginOutcome>>;
  importStaged: (
    input: ImportStagedPluginsInput
  ) => Promise<Result<ImportPluginBatchResult>>;
  discardStaged: (stagingId: string) => Promise<void>;
  updateConfig: (input: UpdatePluginConfigInput) => Promise<Result<Plugin>>;
  remove: (id: string) => Promise<boolean>;
  installToProject: (
    input: InstallPluginToProjectInput
  ) => Promise<Result<InstallPluginResult>>;
  uninstallFromProject: (
    input: InstallPluginToProjectInput
  ) => Promise<Result<InstallPluginResult>>;
  checkStatus: (
    input: CheckPluginStatusInput
  ) => Promise<Result<CheckPluginStatusResult>>;
}

const PluginContext = createContext<PluginContextValue | null>(null);

export function PluginProvider({ children }: { children: ReactNode }) {
  const { getAppMeta } = useApps();
  const { refresh: refreshSkills, refreshGlobalStatuses: refreshSkillGlobal } =
    useSkills();
  const { refresh: refreshCommands } = useSlashCommands();
  const { refresh: refreshMcp, refreshGlobalStatuses: refreshMcpGlobal } =
    useMcp();
  const toast = useToast();

  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [globalEnabledByPlugin, setGlobalEnabledByPlugin] = useState<
    Record<string, string[]>
  >({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const syncDependentContexts = useCallback(async () => {
    await Promise.all([refreshSkills(), refreshCommands(), refreshMcp()]);
  }, [refreshCommands, refreshMcp, refreshSkills]);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);

    const [listRes, statusRes] = await Promise.all([
      pluginList(),
      pluginListGlobalEnabledApps(),
    ]);

    if (listRes.ok) {
      setPlugins(listRes.value);
    } else {
      setError(listRes.error);
    }

    if (statusRes.ok) {
      setGlobalEnabledByPlugin(statusRes.value);
    }

    setLoading(false);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const togglePluginGlobalApp = useCallback(
    async (pluginId: string, appId: string): Promise<boolean> => {
      const current = globalEnabledByPlugin[pluginId] ?? [];
      const isEnabled = current.includes(appId);
      const nextEnabled = !isEnabled;

      setGlobalEnabledByPlugin((prev) => {
        const list = prev[pluginId] ?? [];
        const updated = nextEnabled
          ? Array.from(new Set([...list, appId]))
          : list.filter((id) => id !== appId);
        return { ...prev, [pluginId]: updated };
      });

      const appLabel = getAppMeta(appId)?.label ?? appId;
      const tid = toast.loading(
        nextEnabled
          ? `正在为全局 ${appLabel} 装配插件…`
          : `正在从全局 ${appLabel} 卸载插件…`
      );

      const res = nextEnabled
        ? await pluginInstallGlobal({
            pluginId,
            apps: [appId],
            preferNative: true,
          })
        : await pluginUninstallGlobal({
            pluginId,
            apps: [appId],
            preferNative: true,
          });

      if (res.ok && res.value.installedApps.includes(appId)) {
        const mode = res.value.appModes[appId];
        const modeTag =
          mode === "native" ? "（原生挂载+投射）" : "（跨端组件投射）";
        toast.resolve(
          tid,
          "success",
          nextEnabled
            ? `已在全局 ${appLabel} 启用插件${modeTag}`
            : `已从全局 ${appLabel} 卸载插件`
        );
        if (res.value.warnings.length > 0) {
          toast.info(res.value.warnings[0], 4200);
        }
        await Promise.all([refreshSkillGlobal(), refreshMcpGlobal()]);
        return true;
      }

      const errMsg = !res.ok
        ? res.error
        : (res.value.warnings[0] ?? `操作 ${appLabel} 失败`);
      toast.resolve(tid, "error", errMsg);
      setGlobalEnabledByPlugin((prev) => ({
        ...prev,
        [pluginId]: current,
      }));
      return false;
    },
    [
      getAppMeta,
      globalEnabledByPlugin,
      refreshMcpGlobal,
      refreshSkillGlobal,
      toast,
    ]
  );

  const importFromDialog = useCallback(async (): Promise<
    Result<ImportPluginOutcome | null>
  > => {
    const res = await pluginImportFromDialog();
    if (res.ok && res.value?.status === "imported") {
      await Promise.all([refresh(), syncDependentContexts()]);
    }
    return res;
  }, [refresh, syncDependentContexts]);

  const importFromFolder = useCallback(
    async (
      input: ImportPluginFromFolderInput
    ): Promise<Result<ImportPluginOutcome>> => {
      const res = await pluginImportFromFolder(input);
      if (res.ok && res.value.status === "imported") {
        await Promise.all([refresh(), syncDependentContexts()]);
      }
      return res;
    },
    [refresh, syncDependentContexts]
  );

  const importFromZip = useCallback(
    async (zipPath: string): Promise<Result<ImportPluginOutcome>> => {
      const res = await pluginImportFromZip(zipPath);
      if (res.ok && res.value.status === "imported") {
        await Promise.all([refresh(), syncDependentContexts()]);
      }
      return res;
    },
    [refresh, syncDependentContexts]
  );

  const importFromGit = useCallback(
    async (
      input: ImportPluginFromGitInput
    ): Promise<Result<ImportPluginOutcome>> => {
      const res = await pluginImportFromGit(input);
      if (res.ok && res.value.status === "imported") {
        await Promise.all([refresh(), syncDependentContexts()]);
      }
      return res;
    },
    [refresh, syncDependentContexts]
  );

  const importStaged = useCallback(
    async (
      input: ImportStagedPluginsInput
    ): Promise<Result<ImportPluginBatchResult>> => {
      const res = await pluginImportStaged(input);
      if (res.ok) {
        await Promise.all([refresh(), syncDependentContexts()]);
      }
      return res;
    },
    [refresh, syncDependentContexts]
  );

  const discardStaged = useCallback(async (stagingId: string) => {
    await pluginDiscardStaged(stagingId);
  }, []);

  const updateConfig = useCallback(
    async (input: UpdatePluginConfigInput): Promise<Result<Plugin>> => {
      const res = await pluginUpdateConfig(input);
      if (res.ok) {
        setPlugins((prev) =>
          prev.map((p) => (p.id === input.pluginId ? res.value : p))
        );
        await refreshMcp();
      }
      return res;
    },
    [refreshMcp]
  );

  const remove = useCallback(
    async (id: string): Promise<boolean> => {
      const res = await pluginDelete(id);
      if (res.ok) {
        setPlugins((prev) => prev.filter((p) => p.id !== id));
        setGlobalEnabledByPlugin((prev) => {
          const next = { ...prev };
          delete next[id];
          return next;
        });
        await syncDependentContexts();
        return true;
      }
      toast.error(`删除插件失败：${res.error}`);
      return false;
    },
    [syncDependentContexts, toast]
  );

  const installToProject = useCallback(
    async (
      input: InstallPluginToProjectInput
    ): Promise<Result<InstallPluginResult>> => {
      return pluginInstallToProject(input);
    },
    []
  );

  const uninstallFromProject = useCallback(
    async (
      input: InstallPluginToProjectInput
    ): Promise<Result<InstallPluginResult>> => {
      return pluginUninstallFromProject(input);
    },
    []
  );

  const checkStatus = useCallback(
    async (
      input: CheckPluginStatusInput
    ): Promise<Result<CheckPluginStatusResult>> => {
      return pluginCheckStatus(input);
    },
    []
  );

  return (
    <PluginContext.Provider
      value={{
        plugins,
        globalEnabledByPlugin,
        loading,
        error,
        refresh,
        togglePluginGlobalApp,
        importFromDialog,
        importFromFolder,
        importFromZip,
        importFromGit,
        importStaged,
        discardStaged,
        updateConfig,
        remove,
        installToProject,
        uninstallFromProject,
        checkStatus,
      }}
    >
      {children}
    </PluginContext.Provider>
  );
}

export function usePlugins(): PluginContextValue {
  const ctx = useContext(PluginContext);
  if (!ctx) {
    throw new Error("usePlugins must be used within PluginProvider");
  }
  return ctx;
}
