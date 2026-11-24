import { useState, useMemo, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Plus,
  Search,
  Lock,
  ShieldCheck,
  Trash2,
  Edit3,
  Download,
  FolderPlus,
  RefreshCw,
  CheckCircle2,
  AlertCircle,
} from "lucide-react";
import type { McpMode } from "../App";
import { useMcp } from "../context/McpContext";
import { useApps } from "../context/AppContext";
import { useProjects } from "../context/ProjectContext";
import { useToast } from "../components/ui/Toast";
import type {
  ClaudeProjectMcpMode,
  CreateMcpServerInput,
  ExternalMcpServer,
  McpAppInstallStatus,
  McpEnvVarSpec,
  McpServer,
  McpTransport,
} from "../types";
import s from "./McpServersPage.module.css";
import { Drawer } from "../components/ui/Drawer";
import { FolderedGrid, FolderPicker } from "../components/ui/FolderedGrid";
import { useConfirm } from "../components/ui/ConfirmDialog";

interface Props {
  activeMode: McpMode;
  onChangeMode: (mode: McpMode) => void;
}

interface EnvDraftRow {
  name: string;
  value: string;
  isSecret: boolean;
  required: boolean;
}

export function McpServersPage({ activeMode, onChangeMode }: Props) {
  const {
    servers,
    externalServers,
    globalEnabledByServer,
    refresh,
    toggleServerGlobalApp,
    create,
    update,
    remove,
    importExternal,
    installToProject,
    uninstallFromProject,
    checkStatus,
  } = useMcp();
  const { allApps, visibleMcpApps, getAppMeta } = useApps();
  const { projects } = useProjects();
  const toast = useToast();
  const confirm = useConfirm();

  const [query, setQuery] = useState("");
  const [transportFilter, setTransportFilter] = useState<"all" | McpTransport>(
    "all"
  );
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // Secret vault inline edits
  const [secretInputs, setSecretInputs] = useState<Record<string, string>>({});

  // Project-level controls
  const [selectedProjectPath, setSelectedProjectPath] = useState<string>("");
  const [claudeMode, setClaudeMode] = useState<ClaudeProjectMcpMode>("local");
  const [projectStatuses, setProjectStatuses] = useState<McpAppInstallStatus[]>(
    []
  );

  // Create / Edit modal state
  const [showModal, setShowModal] = useState(false);
  const [editingServer, setEditingServer] = useState<McpServer | null>(null);
  const [formName, setFormName] = useState("");
  const [formDesc, setFormDesc] = useState("");
  const [formTransport, setFormTransport] = useState<McpTransport>("stdio");
  const [formCommand, setFormCommand] = useState("npx");
  const [formArgsText, setFormArgsText] = useState("");
  const [formUrl, setFormUrl] = useState("");
  const [formTags, setFormTags] = useState("");
  const [formEnvRows, setFormEnvRows] = useState<EnvDraftRow[]>([]);
  const [saving, setSaving] = useState(false);

  const filteredServers = useMemo(() => {
    const q = query.trim().toLowerCase();
    return servers.filter((srv) => {
      if (transportFilter !== "all" && srv.transport !== transportFilter) {
        return false;
      }
      if (!q) return true;
      return (
        srv.name.toLowerCase().includes(q) ||
        srv.slug.toLowerCase().includes(q) ||
        (srv.description ?? "").toLowerCase().includes(q) ||
        srv.tags.some((t) => t.toLowerCase().includes(q))
      );
    });
  }, [query, servers, transportFilter]);

  // Details open only on click, in a drawer; closes if the server disappears.
  const selectedServer = useMemo(
    () => servers.find((item) => item.id === selectedId) ?? null,
    [servers, selectedId]
  );

  useEffect(() => {
    if (!selectedProjectPath && projects.length > 0) {
      const firstWithPath = projects.find((p) => p.path);
      if (firstWithPath?.path) {
        setSelectedProjectPath(firstWithPath.path);
      }
    }
  }, [projects, selectedProjectPath]);

  const refreshProjectStatus = useCallback(async () => {
    if (!selectedServer || !selectedProjectPath.trim()) {
      setProjectStatuses([]);
      return;
    }
    const res = await checkStatus({
      serverId: selectedServer.id,
      scope: "project",
      projectPath: selectedProjectPath.trim(),
    });
    if (res.ok) {
      setProjectStatuses(res.value.statuses);
    }
  }, [checkStatus, selectedProjectPath, selectedServer]);

  useEffect(() => {
    void refreshProjectStatus();
  }, [refreshProjectStatus]);

  const openCreateModal = () => {
    setEditingServer(null);
    setFormName("");
    setFormDesc("");
    setFormTransport("stdio");
    setFormCommand("npx");
    setFormArgsText("-y\n@modelcontextprotocol/server-filesystem");
    setFormUrl("");
    setFormTags("");
    setFormEnvRows([]);
    setShowModal(true);
  };

  const openEditModal = (srv: McpServer) => {
    setEditingServer(srv);
    setFormName(srv.name);
    setFormDesc(srv.description ?? "");
    setFormTransport(srv.transport);
    setFormCommand(srv.command ?? "");
    setFormArgsText(srv.args.join("\n"));
    setFormUrl(srv.url ?? "");
    setFormTags(srv.tags.join(", "));
    setFormEnvRows(
      srv.envSpecs.map((spec) => ({
        name: spec.name,
        value: spec.isSecret ? "" : (spec.defaultValue ?? ""),
        isSecret: spec.isSecret,
        required: spec.required,
      }))
    );
    setShowModal(true);
  };

  const handleSaveModal = async () => {
    if (!formName.trim()) {
      toast.error("请填写 MCP 服务名称");
      return;
    }
    setSaving(true);
    const args = formArgsText
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
    const tags = formTags
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);

    const envSpecs: McpEnvVarSpec[] = [];
    const secretValues: Record<string, string> = {};

    for (const row of formEnvRows) {
      const key = row.name.trim();
      if (!key) continue;
      envSpecs.push({
        name: key,
        required: row.required,
        isSecret: row.isSecret,
        defaultValue: row.isSecret ? null : row.value.trim() || null,
      });
      if (row.isSecret && row.value.trim()) {
        secretValues[key] = row.value.trim();
      }
    }

    if (editingServer) {
      const res = await update({
        id: editingServer.id,
        name: formName.trim(),
        description: formDesc.trim(),
        transport: formTransport,
        command: formTransport === "stdio" ? formCommand.trim() : "",
        args: formTransport === "stdio" ? args : [],
        url: formTransport !== "stdio" ? formUrl.trim() : "",
        envSpecs,
        secretValues:
          Object.keys(secretValues).length > 0 ? secretValues : null,
        tags,
      });
      setSaving(false);
      if (res.ok) {
        toast.success("已更新 MCP 服务配置");
        setShowModal(false);
      } else {
        toast.error(res.error);
      }
      return;
    }

    const input: CreateMcpServerInput = {
      name: formName.trim(),
      description: formDesc.trim() || null,
      transport: formTransport,
      command: formTransport === "stdio" ? formCommand.trim() : null,
      args: formTransport === "stdio" ? args : [],
      url: formTransport !== "stdio" ? formUrl.trim() : null,
      envSpecs,
      headerSpecs: [],
      secretValues,
      tags,
      projectIds: [],
    };
    const res = await create(input);
    setSaving(false);
    if (res.ok) {
      setSelectedId(res.value.server.id);
      toast.success("已创建 MCP 服务");
      setShowModal(false);
    } else {
      toast.error(res.error);
    }
  };

  const handleSaveInlineSecrets = async (srv: McpServer) => {
    if (Object.keys(secretInputs).length === 0) return;
    const res = await update({
      id: srv.id,
      secretValues: secretInputs,
    });
    if (res.ok) {
      setSecretInputs({});
      toast.success("本机密钥已加密/权限隔离保存（不会上传 Git 备份）");
    } else {
      toast.error(res.error);
    }
  };

  const handlePickProjectFolder = async () => {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "选择项目目录以装配 MCP 服务",
      });
      if (picked) {
        setSelectedProjectPath(picked as string);
      }
    } catch {
      // cancelled
    }
  };

  const handleToggleProjectApp = async (
    srv: McpServer,
    appId: string,
    currentlyInstalled: boolean
  ) => {
    if (!selectedProjectPath.trim()) {
      toast.error("请先选择目标项目目录");
      return;
    }
    const appLabel = getAppMeta(appId)?.label ?? appId;
    const res = currentlyInstalled
      ? await uninstallFromProject({
          serverId: srv.id,
          projectPath: selectedProjectPath.trim(),
          apps: [appId],
          claudeMode,
        })
      : await installToProject({
          serverId: srv.id,
          projectPath: selectedProjectPath.trim(),
          apps: [appId],
          claudeMode,
        });

    if (res.ok && res.value.installedApps.includes(appId)) {
      toast.success(
        currentlyInstalled
          ? `已从项目 ${appLabel} 移除 MCP`
          : `已向项目 ${appLabel} 装配 MCP`
      );
      if (res.value.warnings.length > 0) {
        toast.info(res.value.warnings[0], 4500);
      }
      await refreshProjectStatus();
    } else {
      const err = !res.ok
        ? res.error
        : (res.value.warnings[0] ?? "操作项目配置失败");
      toast.error(err);
    }
  };

  const handleImportExternal = async (ext: ExternalMcpServer) => {
    const res = await importExternal({
      appId: ext.appId,
      configPath: ext.configPath,
      slug: ext.slug,
      importSecretsToLocal: true,
    });
    if (res.ok) {
      toast.success(
        `已安全导入「${ext.name}」（敏感凭据已自动分离至本机密钥库）`
      );
      setSelectedId(res.value.server.id);
      onChangeMode("managed");
    } else {
      toast.error(`导入失败：${res.error}`);
    }
  };

  const t3Meta = allApps.find((a) => a.id === "t3");

  return (
    <div className={s.page}>
      <header className={s.header}>
        <div className={s.headerTitleBlock}>
          <h1 className={s.title}>MCP Servers</h1>
          <p className={s.subtitle}>
            跨客户端 JSON / TOML 键值级无损合并，敏感凭据本机 0600
            隔离（绝不进入 Git 备份）
          </p>
        </div>

        <div className={s.headerActions}>
          <div className={s.modeTabs}>
            <button
              type="button"
              className={`${s.modeTab} ${activeMode === "managed" ? s.modeTabActive : ""}`}
              onClick={() => onChangeMode("managed")}
            >
              托管服务 ({servers.length})
            </button>
            <button
              type="button"
              className={`${s.modeTab} ${activeMode === "external" ? s.modeTabActive : ""}`}
              onClick={() => onChangeMode("external")}
            >
              外部发现 ({externalServers.length})
            </button>
          </div>

          <button
            type="button"
            className={s.btn}
            onClick={() => void refresh()}
            title="刷新状态与外部扫描"
          >
            <RefreshCw size={13} /> 刷新
          </button>

          <button
            type="button"
            className={s.btnPrimary}
            onClick={openCreateModal}
          >
            <Plus size={14} /> 新建 MCP
          </button>
        </div>
      </header>

      {activeMode === "managed" ? (
        <>
          <div className={s.toolbar}>
            <div className={s.searchWrap}>
              <Search size={14} className={s.searchIcon} />
              <input
                className={s.searchInput}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                data-search-input
                placeholder="搜索 MCP 名称、标识或标签…"
              />
            </div>

            <div className={s.filterGroup}>
              {(["all", "stdio", "http", "sse"] as const).map((t) => (
                <button
                  key={t}
                  type="button"
                  className={`${s.filterChip} ${transportFilter === t ? s.filterChipActive : ""}`}
                  onClick={() => setTransportFilter(t)}
                >
                  {t === "all" ? "全部协议" : t.toUpperCase()}
                </button>
              ))}
            </div>
          </div>

          {filteredServers.length === 0 ? (
            <div className={s.emptyState}>
              <span>暂无托管的 MCP Server</span>
              <div className={s.btnRow}>
                <button
                  type="button"
                  className={s.btnPrimary}
                  onClick={openCreateModal}
                >
                  <Plus size={13} /> 手动新建 MCP
                </button>
                {externalServers.length > 0 && (
                  <button
                    type="button"
                    className={s.btn}
                    onClick={() => onChangeMode("external")}
                  >
                    从外部发现导入 ({externalServers.length})
                  </button>
                )}
              </div>
            </div>
          ) : (
            <div className={s.body}>
              <div className={s.listPane}>
                <FolderedGrid
                  kind="mcp"
                  items={filteredServers}
                  getId={(srv) => srv.id}
                  getLabel={(srv) => srv.name}
                  searching={!!query.trim()}
                  empty={null}
                  renderItem={(srv) => {
                    const enabledApps = globalEnabledByServer[srv.id] ?? [];
                    const secretSpecs = srv.envSpecs.filter((e) => e.isSecret);
                    const missingSecrets = secretSpecs.filter(
                      (e) => !srv.configuredSecretKeys.includes(e.name)
                    );
                    return (
                      <button
                        key={srv.id}
                        type="button"
                        className={`${s.serverCard} ${selectedServer?.id === srv.id ? s.serverCardActive : ""}`}
                        onClick={() => setSelectedId(srv.id)}
                      >
                        <div className={s.cardTop}>
                          <span className={s.cardTitle}>{srv.name}</span>
                          <span className={s.transportBadge}>
                            {srv.transport}
                          </span>
                        </div>
                        {srv.description && (
                          <div className={s.cardDesc}>{srv.description}</div>
                        )}
                        <div className={s.cardMetaRow}>
                          <span className={s.metaPill}>
                            全局已启 {enabledApps.length} 端
                          </span>
                          {srv.pluginId && (
                            <span className={s.metaPill}>插件提供</span>
                          )}
                          {secretSpecs.length > 0 && (
                            <span
                              className={`${s.metaPill} ${missingSecrets.length > 0 ? s.metaPillWarn : s.metaPillSuccess}`}
                            >
                              <Lock size={10} />
                              {missingSecrets.length > 0
                                ? `待配密钥 ${missingSecrets.length}`
                                : "本机密钥就绪"}
                            </span>
                          )}
                        </div>
                      </button>
                    );
                  }}
                />
              </div>

              {selectedServer && (
                <Drawer
                  label={selectedServer.name}
                  onClose={() => setSelectedId(null)}
                >
                  <section className={s.detailPane}>
                    <div className={s.detailHeader}>
                      <div>
                        <div className={s.detailTitle}>
                          {selectedServer.name}
                        </div>
                        <div className={s.detailSlug}>
                          标识: {selectedServer.slug} · 协议:{" "}
                          {selectedServer.transport.toUpperCase()}
                        </div>
                      </div>

                      <div className={s.btnRow}>
                        <FolderPicker kind="mcp" itemId={selectedServer.id} />
                        <button
                          type="button"
                          className={s.btn}
                          onClick={() => openEditModal(selectedServer)}
                        >
                          <Edit3 size={13} /> 编辑规格
                        </button>
                        <button
                          type="button"
                          className={`${s.btn} ${s.btnDanger}`}
                          onClick={async () => {
                            const ok = await confirm({
                              title: `删除 MCP「${selectedServer.name}」？`,
                              message:
                                "会从已启用的全局客户端中移除，并同步到备份仓库。",
                              confirmLabel: "删除",
                              danger: true,
                            });
                            if (ok) {
                              setSelectedId(null);
                              void remove(selectedServer.id);
                            }
                          }}
                        >
                          <Trash2 size={13} /> 删除
                        </button>
                      </div>
                    </div>

                    {/* Connection Spec */}
                    <div className={s.sectionCard}>
                      <div className={s.sectionTitle}>运行入口与参数</div>
                      <div className={s.codeBox}>
                        {selectedServer.transport === "stdio"
                          ? `${selectedServer.command ?? ""} ${selectedServer.args.join(" ")}`
                          : (selectedServer.url ?? "")}
                      </div>
                    </div>

                    {/* Secret Isolation Vault */}
                    {selectedServer.envSpecs.length > 0 && (
                      <div className={s.sectionCard}>
                        <div className={s.sectionTitleRow}>
                          <span className={s.sectionTitle}>
                            <ShieldCheck size={15} /> 环境变量与本机私密凭据
                          </span>
                          {Object.keys(secretInputs).length > 0 && (
                            <button
                              type="button"
                              className={s.btnPrimary}
                              onClick={() =>
                                void handleSaveInlineSecrets(selectedServer)
                              }
                            >
                              保存本机密钥
                            </button>
                          )}
                        </div>

                        <div className={s.securityNotice}>
                          <Lock size={13} />
                          <span>
                            标记为敏感的变量值仅保存在本机{" "}
                            <code>local-secrets.json</code>（权限
                            0600），绝不写入 Git
                            备份仓；写入共享项目配置时自动转为{" "}
                            <code>{"${ENV_VAR}"}</code> 占位符。
                          </span>
                        </div>

                        {selectedServer.envSpecs.map((spec) => {
                          const isConfigured =
                            selectedServer.configuredSecretKeys.includes(
                              spec.name
                            );
                          return (
                            <div key={spec.name} className={s.secretRow}>
                              <span className={s.secretKey}>{spec.name}</span>
                              {spec.isSecret ? (
                                <>
                                  <span
                                    className={`${s.metaPill} ${isConfigured ? s.metaPillSuccess : s.metaPillWarn}`}
                                  >
                                    {isConfigured ? (
                                      <>
                                        <CheckCircle2 size={11} /> 已配置
                                      </>
                                    ) : (
                                      <>
                                        <AlertCircle size={11} /> 未配置
                                      </>
                                    )}
                                  </span>
                                  <input
                                    type="password"
                                    className={s.input}
                                    value={secretInputs[spec.name] ?? ""}
                                    onChange={(e) =>
                                      setSecretInputs((prev) => ({
                                        ...prev,
                                        [spec.name]: e.target.value,
                                      }))
                                    }
                                    placeholder={
                                      isConfigured
                                        ? "输入新值以覆盖本机密钥…"
                                        : "输入本机 API Key / Token…"
                                    }
                                  />
                                </>
                              ) : (
                                <span className={s.appPath}>
                                  默认值: {spec.defaultValue || "(空)"}
                                </span>
                              )}
                            </div>
                          );
                        })}
                      </div>
                    )}

                    {/* Global Per-App Switches */}
                    <div className={s.sectionCard}>
                      <div className={s.sectionTitle}>
                        全局多客户端无损启用（保留各端原有配置与注释）
                      </div>
                      <div className={s.appGrid}>
                        {visibleMcpApps.map((app) => {
                          const enabled = (
                            globalEnabledByServer[selectedServer.id] ?? []
                          ).includes(app.id);
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
                              </div>
                              <button
                                type="button"
                                role="switch"
                                aria-checked={enabled}
                                className={`${s.switchBtn} ${enabled ? s.switchBtnOn : ""}`}
                                onClick={() =>
                                  void toggleServerGlobalApp(
                                    selectedServer.id,
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

                        {t3Meta && (
                          <div
                            className={s.appRow}
                            title={t3Meta.mcpUnsupportedNote ?? ""}
                          >
                            <div className={s.appInfo}>
                              <span className={s.appName}>
                                <span
                                  className={s.appDot}
                                  style={{ background: t3Meta.accentColor }}
                                />
                                {t3Meta.label}
                              </span>
                              <span className={s.appPath}>
                                随底层 Claude / Codex 会话生效
                              </span>
                            </div>
                            <span className={s.metaPill}>宿主委托</span>
                          </div>
                        )}
                      </div>
                    </div>

                    {/* Project-Level Scope */}
                    <div className={s.sectionCard}>
                      <div className={s.sectionTitleRow}>
                        <span className={s.sectionTitle}>
                          项目级环境定向装配
                        </span>
                        <div className={s.btnRow}>
                          <select
                            className={s.select}
                            value={claudeMode}
                            onChange={(e) =>
                              setClaudeMode(
                                e.target.value as ClaudeProjectMcpMode
                              )
                            }
                            title="选择 Claude Code 项目级作用域"
                          >
                            <option value="local">
                              Claude: local 本机私有 (~/.claude.json)
                            </option>
                            <option value="project">
                              Claude: project 团队共享 (.mcp.json 仅占位符)
                            </option>
                          </select>
                          <button
                            type="button"
                            className={s.btn}
                            onClick={() => void handlePickProjectFolder()}
                          >
                            <FolderPlus size={13} /> 选择项目目录
                          </button>
                        </div>
                      </div>

                      <div className={s.btnRow}>
                        {projects.filter((p) => p.path).length > 0 && (
                          <select
                            className={s.select}
                            value={selectedProjectPath}
                            onChange={(e) =>
                              setSelectedProjectPath(e.target.value)
                            }
                          >
                            <option value="">-- 选择已保存项目 --</option>
                            {projects
                              .filter((p) => p.path)
                              .map((proj) => (
                                <option key={proj.id} value={proj.path ?? ""}>
                                  {proj.name} ({proj.path})
                                </option>
                              ))}
                          </select>
                        )}
                        <input
                          className={s.input}
                          value={selectedProjectPath}
                          onChange={(e) =>
                            setSelectedProjectPath(e.target.value)
                          }
                          placeholder="输入或选择项目本地根目录路径…"
                        />
                      </div>

                      {selectedProjectPath.trim() && (
                        <div className={s.appGrid}>
                          {projectStatuses
                            .filter((st) => st.supported)
                            .map((st) => {
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
                                    <span className={s.appPath}>
                                      {st.scopeDetail ??
                                        meta.mcpProjectPathLabel}
                                    </span>
                                  </div>
                                  <button
                                    type="button"
                                    role="switch"
                                    aria-checked={st.installed}
                                    className={`${s.switchBtn} ${st.installed ? s.switchBtnOn : ""}`}
                                    onClick={() =>
                                      void handleToggleProjectApp(
                                        selectedServer,
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
        </>
      ) : externalServers.length === 0 ? (
        <div className={s.emptyState}>
          <span>未在各客户端配置文件中发现尚未托管的外部 MCP 服务</span>
        </div>
      ) : (
        <div className={s.externalGrid}>
          {externalServers.map((ext) => {
            const appMeta = getAppMeta(ext.appId);
            return (
              <div
                key={`${ext.appId}:${ext.slug}:${ext.configPath}`}
                className={s.sectionCard}
              >
                <div className={s.cardTop}>
                  <span className={s.cardTitle}>{ext.name}</span>
                  <span className={s.transportBadge}>{ext.transport}</span>
                </div>
                <div className={s.appPath}>
                  来源: {appMeta?.label ?? ext.appId} · {ext.configPath}
                </div>
                <div className={s.codeBox}>
                  {ext.transport === "stdio"
                    ? `${ext.command ?? ""} ${ext.args.join(" ")}`
                    : (ext.url ?? "")}
                </div>
                <div className={s.cardMetaRow}>
                  <span className={s.metaPill}>
                    环境变量 {ext.detectedEnvKeys.length} 项
                  </span>
                  {ext.detectedSecretKeys.length > 0 && (
                    <span className={`${s.metaPill} ${s.metaPillWarn}`}>
                      <Lock size={10} /> 识别敏感键{" "}
                      {ext.detectedSecretKeys.length} 项（导入时自动脱敏）
                    </span>
                  )}
                </div>
                <div className={s.btnRow}>
                  <button
                    type="button"
                    className={s.btnPrimary}
                    onClick={() => void handleImportExternal(ext)}
                  >
                    <Download size={13} /> 脱敏纳管到资产库
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {showModal && (
        <div
          className={s.modalOverlay}
          data-modal-overlay
          onClick={() => setShowModal(false)}
        >
          <div className={s.modal} onClick={(e) => e.stopPropagation()}>
            <div className={s.detailTitle}>
              {editingServer ? "编辑 MCP 服务" : "新建 MCP 服务"}
            </div>

            <div className={s.formField}>
              <label className={s.formLabel}>服务名称 *</label>
              <input
                className={s.input}
                value={formName}
                onChange={(e) => setFormName(e.target.value)}
                placeholder="例如：github / playwright / postgres"
              />
            </div>

            <div className={s.formField}>
              <label className={s.formLabel}>描述</label>
              <input
                className={s.input}
                value={formDesc}
                onChange={(e) => setFormDesc(e.target.value)}
                placeholder="简要说明该 MCP 提供的工具或数据源能力"
              />
            </div>

            <div className={s.formField}>
              <label className={s.formLabel}>传输协议</label>
              <select
                className={s.select}
                value={formTransport}
                onChange={(e) =>
                  setFormTransport(e.target.value as McpTransport)
                }
              >
                <option value="stdio">
                  stdio（本地子进程 npx / uvx / binary）
                </option>
                <option value="http">Streamable HTTP（远程服务 URL）</option>
                <option value="sse">SSE（旧版远程事件流）</option>
              </select>
            </div>

            {formTransport === "stdio" ? (
              <>
                <div className={s.formField}>
                  <label className={s.formLabel}>启动命令 (command) *</label>
                  <input
                    className={s.input}
                    value={formCommand}
                    onChange={(e) => setFormCommand(e.target.value)}
                    placeholder="例如：npx 或 uvx 或 node"
                  />
                </div>
                <div className={s.formField}>
                  <label className={s.formLabel}>启动参数 (每行一个参数)</label>
                  <textarea
                    className={s.textarea}
                    value={formArgsText}
                    onChange={(e) => setFormArgsText(e.target.value)}
                    placeholder={"-y\n@modelcontextprotocol/server-github"}
                  />
                </div>
              </>
            ) : (
              <div className={s.formField}>
                <label className={s.formLabel}>服务地址 (URL) *</label>
                <input
                  className={s.input}
                  value={formUrl}
                  onChange={(e) => setFormUrl(e.target.value)}
                  placeholder="https://mcp.example.com/mcp"
                />
              </div>
            )}

            <div className={s.formField}>
              <div className={s.sectionTitleRow}>
                <label className={s.formLabel}>
                  环境变量（勾选「敏感」则密钥仅保存在本机）
                </label>
                <button
                  type="button"
                  className={s.btn}
                  onClick={() =>
                    setFormEnvRows((prev) => [
                      ...prev,
                      {
                        name: "",
                        value: "",
                        isSecret: true,
                        required: true,
                      },
                    ])
                  }
                >
                  <Plus size={12} /> 添加变量
                </button>
              </div>

              {formEnvRows.map((row, idx) => (
                <div key={idx} className={s.envBuilderRow}>
                  <input
                    className={s.input}
                    value={row.name}
                    onChange={(e) =>
                      setFormEnvRows((prev) =>
                        prev.map((item, i) =>
                          i === idx ? { ...item, name: e.target.value } : item
                        )
                      )
                    }
                    placeholder="ENV_KEY_NAME"
                  />
                  <input
                    type={row.isSecret ? "password" : "text"}
                    className={s.input}
                    value={row.value}
                    onChange={(e) =>
                      setFormEnvRows((prev) =>
                        prev.map((item, i) =>
                          i === idx ? { ...item, value: e.target.value } : item
                        )
                      )
                    }
                    placeholder={
                      row.isSecret
                        ? editingServer
                          ? "留空保持原密钥不变"
                          : "本机私密密钥值"
                        : "非敏感默认值"
                    }
                  />
                  <label className={s.metaPill}>
                    <input
                      type="checkbox"
                      checked={row.isSecret}
                      onChange={(e) =>
                        setFormEnvRows((prev) =>
                          prev.map((item, i) =>
                            i === idx
                              ? { ...item, isSecret: e.target.checked }
                              : item
                          )
                        )
                      }
                    />
                    敏感
                  </label>
                  <button
                    type="button"
                    className={`${s.btn} ${s.btnDanger}`}
                    onClick={() =>
                      setFormEnvRows((prev) => prev.filter((_, i) => i !== idx))
                    }
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              ))}
            </div>

            <div className={s.formField}>
              <label className={s.formLabel}>标签（英文逗号分隔）</label>
              <input
                className={s.input}
                value={formTags}
                onChange={(e) => setFormTags(e.target.value)}
                placeholder="search, browser, database"
              />
            </div>

            <div className={s.btnRow}>
              <button
                type="button"
                className={s.btn}
                onClick={() => setShowModal(false)}
              >
                取消
              </button>
              <button
                type="button"
                className={s.btnPrimary}
                disabled={saving}
                onClick={() => void handleSaveModal()}
              >
                {saving ? "保存中…" : "保存"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
