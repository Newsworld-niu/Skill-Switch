import { useState, useMemo, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  FolderOpen,
  FileArchive,
  GitBranch,
  Search,
  Trash2,
  Lock,
  ExternalLink,
  FolderPlus,
  Layers,
  Check,
} from "lucide-react";
import type { PageId } from "../App";
import { usePlugins } from "../context/PluginContext";
import { useApps } from "../context/AppContext";
import { useProjects } from "../context/ProjectContext";
import { showInFinder } from "../services/skill";
import { useToast } from "../components/ui/Toast";
import type {
  ClaudeProjectMcpMode,
  DiscoveredPluginCandidate,
  ImportPluginOutcome,
  Plugin,
  PluginAppInstallStatus,
  PluginManifestKind,
} from "../types";
import s from "./PluginsPage.module.css";
import { Drawer } from "../components/ui/Drawer";
import { FolderedGrid, FolderPicker } from "../components/ui/FolderedGrid";
import { useConfirm } from "../components/ui/ConfirmDialog";

interface Props {
  onNavigate: (page: PageId) => void;
}

interface PendingMultiPluginSelection {
  stagingId: string;
  sourceLabel: string;
  marketplaceName?: string | null;
  candidates: DiscoveredPluginCandidate[];
}

const MANIFEST_LABELS: Record<PluginManifestKind, string> = {
  "agent-plugins": "Agent Plugins 1.0",
  "claude-plugin": "Claude Plugin",
  "codex-plugin": "Codex Plugin",
  "cursor-plugin": "Cursor Plugin",
  "antigravity-plugin": "Antigravity Plugin",
  "gemini-extension": "Gemini Extension",
  generic: "复合扩展包",
};

const NATIVE_CAPABLE_APPS = new Set([
  "claude",
  "codex",
  "cursor",
  "antigravity",
  "gemini",
]);

export function PluginsPage({ onNavigate }: Props) {
  const {
    plugins,
    globalEnabledByPlugin,
    togglePluginGlobalApp,
    importFromDialog,
    importFromZip,
    importFromGit,
    importStaged,
    discardStaged,
    updateConfig,
    remove,
    installToProject,
    uninstallFromProject,
    checkStatus,
  } = usePlugins();
  const { visibleSkillApps, getAppMeta } = useApps();
  const { projects } = useProjects();
  const toast = useToast();
  const confirm = useConfirm();

  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // userConfig & secret edits
  const [configDrafts, setConfigDrafts] = useState<Record<string, string>>({});
  const [secretDrafts, setSecretDrafts] = useState<Record<string, string>>({});

  // Project install state
  const [selectedProjectPath, setSelectedProjectPath] = useState("");
  const [claudeMcpMode, setClaudeMcpMode] =
    useState<ClaudeProjectMcpMode>("local");
  const [projectStatuses, setProjectStatuses] = useState<
    PluginAppInstallStatus[]
  >([]);

  // Git import modal
  const [showGitModal, setShowGitModal] = useState(false);
  const [gitRepoUrl, setGitRepoUrl] = useState("");
  const [gitSubPath, setGitSubPath] = useState("");
  const [gitBranch, setGitBranch] = useState("");
  const [importingGit, setImportingGit] = useState(false);

  // Multi-plugin / Marketplace selection modal
  const [pendingSelection, setPendingSelection] =
    useState<PendingMultiPluginSelection | null>(null);
  const [selectedSubPaths, setSelectedSubPaths] = useState<string[]>([]);
  const [candidateQuery, setCandidateQuery] = useState("");
  const [importingStaged, setImportingStaged] = useState(false);

  const filteredPlugins = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return plugins;
    return plugins.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.slug.toLowerCase().includes(q) ||
        (p.description ?? "").toLowerCase().includes(q)
    );
  }, [plugins, query]);

  // Details open only on click, in a drawer; closes if the plugin disappears.
  const selectedPlugin = useMemo(
    () => plugins.find((p) => p.id === selectedId) ?? null,
    [plugins, selectedId]
  );

  const filteredCandidates = useMemo(() => {
    if (!pendingSelection) return [];
    const q = candidateQuery.trim().toLowerCase();
    if (!q) return pendingSelection.candidates;
    return pendingSelection.candidates.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        c.subPath.toLowerCase().includes(q) ||
        (c.description ?? "").toLowerCase().includes(q) ||
        (c.category ?? "").toLowerCase().includes(q)
    );
  }, [candidateQuery, pendingSelection]);

  useEffect(() => {
    if (selectedPlugin) {
      setConfigDrafts(selectedPlugin.userConfigValues ?? {});
      setSecretDrafts({});
    }
  }, [selectedPlugin]);

  useEffect(() => {
    if (!selectedProjectPath && projects.length > 0) {
      const first = projects.find((p) => p.path);
      if (first?.path) setSelectedProjectPath(first.path);
    }
  }, [projects, selectedProjectPath]);

  const refreshProjectStatuses = useCallback(async () => {
    if (!selectedPlugin || !selectedProjectPath.trim()) {
      setProjectStatuses([]);
      return;
    }
    const res = await checkStatus({
      pluginId: selectedPlugin.id,
      scope: "project",
      projectPath: selectedProjectPath.trim(),
    });
    if (res.ok) {
      setProjectStatuses(res.value.statuses);
    }
  }, [checkStatus, selectedPlugin, selectedProjectPath]);

  useEffect(() => {
    void refreshProjectStatuses();
  }, [refreshProjectStatuses]);

  const applyImportOutcome = useCallback(
    (outcome: ImportPluginOutcome, loadingToastId?: string) => {
      if (outcome.status === "imported") {
        const first = outcome.plugins[0];
        if (first) {
          setSelectedId(first.id);
        }
        const msg =
          outcome.plugins.length === 1
            ? `已导入插件「${outcome.plugins[0].name}」并同步投射子组件`
            : `已批量导入 ${outcome.plugins.length} 个插件包`;
        if (loadingToastId) {
          toast.resolve(loadingToastId, "success", msg);
        } else {
          toast.success(msg);
        }
        if (outcome.warnings.length > 0) {
          toast.info(outcome.warnings[0], 4500);
        }
        return;
      }

      if (loadingToastId) {
        toast.dismiss(loadingToastId);
      }
      setCandidateQuery("");
      // Pre-select all plugins when bundle size is modest (e.g. stitch-skills has 3 plugins)
      setSelectedSubPaths(
        outcome.candidates.length <= 10
          ? outcome.candidates.map((c) => c.subPath)
          : []
      );
      setPendingSelection({
        stagingId: outcome.stagingId,
        sourceLabel: outcome.sourceLabel,
        marketplaceName: outcome.marketplaceName,
        candidates: outcome.candidates,
      });
    },
    [toast]
  );

  const handleImportFolder = async () => {
    const res = await importFromDialog();
    if (!res.ok) {
      toast.error(`导入插件失败：${res.error}`);
      return;
    }
    if (res.value) {
      applyImportOutcome(res.value);
    }
  };

  const handleImportZip = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Plugin Archive", extensions: ["zip"] }],
        title: "选择插件或插件合集 ZIP 压缩包",
      });
      if (!selected) return;
      const tid = toast.loading("正在解压并解析插件包…");
      const res = await importFromZip(selected as string);
      if (res.ok) {
        applyImportOutcome(res.value, tid);
      } else {
        toast.resolve(tid, "error", res.error);
      }
    } catch {
      // cancelled
    }
  };

  const handleImportGit = async () => {
    if (!gitRepoUrl.trim()) {
      toast.error("请输入 GitHub 仓库地址或 owner/repo");
      return;
    }
    setImportingGit(true);
    const res = await importFromGit({
      repoUrl: gitRepoUrl.trim(),
      subPath: gitSubPath.trim() || null,
      branch: gitBranch.trim() || null,
      projectComponentsToLibrary: true,
    });
    setImportingGit(false);
    if (res.ok) {
      setShowGitModal(false);
      setGitRepoUrl("");
      setGitSubPath("");
      setGitBranch("");
      applyImportOutcome(res.value);
    } else {
      toast.error(`克隆导入失败：${res.error}`);
    }
  };

  const handleCancelMultiSelect = async () => {
    if (!pendingSelection || importingStaged) return;
    const sid = pendingSelection.stagingId;
    setPendingSelection(null);
    setSelectedSubPaths([]);
    setCandidateQuery("");
    await discardStaged(sid);
  };

  const handleConfirmMultiSelect = async () => {
    if (!pendingSelection || selectedSubPaths.length === 0) {
      toast.error("请至少勾选一个插件");
      return;
    }
    setImportingStaged(true);
    const res = await importStaged({
      stagingId: pendingSelection.stagingId,
      subPaths: selectedSubPaths,
      projectComponentsToLibrary: true,
    });
    setImportingStaged(false);
    if (res.ok) {
      setPendingSelection(null);
      setSelectedSubPaths([]);
      setCandidateQuery("");
      if (res.value.plugins[0]) {
        setSelectedId(res.value.plugins[0].id);
      }
      const names = res.value.plugins.map((p) => p.name).join("、");
      toast.success(
        res.value.plugins.length === 1
          ? `已导入插件「${names}」`
          : `已导入 ${res.value.plugins.length} 个插件（${names}）`
      );
      if (res.value.warnings.length > 0) {
        toast.info(res.value.warnings[0], 4500);
      }
    } else {
      toast.error(`导入失败：${res.error}`);
    }
  };

  const toggleCandidatePath = (subPath: string) => {
    setSelectedSubPaths((prev) =>
      prev.includes(subPath)
        ? prev.filter((p) => p !== subPath)
        : [...prev, subPath]
    );
  };

  const handleToggleAllFiltered = () => {
    const visiblePaths = filteredCandidates.map((c) => c.subPath);
    const allVisibleSelected =
      visiblePaths.length > 0 &&
      visiblePaths.every((p) => selectedSubPaths.includes(p));
    if (allVisibleSelected) {
      const visibleSet = new Set(visiblePaths);
      setSelectedSubPaths((prev) => prev.filter((p) => !visibleSet.has(p)));
    } else {
      setSelectedSubPaths((prev) =>
        Array.from(new Set([...prev, ...visiblePaths]))
      );
    }
  };

  const handleSaveUserConfig = async (plugin: Plugin) => {
    const res = await updateConfig({
      pluginId: plugin.id,
      userConfigValues: configDrafts,
      secretValues:
        Object.keys(secretDrafts).length > 0 ? secretDrafts : undefined,
    });
    if (res.ok) {
      setSecretDrafts({});
      toast.success("已更新插件配置并同步至捆绑 MCP 服务");
    } else {
      toast.error(res.error);
    }
  };

  const handleToggleProjectApp = async (
    plugin: Plugin,
    appId: string,
    installed: boolean
  ) => {
    if (!selectedProjectPath.trim()) {
      toast.error("请先选择项目根目录");
      return;
    }
    const appLabel = getAppMeta(appId)?.label ?? appId;
    const res = installed
      ? await uninstallFromProject({
          pluginId: plugin.id,
          projectPath: selectedProjectPath.trim(),
          apps: [appId],
          claudeMcpMode,
        })
      : await installToProject({
          pluginId: plugin.id,
          projectPath: selectedProjectPath.trim(),
          apps: [appId],
          claudeMcpMode,
        });

    if (res.ok && res.value.installedApps.includes(appId)) {
      toast.success(
        installed
          ? `已从项目 ${appLabel} 卸载插件组件`
          : `已向项目 ${appLabel} 投射插件组件`
      );
      if (res.value.warnings.length > 0) {
        toast.info(res.value.warnings[0], 4500);
      }
      await refreshProjectStatuses();
    } else {
      toast.error(!res.ok ? res.error : "操作失败");
    }
  };

  return (
    <div className={s.page}>
      <header className={s.header}>
        <div className={s.titleBlock}>
          <h1 className={s.title}>Plugins & Extensions</h1>
          <p className={s.subtitle}>
            兼容 Claude / Codex / Cursor / Antigravity / Gemini 插件与
            Marketplace 合集仓库，自动展开路径宏并跨端投射 Skills 与 MCP
          </p>
        </div>

        <div className={s.actions}>
          <button
            type="button"
            className={s.btn}
            onClick={() => void handleImportFolder()}
          >
            <FolderOpen size={13} /> 目录导入
          </button>
          <button
            type="button"
            className={s.btn}
            onClick={() => void handleImportZip()}
          >
            <FileArchive size={13} /> ZIP 导入
          </button>
          <button
            type="button"
            className={s.btnPrimary}
            onClick={() => setShowGitModal(true)}
          >
            <GitBranch size={13} /> Git 仓库导入
          </button>
        </div>
      </header>

      <div className={s.toolbar}>
        <div className={s.searchWrap}>
          <Search size={14} className={s.searchIcon} />
          <input
            className={s.searchInput}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            data-search-input
            placeholder="搜索已导入插件包…"
          />
        </div>
      </div>

      {filteredPlugins.length === 0 ? (
        <div className={s.emptyState}>
          <span>尚未导入任何复合插件包（Plugin / Extension）</span>
          <div className={s.actions}>
            <button
              type="button"
              className={s.btn}
              onClick={() => void handleImportFolder()}
            >
              <FolderOpen size={13} /> 从本地目录导入
            </button>
            <button
              type="button"
              className={s.btnPrimary}
              onClick={() => setShowGitModal(true)}
            >
              <GitBranch size={13} /> 从 GitHub 仓库导入
            </button>
          </div>
        </div>
      ) : (
        <div className={s.body}>
          <div className={s.listPane}>
            <FolderedGrid
              kind="plugin"
              items={filteredPlugins}
              getId={(plugin) => plugin.id}
              searching={!!query.trim()}
              empty={null}
              renderItem={(plugin) => {
                const enabledCount = (globalEnabledByPlugin[plugin.id] ?? [])
                  .length;
                return (
                  <button
                    key={plugin.id}
                    type="button"
                    className={`${s.pluginCard} ${selectedPlugin?.id === plugin.id ? s.pluginCardActive : ""}`}
                    onClick={() => setSelectedId(plugin.id)}
                  >
                    <div className={s.cardTop}>
                      <span className={s.cardTitle}>{plugin.name}</span>
                      <span className={s.manifestBadge}>
                        {MANIFEST_LABELS[plugin.manifestKind]}
                      </span>
                    </div>
                    {plugin.description && (
                      <div className={s.cardDesc}>{plugin.description}</div>
                    )}
                    <div className={s.pillRow}>
                      <span className={s.pill}>
                        Skills {plugin.components.skills.length}
                      </span>
                      <span className={s.pill}>
                        MCP {plugin.components.mcpServers.length}
                      </span>
                      {plugin.components.hooks.length > 0 && (
                        <span className={s.pill}>
                          Hooks {plugin.components.hooks.length}
                        </span>
                      )}
                      <span className={s.pill}>全局 {enabledCount} 端</span>
                    </div>
                  </button>
                );
              }}
            />
          </div>

          {selectedPlugin && (
            <Drawer
              label={selectedPlugin.name}
              onClose={() => setSelectedId(null)}
            >
              <section className={s.detailPane}>
                <div className={s.detailHeader}>
                  <div>
                    <div className={s.detailTitle}>
                      {selectedPlugin.name}{" "}
                      {selectedPlugin.version
                        ? `v${selectedPlugin.version}`
                        : ""}
                    </div>
                    <div className={s.detailMeta}>
                      {MANIFEST_LABELS[selectedPlugin.manifestKind]} · 载荷目录:{" "}
                      {selectedPlugin.sourceDirPath}
                    </div>
                  </div>

                  <div className={s.actions}>
                    <FolderPicker kind="plugin" itemId={selectedPlugin.id} />
                    <button
                      type="button"
                      className={s.btn}
                      onClick={() =>
                        void showInFinder(selectedPlugin.sourceDirPath)
                      }
                    >
                      <ExternalLink size={13} /> 打开目录
                    </button>
                    <button
                      type="button"
                      className={`${s.btn} ${s.btnDanger}`}
                      onClick={async () => {
                        const ok = await confirm({
                          title: `删除插件「${selectedPlugin.name}」？`,
                          message:
                            "会一并卸载并删除它投射出的 Skills、Commands 和 MCP，并同步到备份仓库。",
                          confirmLabel: "删除",
                          danger: true,
                        });
                        if (ok) {
                          setSelectedId(null);
                          void remove(selectedPlugin.id);
                        }
                      }}
                    >
                      <Trash2 size={13} /> 原子删除
                    </button>
                  </div>
                </div>

                {/* Bundled Components Overview */}
                <div className={s.sectionCard}>
                  <div className={s.sectionHeader}>
                    <span className={s.sectionTitle}>
                      <Layers size={15} /> 内含组件清单与联动导航
                    </span>
                    <div className={s.actions}>
                      {selectedPlugin.projectedSkillIds.length > 0 && (
                        <button
                          type="button"
                          className={s.btn}
                          onClick={() => onNavigate("my-library")}
                        >
                          查看投射 Skills (
                          {selectedPlugin.projectedSkillIds.length})
                        </button>
                      )}
                      {selectedPlugin.projectedMcpServerIds.length > 0 && (
                        <button
                          type="button"
                          className={s.btn}
                          onClick={() => onNavigate("mcp-servers")}
                        >
                          查看捆绑 MCP (
                          {selectedPlugin.projectedMcpServerIds.length})
                        </button>
                      )}
                    </div>
                  </div>

                  <div className={s.noticeBanner}>
                    跨端双模编排：在同构客户端（Claude / Codex / Cursor /
                    Antigravity /
                    Gemini）挂载原生插件目录并投射组件；在异构客户端自动展开{" "}
                    <code>{"${CLAUDE_PLUGIN_ROOT}"}</code> 宏后投射 Skills 与
                    MCP。
                  </div>

                  <div className={s.componentGrid}>
                    <div className={s.componentBox}>
                      <span className={s.componentLabel}>
                        Skills ({selectedPlugin.components.skills.length})
                      </span>
                      <span className={s.componentList}>
                        {selectedPlugin.components.skills.join(", ") || "无"}
                      </span>
                    </div>
                    <div className={s.componentBox}>
                      <span className={s.componentLabel}>
                        MCP Servers (
                        {selectedPlugin.components.mcpServers.length})
                      </span>
                      <span className={s.componentList}>
                        {selectedPlugin.components.mcpServers.join(", ") ||
                          "无"}
                      </span>
                    </div>
                    <div className={s.componentBox}>
                      <span className={s.componentLabel}>
                        Commands ({selectedPlugin.components.commands.length})
                      </span>
                      <span className={s.componentList}>
                        {selectedPlugin.components.commands.join(", ") || "无"}
                      </span>
                    </div>
                    <div className={s.componentBox}>
                      <span className={s.componentLabel}>
                        Hooks / Agents / Rules
                      </span>
                      <span className={s.componentList}>
                        {[
                          ...selectedPlugin.components.hooks,
                          ...selectedPlugin.components.agents,
                          ...selectedPlugin.components.rules,
                        ].join(", ") || "无"}
                      </span>
                    </div>
                  </div>
                </div>

                {/* userConfig & Secrets */}
                {selectedPlugin.userConfigSchema.length > 0 && (
                  <div className={s.sectionCard}>
                    <div className={s.sectionHeader}>
                      <span className={s.sectionTitle}>
                        <Lock size={15} /> 插件参数与敏感凭据 (userConfig)
                      </span>
                      <button
                        type="button"
                        className={s.btnPrimary}
                        onClick={() =>
                          void handleSaveUserConfig(selectedPlugin)
                        }
                      >
                        保存插件配置
                      </button>
                    </div>

                    {selectedPlugin.userConfigSchema.map((field) => {
                      const hasSecret =
                        selectedPlugin.configuredSecretKeys.includes(field.key);
                      return (
                        <div key={field.key} className={s.configRow}>
                          <span className={s.configKey}>
                            {field.title} ({field.key})
                          </span>
                          {field.sensitive ? (
                            <input
                              type="password"
                              className={s.input}
                              value={secretDrafts[field.key] ?? ""}
                              onChange={(e) =>
                                setSecretDrafts((prev) => ({
                                  ...prev,
                                  [field.key]: e.target.value,
                                }))
                              }
                              placeholder={
                                hasSecret
                                  ? "本机已配置敏感凭据（输入新值可覆盖）"
                                  : "输入本机私密凭据（不进 Git 备份）"
                              }
                            />
                          ) : (
                            <input
                              className={s.input}
                              value={configDrafts[field.key] ?? ""}
                              onChange={(e) =>
                                setConfigDrafts((prev) => ({
                                  ...prev,
                                  [field.key]: e.target.value,
                                }))
                              }
                              placeholder={
                                field.defaultValue ?? field.description ?? ""
                              }
                            />
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}

                {/* Global Per-App Switches */}
                <div className={s.sectionCard}>
                  <div className={s.sectionTitle}>全局整包启停</div>
                  <div className={s.appGrid}>
                    {visibleSkillApps.map((app) => {
                      const enabled = (
                        globalEnabledByPlugin[selectedPlugin.id] ?? []
                      ).includes(app.id);
                      const isNativeCapable = NATIVE_CAPABLE_APPS.has(app.id);
                      return (
                        <div key={app.id} className={s.appRow}>
                          <div className={s.appInfo}>
                            <span className={s.appName}>
                              <span
                                className={s.appDot}
                                style={{ background: app.accentColor }}
                              />
                              {app.label}
                            </span>
                            <span className={s.appModeHint}>
                              {isNativeCapable
                                ? "原生挂载 + 组件投射"
                                : "跨端解包投射 (Skills/MCP)"}
                            </span>
                          </div>
                          <button
                            type="button"
                            role="switch"
                            aria-checked={enabled}
                            className={`${s.switchBtn} ${enabled ? s.switchBtnOn : ""}`}
                            onClick={() =>
                              void togglePluginGlobalApp(
                                selectedPlugin.id,
                                app.id
                              )
                            }
                          >
                            <span
                              className={`${s.switchThumb} ${enabled ? s.switchThumbOn : ""}`}
                            />
                          </button>
                        </div>
                      );
                    })}
                  </div>
                </div>

                {/* Project Scope */}
                <div className={s.sectionCard}>
                  <div className={s.sectionHeader}>
                    <span className={s.sectionTitle}>项目级整包组件装配</span>
                    <div className={s.actions}>
                      <select
                        className={s.select}
                        value={claudeMcpMode}
                        onChange={(e) =>
                          setClaudeMcpMode(
                            e.target.value as ClaudeProjectMcpMode
                          )
                        }
                      >
                        <option value="local">
                          Claude MCP: local 本机私有
                        </option>
                        <option value="project">
                          Claude MCP: project (.mcp.json)
                        </option>
                      </select>
                      <button
                        type="button"
                        className={s.btn}
                        onClick={async () => {
                          try {
                            const p = await open({
                              directory: true,
                              multiple: false,
                            });
                            if (p) setSelectedProjectPath(p as string);
                          } catch {
                            // ignore
                          }
                        }}
                      >
                        <FolderPlus size={13} /> 选择项目
                      </button>
                    </div>
                  </div>

                  <div className={s.actions}>
                    {projects.filter((p) => p.path).length > 0 && (
                      <select
                        className={s.select}
                        value={selectedProjectPath}
                        onChange={(e) => setSelectedProjectPath(e.target.value)}
                      >
                        <option value="">-- 选择已保存项目 --</option>
                        {projects
                          .filter((p) => p.path)
                          .map((proj) => (
                            <option key={proj.id} value={proj.path ?? ""}>
                              {proj.name}
                            </option>
                          ))}
                      </select>
                    )}
                    <input
                      className={s.input}
                      value={selectedProjectPath}
                      onChange={(e) => setSelectedProjectPath(e.target.value)}
                      placeholder="输入或选择项目本地根目录…"
                    />
                  </div>

                  {selectedProjectPath.trim() && (
                    <div className={s.appGrid}>
                      {projectStatuses.map((st) => {
                        const meta = getAppMeta(st.appId);
                        if (!meta) return null;
                        return (
                          <div key={st.appId} className={s.appRow}>
                            <div className={s.appInfo}>
                              <span className={s.appName}>
                                <span
                                  className={s.appDot}
                                  style={{ background: meta.accentColor }}
                                />
                                {meta.label}
                              </span>
                              <span className={s.appModeHint}>
                                {st.details ?? "组件投射"}
                              </span>
                            </div>
                            <button
                              type="button"
                              role="switch"
                              aria-checked={st.installed}
                              className={`${s.switchBtn} ${st.installed ? s.switchBtnOn : ""}`}
                              onClick={() =>
                                void handleToggleProjectApp(
                                  selectedPlugin,
                                  st.appId,
                                  st.installed
                                )
                              }
                            >
                              <span
                                className={`${s.switchThumb} ${st.installed ? s.switchThumbOn : ""}`}
                              />
                            </button>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              </section>
            </Drawer>
          )}
        </div>
      )}

      {showGitModal && (
        <div
          className={s.modalOverlay}
          data-modal-overlay
          onClick={() => !importingGit && setShowGitModal(false)}
        >
          <div className={s.modal} onClick={(e) => e.stopPropagation()}>
            <div className={s.detailTitle}>从 Git 仓库导入插件包</div>
            <div className={s.modalHint}>
              支持仓库地址、<code>owner/repo</code> 简写或子目录链接（如{" "}
              <code>google-labs-code/stitch-skills</code>
              ）；包含多个插件的 Marketplace 仓库将自动识别供勾选导入。
            </div>
            <input
              className={s.input}
              value={gitRepoUrl}
              onChange={(e) => setGitRepoUrl(e.target.value)}
              placeholder="https://github.com/owner/repo 或 owner/repo"
            />
            <input
              className={s.input}
              value={gitSubPath}
              onChange={(e) => setGitSubPath(e.target.value)}
              placeholder="仓库内插件子路径（选填，留空可自动识别全部插件）"
            />
            <input
              className={s.input}
              value={gitBranch}
              onChange={(e) => setGitBranch(e.target.value)}
              placeholder="分支名（选填，默认远端默认分支）"
            />
            <div className={s.modalFooter}>
              <button
                type="button"
                className={s.btn}
                disabled={importingGit}
                onClick={() => setShowGitModal(false)}
              >
                取消
              </button>
              <button
                type="button"
                className={s.btnPrimary}
                disabled={importingGit}
                onClick={() => void handleImportGit()}
              >
                {importingGit ? "正在克隆与解析…" : "确认导入"}
              </button>
            </div>
          </div>
        </div>
      )}

      {pendingSelection && (
        <div
          className={s.modalOverlay}
          data-modal-overlay
          onClick={() => void handleCancelMultiSelect()}
        >
          <div
            className={`${s.modal} ${s.modalWide}`}
            onClick={(e) => e.stopPropagation()}
          >
            <div className={s.selectionHeader}>
              <div>
                <div className={s.detailTitle}>
                  检测到插件合集
                  {pendingSelection.marketplaceName
                    ? ` · ${pendingSelection.marketplaceName}`
                    : ""}
                </div>
                <div className={s.detailMeta}>
                  共发现 {pendingSelection.candidates.length} 个可导入插件包 ·
                  已勾选 {selectedSubPaths.length} 个
                </div>
              </div>
              <button
                type="button"
                className={s.btn}
                disabled={importingStaged}
                onClick={handleToggleAllFiltered}
              >
                {filteredCandidates.length > 0 &&
                filteredCandidates.every((c) =>
                  selectedSubPaths.includes(c.subPath)
                )
                  ? "取消全选"
                  : "全选当前"}
              </button>
            </div>

            {pendingSelection.candidates.length > 4 && (
              <div className={s.candidateSearchWrap}>
                <Search size={14} className={s.searchIcon} />
                <input
                  className={s.searchInput}
                  value={candidateQuery}
                  onChange={(e) => setCandidateQuery(e.target.value)}
                  placeholder="按插件名、路径或描述筛选…"
                />
              </div>
            )}

            <div className={s.candidateList}>
              {filteredCandidates.map((cand) => {
                const checked = selectedSubPaths.includes(cand.subPath);
                return (
                  <button
                    key={cand.subPath}
                    type="button"
                    className={`${s.candidateCard} ${checked ? s.candidateCardChecked : ""}`}
                    onClick={() => toggleCandidatePath(cand.subPath)}
                  >
                    <span
                      className={`${s.checkbox} ${checked ? s.checkboxChecked : ""}`}
                    >
                      {checked && <Check size={12} />}
                    </span>
                    <div className={s.candidateBody}>
                      <div className={s.cardTop}>
                        <span className={s.cardTitle}>
                          {cand.name}
                          {cand.version ? ` v${cand.version}` : ""}
                        </span>
                        <span className={s.manifestBadge}>
                          {MANIFEST_LABELS[cand.manifestKind]}
                        </span>
                      </div>
                      {cand.description && (
                        <div className={s.cardDesc}>{cand.description}</div>
                      )}
                      <div className={s.pillRow}>
                        <span className={s.pathPill}>{cand.subPath}</span>
                        <span className={s.pill}>
                          Skills {cand.components.skills.length}
                        </span>
                        {cand.components.mcpServers.length > 0 && (
                          <span className={s.pill}>
                            MCP {cand.components.mcpServers.length}
                          </span>
                        )}
                        {cand.components.commands.length > 0 && (
                          <span className={s.pill}>
                            Commands {cand.components.commands.length}
                          </span>
                        )}
                        {cand.components.hooks.length > 0 && (
                          <span className={s.pill}>
                            Hooks {cand.components.hooks.length}
                          </span>
                        )}
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>

            <div className={s.modalFooter}>
              <button
                type="button"
                className={s.btn}
                disabled={importingStaged}
                onClick={() => void handleCancelMultiSelect()}
              >
                取消
              </button>
              <button
                type="button"
                className={s.btnPrimary}
                disabled={importingStaged || selectedSubPaths.length === 0}
                onClick={() => void handleConfirmMultiSelect()}
              >
                {importingStaged
                  ? `正在导入 ${selectedSubPaths.length} 个插件…`
                  : `导入选中的 ${selectedSubPaths.length} 个插件`}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
