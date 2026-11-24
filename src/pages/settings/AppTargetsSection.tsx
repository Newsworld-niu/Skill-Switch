import { useState, useCallback, useEffect, useMemo } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Folder,
  Layers,
  Plus,
  Trash2,
  Server,
  Edit3,
  ChevronDown,
  ChevronUp,
} from "lucide-react";
import { useSettings } from "../../context/SettingsContext";
import { useApps } from "../../context/AppContext";
import { useSkills } from "../../context/SkillContext";
import { useSlashCommands } from "../../context/SlashCommandContext";
import { useMcp } from "../../context/McpContext";
import { usePlugins } from "../../context/PluginContext";
import {
  sshCalibrateTarget,
  sshDiscoverHosts,
  sshSyncAllEnabled,
  sshTestConnection,
} from "../../services/ssh";
import type {
  CustomAppConfig,
  DiscoveredSshHost,
  SshTargetConfig,
} from "../../types";
import s from "../SettingsPage.module.css";
import { Toggle } from "./Toggle";
import { useConfirm } from "../../components/ui/ConfirmDialog";

const CUSTOM_APP_COLORS = [
  "#0ea5e9",
  "#2563eb",
  "#7c3aed",
  "#16a34a",
  "#f97316",
  "#ef4444",
];

type AppFormMode = "local" | "ssh";

type RemoteAppPresetId =
  | "claude"
  | "codex"
  | "cursor"
  | "antigravity"
  | "gemini"
  | "custom";

interface RemotePresetDef {
  id: RemoteAppPresetId;
  label: string;
  shortName: string;
  accentColor: string;
  globalSkillDir: string;
  globalCommandDir: string;
  globalMcpConfigPath: string;
  mcpFormat: "json" | "toml";
  mcpRootKey: string;
  globalPluginDir: string;
}

const REMOTE_APP_PRESETS: readonly RemotePresetDef[] = [
  {
    id: "claude",
    label: "Claude Code",
    shortName: "Claude",
    accentColor: "#2563eb",
    globalSkillDir: "~/.claude/skills",
    globalCommandDir: "~/.claude/commands",
    globalMcpConfigPath: "~/.claude.json",
    mcpFormat: "json",
    mcpRootKey: "mcpServers",
    globalPluginDir: "~/.claude/plugins/local",
  },
  {
    id: "codex",
    label: "Codex CLI",
    shortName: "Codex",
    accentColor: "#7c3aed",
    globalSkillDir: "~/.codex/skills",
    globalCommandDir: "~/.codex/commands",
    globalMcpConfigPath: "~/.codex/config.toml",
    mcpFormat: "toml",
    mcpRootKey: "mcp_servers",
    globalPluginDir: "~/.codex/plugins",
  },
  {
    id: "cursor",
    label: "Cursor",
    shortName: "Cursor",
    accentColor: "#0891b2",
    globalSkillDir: "~/.cursor/skills",
    globalCommandDir: "~/.cursor/commands",
    globalMcpConfigPath: "~/.cursor/mcp.json",
    mcpFormat: "json",
    mcpRootKey: "mcpServers",
    globalPluginDir: "~/.cursor/plugins/local",
  },
  {
    id: "antigravity",
    label: "Antigravity",
    shortName: "Antigravity",
    accentColor: "#16a34a",
    globalSkillDir: "~/.gemini/config/skills",
    globalCommandDir: "",
    globalMcpConfigPath: "~/.gemini/config/mcp_config.json",
    mcpFormat: "json",
    mcpRootKey: "mcpServers",
    globalPluginDir: "~/.gemini/config/plugins",
  },
  {
    id: "gemini",
    label: "Gemini CLI",
    shortName: "Gemini",
    accentColor: "#0284c7",
    globalSkillDir: "~/.gemini/skills",
    globalCommandDir: "~/.gemini/commands",
    globalMcpConfigPath: "~/.gemini/settings.json",
    mcpFormat: "json",
    mcpRootKey: "mcpServers",
    globalPluginDir: "~/.gemini/extensions",
  },
  {
    id: "custom",
    label: "自定义远端路径",
    shortName: "Remote",
    accentColor: "#0ea5e9",
    globalSkillDir: "",
    globalCommandDir: "",
    globalMcpConfigPath: "",
    mcpFormat: "json",
    mcpRootKey: "mcpServers",
    globalPluginDir: "",
  },
];

function isRemotePresetId(val?: string | null): val is RemoteAppPresetId {
  return REMOTE_APP_PRESETS.some((p) => p.id === val);
}

function slugifyAppId(label: string, prefix = "custom"): string {
  const base = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return base ? `${prefix}-${base}` : `${prefix}-${Date.now()}`;
}

type RowFeedback = {
  tone: "ok" | "error" | "neutral";
  text: string;
};

interface Props {
  showToast: (message: string) => void;
}

/** Settings card for managing target apps: built-ins, custom local dirs and SSH targets. */
export function AppTargetsSection({ showToast }: Props) {
  const { settings, updateSettings } = useSettings();
  const confirm = useConfirm();
  const { allApps } = useApps();
  const { refresh: refreshSkills } = useSkills();
  const { refresh: refreshCommands } = useSlashCommands();
  const { refresh: refreshMcp } = useMcp();
  const { refresh: refreshPlugins } = usePlugins();

  // Custom / SSH target form state
  const [showAddAppForm, setShowAddAppForm] = useState(false);
  const [appFormMode, setAppFormMode] = useState<AppFormMode>("local");
  const [editingAppId, setEditingAppId] = useState<string | null>(null);

  const [newAppLabel, setNewAppLabel] = useState("");
  const [newAppColor, setNewAppColor] = useState(CUSTOM_APP_COLORS[0]);
  const [newGlobalSkillDir, setNewGlobalSkillDir] = useState("");
  const [newProjectSkillDir, setNewProjectSkillDir] = useState("");
  const [newGlobalCommandDir, setNewGlobalCommandDir] = useState("");
  const [newProjectCommandDir, setNewProjectCommandDir] = useState("");
  const [newGlobalMcpPath, setNewGlobalMcpPath] = useState("");
  const [newProjectMcpPath, setNewProjectMcpPath] = useState("");
  const [newMcpFormat, setNewMcpFormat] = useState<"json" | "toml">("json");
  const [newMcpRootKey, setNewMcpRootKey] = useState("");

  // SSH-specific form state
  const [sshHost, setSshHost] = useState("");
  const [sshUser, setSshUser] = useState("");
  const [sshPort, setSshPort] = useState("");
  const [sshIdentityFile, setSshIdentityFile] = useState("");
  const [sshPreset, setSshPreset] = useState<RemoteAppPresetId>("claude");
  const [sshGlobalPluginDir, setSshGlobalPluginDir] = useState(
    "~/.claude/plugins/local"
  );
  const [sshInjectSecrets, setSshInjectSecrets] = useState(false);
  const [showSshAdvancedPaths, setShowSshAdvancedPaths] = useState(false);

  // SSH discovery & connection test state
  const [discoveredHosts, setDiscoveredHosts] = useState<DiscoveredSshHost[]>(
    []
  );
  const [discoveringHosts, setDiscoveringHosts] = useState(false);
  const [discoverError, setDiscoverError] = useState<string | null>(null);
  const [formTestingSsh, setFormTestingSsh] = useState(false);
  const [formTestStatus, setFormTestStatus] = useState<RowFeedback | null>(
    null
  );

  // Per-saved-SSH-target row actions state
  const [sshRowBusy, setSshRowBusy] = useState<
    Record<string, "test" | "calibrate" | "sync" | undefined>
  >({});
  const [sshRowFeedback, setSshRowFeedback] = useState<
    Record<string, RowFeedback | undefined>
  >({});

  const customAppMap = useMemo(() => {
    return new Map((settings.customApps ?? []).map((c) => [c.id, c]));
  }, [settings.customApps]);

  const refreshAllResourceContexts = useCallback(async () => {
    await Promise.all([
      refreshSkills(),
      refreshCommands(),
      refreshMcp(),
      refreshPlugins(),
    ]);
  }, [refreshCommands, refreshMcp, refreshPlugins, refreshSkills]);

  const loadDiscoveredSshHosts = useCallback(async () => {
    setDiscoveringHosts(true);
    setDiscoverError(null);
    const res = await sshDiscoverHosts();
    if (res.ok) {
      setDiscoveredHosts(res.value);
    } else {
      setDiscoverError(res.error);
    }
    setDiscoveringHosts(false);
  }, []);

  useEffect(() => {
    if (showAddAppForm && appFormMode === "ssh") {
      void loadDiscoveredSshHosts();
    }
  }, [appFormMode, loadDiscoveredSshHosts, showAddAppForm]);

  const hiddenSet = new Set(settings.hiddenAppIds ?? []);
  const defaultInstallSet = new Set(settings.defaultInstallAppIds ?? []);

  const applyPresetFields = useCallback(
    (
      presetId: RemoteAppPresetId,
      options?: { currentHost?: string; updateColor?: boolean }
    ) => {
      setSshPreset(presetId);
      const preset = REMOTE_APP_PRESETS.find((p) => p.id === presetId);
      if (!preset) return;

      if (presetId === "custom") {
        setShowSshAdvancedPaths(true);
        return;
      }

      setNewGlobalSkillDir(preset.globalSkillDir);
      setNewGlobalCommandDir(preset.globalCommandDir);
      setNewGlobalMcpPath(preset.globalMcpConfigPath);
      setNewMcpFormat(preset.mcpFormat);
      setNewMcpRootKey(preset.mcpRootKey);
      setSshGlobalPluginDir(preset.globalPluginDir);

      if (options?.updateColor) {
        setNewAppColor(preset.accentColor);
      }

      const hostTrimmed = (options?.currentHost ?? sshHost).trim();
      if (hostTrimmed) {
        setNewAppLabel((prev) => {
          const trimmedPrev = prev.trim();
          if (!trimmedPrev) {
            return `${hostTrimmed} · ${preset.shortName}`;
          }
          const matchedPresetSuffix = REMOTE_APP_PRESETS.some(
            (p) => trimmedPrev === `${hostTrimmed} · ${p.shortName}`
          );
          return matchedPresetSuffix
            ? `${hostTrimmed} · ${preset.shortName}`
            : prev;
        });
      }
    },
    [sshHost]
  );

  const resetFormFields = useCallback(() => {
    setEditingAppId(null);
    setNewAppLabel("");
    setNewAppColor(CUSTOM_APP_COLORS[0]);
    setNewGlobalSkillDir("");
    setNewProjectSkillDir("");
    setNewGlobalCommandDir("");
    setNewProjectCommandDir("");
    setNewGlobalMcpPath("");
    setNewProjectMcpPath("");
    setNewMcpFormat("json");
    setNewMcpRootKey("");
    setSshHost("");
    setSshUser("");
    setSshPort("");
    setSshIdentityFile("");
    setSshPreset("claude");
    setSshGlobalPluginDir("~/.claude/plugins/local");
    setSshInjectSecrets(false);
    setShowSshAdvancedPaths(false);
    setFormTestStatus(null);
  }, []);

  const handleOpenCreateLocal = () => {
    resetFormFields();
    setAppFormMode("local");
    setShowAddAppForm(true);
  };

  const handleOpenCreateSsh = () => {
    resetFormFields();
    setAppFormMode("ssh");
    const claudePreset = REMOTE_APP_PRESETS[0];
    setNewAppColor(claudePreset.accentColor);
    setNewGlobalSkillDir(claudePreset.globalSkillDir);
    setNewGlobalCommandDir(claudePreset.globalCommandDir);
    setNewGlobalMcpPath(claudePreset.globalMcpConfigPath);
    setNewMcpFormat(claudePreset.mcpFormat);
    setNewMcpRootKey(claudePreset.mcpRootKey);
    setSshGlobalPluginDir(claudePreset.globalPluginDir);
    setShowAddAppForm(true);
  };

  const handleSwitchFormMode = (nextMode: AppFormMode) => {
    setAppFormMode(nextMode);
    setFormTestStatus(null);
    if (nextMode === "ssh" && !newGlobalSkillDir.trim()) {
      applyPresetFields(sshPreset, { updateColor: editingAppId === null });
    }
  };

  const handleOpenEditCustomApp = (custom: CustomAppConfig) => {
    setEditingAppId(custom.id);
    setNewAppLabel(custom.label);
    setNewAppColor(custom.accentColor || CUSTOM_APP_COLORS[0]);
    setNewGlobalSkillDir(custom.globalSkillDir);
    setNewProjectSkillDir(custom.projectSkillDir ?? "");
    setNewGlobalCommandDir(custom.globalCommandDir ?? "");
    setNewProjectCommandDir(custom.projectCommandDir ?? "");
    setNewGlobalMcpPath(custom.globalMcpConfigPath ?? "");
    setNewProjectMcpPath(custom.projectMcpConfigPath ?? "");
    setNewMcpFormat(custom.mcpFormat ?? "json");
    setNewMcpRootKey(custom.mcpRootKey ?? "");
    setFormTestStatus(null);

    if (custom.ssh?.host?.trim()) {
      setAppFormMode("ssh");
      setSshHost(custom.ssh.host);
      setSshUser(custom.ssh.user ?? "");
      setSshPort(custom.ssh.port ? String(custom.ssh.port) : "");
      setSshIdentityFile(custom.ssh.identityFile ?? "");
      const preset = isRemotePresetId(custom.ssh.remoteAppPreset)
        ? custom.ssh.remoteAppPreset
        : "custom";
      setSshPreset(preset);
      setSshGlobalPluginDir(custom.ssh.globalPluginDir ?? "");
      setSshInjectSecrets(Boolean(custom.ssh.injectSecrets));
      setShowSshAdvancedPaths(preset === "custom");
    } else {
      setAppFormMode("local");
      setSshHost("");
      setSshUser("");
      setSshPort("");
      setSshIdentityFile("");
      setSshPreset("claude");
      setSshGlobalPluginDir("~/.claude/plugins/local");
      setSshInjectSecrets(false);
      setShowSshAdvancedPaths(false);
    }

    setShowAddAppForm(true);
  };

  const handlePickDiscoveredHost = (hostItem: DiscoveredSshHost) => {
    const prevHost = sshHost.trim();
    setSshHost(hostItem.alias);
    setFormTestStatus(null);

    const activePreset =
      REMOTE_APP_PRESETS.find((p) => p.id === sshPreset) ??
      REMOTE_APP_PRESETS[0];
    const suggested = `${hostItem.alias} · ${activePreset.shortName}`;

    setNewAppLabel((prev) => {
      const trimmed = prev.trim();
      if (!trimmed) return suggested;
      if (
        prevHost &&
        REMOTE_APP_PRESETS.some(
          (p) => trimmed === `${prevHost} · ${p.shortName}`
        )
      ) {
        return suggested;
      }
      return prev;
    });
  };

  const toggleAppVisible = async (appId: string) => {
    const current = settings.hiddenAppIds ?? [];
    const isHidden = current.includes(appId);
    const nextHidden = isHidden
      ? current.filter((id) => id !== appId)
      : [...current, appId];
    const ok = await updateSettings({ hiddenAppIds: nextHidden });
    if (!ok) {
      showToast("保存失败");
    }
  };

  const toggleDefaultInstallApp = async (appId: string) => {
    const current = settings.defaultInstallAppIds ?? [];
    const isDefault = current.includes(appId);
    const nextDefaults = isDefault
      ? current.filter((id) => id !== appId)
      : [...current, appId];
    const ok = await updateSettings({ defaultInstallAppIds: nextDefaults });
    if (!ok) {
      showToast("保存失败");
    }
  };

  const handleBrowseGlobalSkillDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择全局 Skills 目录",
      });
      if (selected) {
        setNewGlobalSkillDir(selected as string);
      }
    } catch {
      // Ignore cancel/errors
    }
  };

  const handleBrowseSshIdentityFile = async () => {
    try {
      const selected = await open({
        directory: false,
        multiple: false,
        title: "选择 SSH 私钥文件 (IdentityFile)",
      });
      if (selected) {
        setSshIdentityFile(selected as string);
      }
    } catch {
      // Ignore cancel/errors
    }
  };

  const buildDraftSshConfig = (): {
    config: SshTargetConfig | null;
    error: string | null;
  } => {
    const host = sshHost.trim();
    if (!host) {
      return { config: null, error: "请填写 SSH Host / IP 或配置别名" };
    }
    let parsedPort: number | null = null;
    if (sshPort.trim()) {
      const num = Number.parseInt(sshPort.trim(), 10);
      if (!Number.isInteger(num) || num < 1 || num > 65535) {
        return { config: null, error: "SSH 端口需为 1–65535 之间的整数" };
      }
      parsedPort = num;
    }

    return {
      config: {
        host,
        user: sshUser.trim() || null,
        port: parsedPort,
        identityFile: sshIdentityFile.trim() || null,
        remoteAppPreset: sshPreset,
        globalPluginDir: sshGlobalPluginDir.trim() || null,
        injectSecrets: sshInjectSecrets,
      },
      error: null,
    };
  };

  const handleTestDraftSsh = async () => {
    const { config, error } = buildDraftSshConfig();
    if (!config || error) {
      setFormTestStatus({
        tone: "error",
        text: error ?? "请填写 SSH 主机信息",
      });
      return;
    }

    setFormTestingSsh(true);
    setFormTestStatus({ tone: "neutral", text: "测试中…" });
    const res = await sshTestConnection(config);
    if (res.ok && res.value.ok) {
      const parts = [`连接成功 · ${res.value.latencyMs}ms`];
      if (res.value.remoteOs) parts.push(res.value.remoteOs);
      if (res.value.remoteHome) parts.push(res.value.remoteHome);
      setFormTestStatus({ tone: "ok", text: parts.join(" · ") });
    } else if (res.ok) {
      setFormTestStatus({
        tone: "error",
        text: `连接失败：${res.value.message || "远端未响应"}`,
      });
    } else {
      setFormTestStatus({
        tone: "error",
        text: `连接失败：${res.error}`,
      });
    }
    setFormTestingSsh(false);
  };

  const handleSaveCustomApp = async () => {
    const label = newAppLabel.trim();
    const globalSkillDir = newGlobalSkillDir.trim();
    if (!label) {
      showToast("请填写客户端名称");
      return;
    }

    let sshConfig: SshTargetConfig | null = null;
    if (appFormMode === "ssh") {
      const built = buildDraftSshConfig();
      if (built.error || !built.config) {
        showToast(built.error ?? "请完善 SSH 连接信息");
        return;
      }
      sshConfig = built.config;
    }

    if (!globalSkillDir) {
      showToast(
        appFormMode === "ssh"
          ? "请填写远端全局 Skills 目录路径"
          : "请填写全局 Skills 目录路径"
      );
      return;
    }

    const isSsh = appFormMode === "ssh";
    let id = editingAppId ?? slugifyAppId(label, isSsh ? "ssh" : "custom");
    if (!editingAppId) {
      const existingIds = new Set(allApps.map((a) => a.id));
      if (existingIds.has(id)) {
        id = `${id}-${Date.now().toString().slice(-4)}`;
      }
    }

    const hasMcpPath = isSsh
      ? Boolean(newGlobalMcpPath.trim())
      : Boolean(newGlobalMcpPath.trim() || newProjectMcpPath.trim());

    const nextApp: CustomAppConfig = {
      id,
      label,
      accentColor: newAppColor,
      globalSkillDir,
      projectSkillDir: isSsh ? null : newProjectSkillDir.trim() || null,
      globalCommandDir: newGlobalCommandDir.trim() || null,
      projectCommandDir: isSsh ? null : newProjectCommandDir.trim() || null,
      globalMcpConfigPath: newGlobalMcpPath.trim() || null,
      projectMcpConfigPath: isSsh ? null : newProjectMcpPath.trim() || null,
      mcpFormat: hasMcpPath ? newMcpFormat : null,
      mcpRootKey: newMcpRootKey.trim() || null,
      ssh: isSsh ? sshConfig : null,
    };

    const existingList = settings.customApps ?? [];
    const nextCustomApps = editingAppId
      ? existingList.map((item) => (item.id === editingAppId ? nextApp : item))
      : [...existingList, nextApp];

    const ok = await updateSettings({ customApps: nextCustomApps });
    if (!ok) {
      showToast("保存目标客户端失败");
      return;
    }

    const wasEditing = Boolean(editingAppId);
    resetFormFields();
    setShowAddAppForm(false);
    await refreshAllResourceContexts();
    showToast(
      wasEditing
        ? `已更新目标端「${label}」`
        : isSsh
          ? `已添加 SSH 远程目标端「${label}」`
          : `已添加客户端「${label}」`
    );
  };

  const handleDeleteCustomApp = async (appId: string, label: string) => {
    const isEditingTarget = editingAppId === appId;
    const confirmed = await confirm({
      title: `移除目标端「${label}」？`,
      message: "不会删除本机或远端已有的文件。",
      confirmLabel: "移除",
      danger: true,
    });
    if (!confirmed) return;

    const ok = await updateSettings({
      customApps: (settings.customApps ?? []).filter((a) => a.id !== appId),
      hiddenAppIds: (settings.hiddenAppIds ?? []).filter((id) => id !== appId),
      defaultInstallAppIds: (settings.defaultInstallAppIds ?? []).filter(
        (id) => id !== appId
      ),
    });
    if (ok) {
      if (isEditingTarget) {
        resetFormFields();
        setShowAddAppForm(false);
      }
      await refreshAllResourceContexts();
      showToast(`已移除「${label}」`);
    } else {
      showToast("移除失败");
    }
  };

  const handleRowTestSsh = async (appId: string, ssh: SshTargetConfig) => {
    setSshRowBusy((prev) => ({ ...prev, [appId]: "test" }));
    setSshRowFeedback((prev) => ({
      ...prev,
      [appId]: { tone: "neutral", text: "测试中…" },
    }));

    const res = await sshTestConnection(ssh);
    if (res.ok && res.value.ok) {
      const parts = [`连接成功 · ${res.value.latencyMs}ms`];
      if (res.value.remoteOs) parts.push(res.value.remoteOs);
      if (res.value.remoteHome) parts.push(res.value.remoteHome);
      const summary = parts.join(" · ");
      setSshRowFeedback((prev) => ({
        ...prev,
        [appId]: { tone: "ok", text: summary },
      }));
    } else {
      const errMsg = res.ok ? res.value.message || "远端未响应" : res.error;
      setSshRowFeedback((prev) => ({
        ...prev,
        [appId]: { tone: "error", text: `连接失败：${errMsg}` },
      }));
    }
    setSshRowBusy((prev) => ({ ...prev, [appId]: undefined }));
  };

  const handleRowCalibrateSsh = async (appId: string, appLabel: string) => {
    setSshRowBusy((prev) => ({ ...prev, [appId]: "calibrate" }));
    setSshRowFeedback((prev) => ({
      ...prev,
      [appId]: { tone: "neutral", text: "校准中…" },
    }));

    const res = await sshCalibrateTarget(appId);
    if (res.ok) {
      await refreshAllResourceContexts();
      const l = res.value;
      const text = `状态已校准 · ${l.skills.length} Skills · ${l.commands.length} Commands · ${l.mcpServers.length} MCP · ${l.plugins.length} Plugins`;
      setSshRowFeedback((prev) => ({
        ...prev,
        [appId]: { tone: "ok", text },
      }));
      showToast(`已校准「${appLabel}」远端状态`);
    } else {
      setSshRowFeedback((prev) => ({
        ...prev,
        [appId]: { tone: "error", text: `校准失败：${res.error}` },
      }));
      showToast(`校准失败：${res.error}`);
    }
    setSshRowBusy((prev) => ({ ...prev, [appId]: undefined }));
  };

  const handleRowSyncAllSsh = async (appId: string) => {
    setSshRowBusy((prev) => ({ ...prev, [appId]: "sync" }));
    setSshRowFeedback((prev) => ({
      ...prev,
      [appId]: { tone: "neutral", text: "同步中…" },
    }));

    const res = await sshSyncAllEnabled(appId);
    if (res.ok) {
      await refreshAllResourceContexts();
      const v = res.value;
      const summary = `已同步 ${v.syncedSkills} Skills${
        v.syncedCommands > 0 ? ` · ${v.syncedCommands} Commands` : ""
      } · ${v.syncedMcps} MCP · ${v.syncedPlugins} Plugins`;
      const feedbackText =
        v.warnings.length > 0
          ? `${summary}（提示：${v.warnings[0]}）`
          : summary;
      setSshRowFeedback((prev) => ({
        ...prev,
        [appId]: {
          tone: v.warnings.length > 0 ? "neutral" : "ok",
          text: feedbackText,
        },
      }));
      showToast(summary);
    } else {
      setSshRowFeedback((prev) => ({
        ...prev,
        [appId]: { tone: "error", text: `全量重推失败：${res.error}` },
      }));
      showToast(`全量重推失败：${res.error}`);
    }
    setSshRowBusy((prev) => ({ ...prev, [appId]: undefined }));
  };

  const selectedDiscoveredHost = useMemo(
    () => discoveredHosts.find((h) => h.alias === sshHost.trim()) ?? null,
    [discoveredHosts, sshHost]
  );

  return (
    <section className={s.card} data-section="apps">
      <div className={s.cardHeader}>
        <div className={s.cardTitleWrap}>
          <div className={s.cardTitleIcon}>
            <Layers size={15} />
          </div>
          <h2 className={s.cardTitle}>目标客户端管理</h2>
        </div>
        <div className={s.cardHeaderActions}>
          {showAddAppForm ? (
            <button
              type="button"
              className={s.btn}
              onClick={() => {
                resetFormFields();
                setShowAddAppForm(false);
              }}
            >
              收起表单
            </button>
          ) : (
            <>
              <button
                type="button"
                className={s.btn}
                onClick={handleOpenCreateLocal}
              >
                <Plus size={13} /> 添加本地客户端
              </button>
              <button
                type="button"
                className={s.btnPrimary}
                onClick={handleOpenCreateSsh}
              >
                <Server size={13} /> 添加 SSH 远程目标端
              </button>
            </>
          )}
        </div>
      </div>

      <div className={s.rows}>
        <div className={s.row}>
          <div className={s.rowBody}>
            <div className={s.rowLabel}>本地软链接与 SSH 远程目标端</div>
            <div className={s.rowHint}>
              新安装的 Skill
              默认仅存入本地库；点亮某端的「新装自启」后会自动启用。支持添加本地自定义目录，或配置虚拟机（如
              claude-cage）/ 远程服务器的 SSH 目标端同步全局
              Skills、Commands、MCP 与 Plugins。
            </div>
          </div>
        </div>

        {allApps.map((app) => {
          const isVisible = !hiddenSet.has(app.id);
          const isDefaultInstall = defaultInstallSet.has(app.id);
          const sharesAgentsDir =
            app.projectSkillRelDir?.toLowerCase() === ".agents/skills";
          const rawCustom = !app.isBuiltin
            ? (customAppMap.get(app.id) ?? null)
            : null;
          const sshCfg = rawCustom?.ssh ?? null;
          const busyState = sshRowBusy[app.id];
          const feedback = sshRowFeedback[app.id];

          return (
            <div
              key={app.id}
              className={`${s.appRow} ${app.isSsh ? s.appRowSsh : ""} ${
                !isVisible ? s.appRowHidden : ""
              }`}
            >
              <div className={s.appRowMain}>
                <div className={s.appRowLeft}>
                  {app.iconSrc ? (
                    <img src={app.iconSrc} alt="" className={s.appIconImg} />
                  ) : (
                    <span
                      className={s.appMonogram}
                      style={{ background: app.accentColor }}
                    >
                      {app.shortLabel}
                    </span>
                  )}
                  <div className={s.appInfo}>
                    <div className={s.appNameLine}>
                      <span className={s.appName}>{app.label}</span>
                      {app.isSsh ? (
                        <>
                          <span className={s.sshBadge}>SSH</span>
                          {app.sshHostLabel && (
                            <span className={s.sshHostTag}>
                              {app.sshHostLabel}
                            </span>
                          )}
                          <span
                            className={
                              sshCfg?.injectSecrets
                                ? s.secretBadgeInject
                                : s.secretBadgePlaceholder
                            }
                          >
                            {sshCfg?.injectSecrets ? "注入密钥" : "仅占位符"}
                          </span>
                        </>
                      ) : (
                        <span className={s.appKindBadge}>
                          {app.isBuiltin ? "内置预设" : "自定义"}
                        </span>
                      )}
                      {sharesAgentsDir && (
                        <span className={s.appSharedNote}>
                          项目级共用 .agents/skills/
                        </span>
                      )}
                    </div>
                    <div className={s.appPathLine}>
                      全局: {app.skillPathLabel}
                      {app.projectSkillRelDir
                        ? ` · 项目: ${app.projectSkillRelDir}/`
                        : " · 仅全局"}
                      {app.commandPathLabel
                        ? ` · 命令: ${app.commandPathLabel}`
                        : ""}
                      {app.isSsh && app.mcpGlobalPathLabel
                        ? ` · MCP: ${app.mcpGlobalPathLabel}`
                        : ""}
                    </div>
                  </div>
                </div>

                <div className={s.appControls}>
                  <button
                    type="button"
                    className={`${s.appChipBtn} ${
                      isDefaultInstall ? s.appChipBtnActive : ""
                    }`}
                    onClick={() => void toggleDefaultInstallApp(app.id)}
                    title="从在线搜索、市场或本地导入新 Skill 时自动启用该客户端"
                  >
                    新装自启
                  </button>
                  <button
                    type="button"
                    className={`${s.appChipBtn} ${
                      isVisible ? s.appChipBtnActive : ""
                    }`}
                    onClick={() => void toggleAppVisible(app.id)}
                    title="控制是否在技能卡片与启用面板中显示该客户端"
                  >
                    {isVisible ? "已显示" : "已隐藏"}
                  </button>
                  {!app.isBuiltin && rawCustom && (
                    <button
                      type="button"
                      className={s.appIconActionBtn}
                      onClick={() => handleOpenEditCustomApp(rawCustom)}
                      title={`编辑 ${app.label}`}
                      aria-label={`编辑 ${app.label}`}
                    >
                      <Edit3 size={13} />
                    </button>
                  )}
                  {!app.isBuiltin && (
                    <button
                      type="button"
                      className={s.appDeleteBtn}
                      onClick={() =>
                        void handleDeleteCustomApp(app.id, app.label)
                      }
                      title={`移除 ${app.label}`}
                      aria-label={`移除 ${app.label}`}
                    >
                      <Trash2 size={14} />
                    </button>
                  )}
                </div>
              </div>

              {app.isSsh && sshCfg && (
                <div className={s.sshRowToolbar}>
                  <div
                    className={`${s.sshRowStatus} ${
                      feedback?.tone === "ok"
                        ? s.sshStatusOk
                        : feedback?.tone === "error"
                          ? s.sshStatusError
                          : ""
                    }`}
                  >
                    {feedback
                      ? feedback.text
                      : "远端同步就绪 · 支持检测连通性、校准远端目录或全量重推"}
                  </div>
                  <div className={s.sshRowActions}>
                    <button
                      type="button"
                      className={s.sshActionBtn}
                      disabled={Boolean(busyState)}
                      onClick={() => void handleRowTestSsh(app.id, sshCfg)}
                    >
                      {busyState === "test" ? "测试中…" : "测试连接"}
                    </button>
                    <button
                      type="button"
                      className={s.sshActionBtn}
                      disabled={Boolean(busyState)}
                      onClick={() =>
                        void handleRowCalibrateSsh(app.id, app.label)
                      }
                      title="扫描远端已存在的 Skills / Commands / MCP / Plugins 并同步回本地开关"
                    >
                      {busyState === "calibrate" ? "校准中…" : "校准状态"}
                    </button>
                    <button
                      type="button"
                      className={s.sshActionBtnPrimary}
                      disabled={Boolean(busyState)}
                      onClick={() => void handleRowSyncAllSsh(app.id)}
                      title="将当前已对该目标启用的全部资源重新推送到远端主机"
                    >
                      {busyState === "sync" ? "同步中…" : "全量重推"}
                    </button>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>

      {showAddAppForm && (
        <div className={s.customFormWrap}>
          <div className={s.formTopBar}>
            <div className={s.customFormTitle}>
              {editingAppId
                ? appFormMode === "ssh"
                  ? "编辑 SSH 远程目标端"
                  : "编辑本地客户端"
                : "新增目标客户端"}
            </div>
            <div className={s.modeTabs} role="tablist">
              <button
                type="button"
                role="tab"
                aria-selected={appFormMode === "local"}
                className={`${s.modeTab} ${
                  appFormMode === "local" ? s.modeTabActive : ""
                }`}
                onClick={() => handleSwitchFormMode("local")}
              >
                本地目录客户端
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={appFormMode === "ssh"}
                className={`${s.modeTab} ${
                  appFormMode === "ssh" ? s.modeTabActive : ""
                }`}
                onClick={() => handleSwitchFormMode("ssh")}
              >
                SSH 远程目标端（虚拟机 / 服务器）
              </button>
            </div>
          </div>

          {appFormMode === "ssh" ? (
            <>
              {/* 1. SSH Host Discovery */}
              <div className={s.sshDiscoveryBox}>
                <div className={s.sshDiscoveryHeader}>
                  <span className={s.customFieldLabel}>
                    从 ~/.ssh/config 快速选择主机别名
                  </span>
                  <button
                    type="button"
                    className={s.sshInlineLinkBtn}
                    disabled={discoveringHosts}
                    onClick={() => void loadDiscoveredSshHosts()}
                  >
                    {discoveringHosts ? "扫描中…" : "重新扫描"}
                  </button>
                </div>

                {discoveringHosts ? (
                  <div className={s.sshDiscoveryHint}>
                    正在解析 ~/.ssh/config（含 Include 子配置）…
                  </div>
                ) : discoverError ? (
                  <div className={s.sshDiscoveryHint}>
                    读取 SSH 配置失败：{discoverError}（可直接手动填写）
                  </div>
                ) : discoveredHosts.length === 0 ? (
                  <div className={s.sshDiscoveryHint}>
                    未在 ~/.ssh/config 检测到主机别名，请在下方手动输入 Host 或
                    IP。
                  </div>
                ) : (
                  <div className={s.hostChipList}>
                    {discoveredHosts.map((h) => {
                      const active = sshHost.trim() === h.alias;
                      return (
                        <button
                          key={h.alias}
                          type="button"
                          className={`${s.hostChip} ${
                            active ? s.hostChipActive : ""
                          }`}
                          onClick={() => handlePickDiscoveredHost(h)}
                          title={`来源: ${h.sourceFile}`}
                        >
                          <span className={s.hostChipAlias}>{h.alias}</span>
                          {(h.user || h.hostname) && (
                            <span className={s.hostChipMeta}>
                              {h.user ? `${h.user}@` : ""}
                              {h.hostname ?? h.alias}
                              {h.port ? `:${h.port}` : ""}
                            </span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                )}
                {selectedDiscoveredHost && (
                  <div className={s.sshDiscoveryHint}>
                    已关联配置别名{" "}
                    <code className={s.code}>
                      {selectedDiscoveredHost.alias}
                    </code>
                    ：留空下方用户名、端口与私钥时将自动沿用{" "}
                    <code className={s.code}>~/.ssh/config</code> 的连接参数。
                  </div>
                )}
              </div>

              {/* 2. Basic & SSH Connection Fields */}
              <div className={s.customFormGrid}>
                <div className={s.customField}>
                  <span className={s.customFieldLabel}>
                    显示名称 *（在各面板展示）
                  </span>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={newAppLabel}
                    onChange={(e) => setNewAppLabel(e.target.value)}
                    placeholder="例如：claude-cage · Claude"
                  />
                </div>
                <div className={s.customField}>
                  <span className={s.customFieldLabel}>标识颜色</span>
                  <div className={s.colorSwatches}>
                    {CUSTOM_APP_COLORS.map((color) => (
                      <button
                        key={color}
                        type="button"
                        className={`${s.colorSwatch} ${
                          newAppColor === color ? s.colorSwatchActive : ""
                        }`}
                        style={{ background: color }}
                        onClick={() => setNewAppColor(color)}
                        aria-label={`选择颜色 ${color}`}
                      />
                    ))}
                  </div>
                </div>

                <div className={s.customField}>
                  <span className={s.customFieldLabel}>
                    Host / IP 或 SSH 别名 *
                  </span>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={sshHost}
                    onChange={(e) => {
                      setSshHost(e.target.value);
                      setFormTestStatus(null);
                    }}
                    placeholder="例如：lima-claude-cage-... 或 192.168.1.50"
                  />
                </div>

                <div className={s.sshUserPortRow}>
                  <div className={s.customField}>
                    <span className={s.customFieldLabel}>
                      用户名 (User，选填)
                    </span>
                    <input
                      className={`${s.repoInput} ${s.repoInputWide}`}
                      value={sshUser}
                      onChange={(e) => {
                        setSshUser(e.target.value);
                        setFormTestStatus(null);
                      }}
                      placeholder="默认沿用 SSH 配置"
                    />
                  </div>
                  <div className={s.sshPortField}>
                    <span className={s.customFieldLabel}>端口</span>
                    <input
                      className={`${s.repoInput} ${s.repoInputWide}`}
                      value={sshPort}
                      onChange={(e) => {
                        setSshPort(e.target.value);
                        setFormTestStatus(null);
                      }}
                      placeholder="22"
                      inputMode="numeric"
                    />
                  </div>
                </div>

                <div className={`${s.customField} ${s.customFieldFull}`}>
                  <span className={s.customFieldLabel}>
                    私钥路径 (IdentityFile，选填)
                  </span>
                  <div className={s.customPathRow}>
                    <input
                      className={`${s.repoInput} ${s.repoInputWide}`}
                      value={sshIdentityFile}
                      onChange={(e) => {
                        setSshIdentityFile(e.target.value);
                        setFormTestStatus(null);
                      }}
                      placeholder="留空则使用默认密钥或 ~/.ssh/config 配置"
                    />
                    <button
                      type="button"
                      className={s.btn}
                      onClick={() => void handleBrowseSshIdentityFile()}
                    >
                      <Folder size={13} /> 浏览
                    </button>
                  </div>
                </div>

                {/* 3. Remote Client Preset Selector */}
                <div className={`${s.customField} ${s.customFieldFull}`}>
                  <span className={s.customFieldLabel}>
                    远端 AI 客户端预设（一键自动填入远端路径）
                  </span>
                  <div className={s.presetGrid}>
                    {REMOTE_APP_PRESETS.map((preset) => (
                      <button
                        key={preset.id}
                        type="button"
                        className={`${s.presetChip} ${
                          sshPreset === preset.id ? s.presetChipActive : ""
                        }`}
                        onClick={() =>
                          applyPresetFields(preset.id, {
                            updateColor: editingAppId === null,
                          })
                        }
                      >
                        {preset.label}
                      </button>
                    ))}
                  </div>
                </div>

                {/* Preset path summary + expandable advanced remote paths */}
                <div className={`${s.customField} ${s.customFieldFull}`}>
                  <div className={s.remotePathSummaryBar}>
                    <div className={s.remotePathSummaryText}>
                      Skills: {newGlobalSkillDir || "未填"}
                      {newGlobalCommandDir
                        ? ` · Commands: ${newGlobalCommandDir}`
                        : ""}
                      {newGlobalMcpPath ? ` · MCP: ${newGlobalMcpPath}` : ""}
                      {sshGlobalPluginDir
                        ? ` · Plugins: ${sshGlobalPluginDir}`
                        : ""}
                    </div>
                    {sshPreset !== "custom" && (
                      <button
                        type="button"
                        className={s.sshInlineLinkBtn}
                        onClick={() => setShowSshAdvancedPaths((prev) => !prev)}
                      >
                        {showSshAdvancedPaths ? (
                          <>
                            收起远端路径 <ChevronUp size={12} />
                          </>
                        ) : (
                          <>
                            展开修改远端路径 <ChevronDown size={12} />
                          </>
                        )}
                      </button>
                    )}
                  </div>
                </div>

                {(sshPreset === "custom" || showSshAdvancedPaths) && (
                  <>
                    <div className={s.customField}>
                      <span className={s.customFieldLabel}>
                        远端全局 Skills 目录 *（远端 ~ 展开为远端 $HOME）
                      </span>
                      <input
                        className={`${s.repoInput} ${s.repoInputWide}`}
                        value={newGlobalSkillDir}
                        onChange={(e) => setNewGlobalSkillDir(e.target.value)}
                        placeholder="例如：~/.claude/skills"
                      />
                    </div>

                    <div className={s.customField}>
                      <span className={s.customFieldLabel}>
                        远端全局 Commands 目录（选填）
                      </span>
                      <input
                        className={`${s.repoInput} ${s.repoInputWide}`}
                        value={newGlobalCommandDir}
                        onChange={(e) => setNewGlobalCommandDir(e.target.value)}
                        placeholder="例如：~/.claude/commands"
                      />
                    </div>

                    <div className={s.customField}>
                      <span className={s.customFieldLabel}>
                        远端全局 MCP 配置文件路径（选填）
                      </span>
                      <input
                        className={`${s.repoInput} ${s.repoInputWide}`}
                        value={newGlobalMcpPath}
                        onChange={(e) => setNewGlobalMcpPath(e.target.value)}
                        placeholder="例如：~/.claude.json"
                      />
                    </div>

                    <div className={s.customField}>
                      <span className={s.customFieldLabel}>
                        远端全局 Plugins 目录（选填）
                      </span>
                      <input
                        className={`${s.repoInput} ${s.repoInputWide}`}
                        value={sshGlobalPluginDir}
                        onChange={(e) => setSshGlobalPluginDir(e.target.value)}
                        placeholder="例如：~/.claude/plugins/local"
                      />
                    </div>

                    <div className={s.customField}>
                      <span className={s.customFieldLabel}>MCP 配置格式</span>
                      <select
                        className={s.select}
                        value={newMcpFormat}
                        onChange={(e) =>
                          setNewMcpFormat(e.target.value as "json" | "toml")
                        }
                      >
                        <option value="json">JSON</option>
                        <option value="toml">TOML</option>
                      </select>
                    </div>

                    <div className={s.customField}>
                      <span className={s.customFieldLabel}>MCP 根节点键名</span>
                      <input
                        className={`${s.repoInput} ${s.repoInputWide}`}
                        value={newMcpRootKey}
                        onChange={(e) => setNewMcpRootKey(e.target.value)}
                        placeholder="例如：mcpServers 或 mcp_servers"
                      />
                    </div>
                  </>
                )}

                {/* 4. Secret Injection Toggle */}
                <div className={`${s.customField} ${s.customFieldFull}`}>
                  <div className={s.secretPolicyCard}>
                    <div className={s.rowBody}>
                      <div className={s.rowLabel}>
                        向远端注入本机敏感密钥（API Key）
                      </div>
                      <div className={s.rowHint}>
                        开启后同步 MCP / Plugin 时写入真实凭据，适合本机{" "}
                        <code className={s.code}>claude-cage</code>{" "}
                        等受信任虚拟机；关闭则仅写入{" "}
                        <code className={s.code}>{"${ENV_VAR}"}</code>{" "}
                        占位符，适合公共/共享服务器。
                      </div>
                    </div>
                    <Toggle
                      checked={sshInjectSecrets}
                      onChange={setSshInjectSecrets}
                    />
                  </div>
                </div>
              </div>

              {/* 5. Connection Test Status Banner */}
              {formTestStatus && (
                <div
                  className={`${s.sshTestBanner} ${
                    formTestStatus.tone === "ok"
                      ? s.sshTestBannerOk
                      : formTestStatus.tone === "error"
                        ? s.sshTestBannerError
                        : ""
                  }`}
                >
                  {formTestStatus.text}
                </div>
              )}

              <div className={s.customFormActions}>
                <button
                  type="button"
                  className={s.btn}
                  disabled={formTestingSsh}
                  onClick={() => void handleTestDraftSsh()}
                >
                  {formTestingSsh ? "测试中…" : "测试 SSH 连接"}
                </button>
                <button
                  type="button"
                  className={s.btn}
                  onClick={() => {
                    resetFormFields();
                    setShowAddAppForm(false);
                  }}
                >
                  取消
                </button>
                <button
                  type="button"
                  className={s.btnPrimary}
                  onClick={() => void handleSaveCustomApp()}
                >
                  {editingAppId ? "保存修改" : "保存添加"}
                </button>
              </div>
            </>
          ) : (
            <>
              <div className={s.customFormGrid}>
                <div className={s.customField}>
                  <span className={s.customFieldLabel}>客户端名称 *</span>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={newAppLabel}
                    onChange={(e) => setNewAppLabel(e.target.value)}
                    placeholder="例如：Windsurf / Trae / Zed"
                  />
                </div>
                <div className={s.customField}>
                  <span className={s.customFieldLabel}>标识颜色</span>
                  <div className={s.colorSwatches}>
                    {CUSTOM_APP_COLORS.map((color) => (
                      <button
                        key={color}
                        type="button"
                        className={`${s.colorSwatch} ${
                          newAppColor === color ? s.colorSwatchActive : ""
                        }`}
                        style={{ background: color }}
                        onClick={() => setNewAppColor(color)}
                        aria-label={`选择颜色 ${color}`}
                      />
                    ))}
                  </div>
                </div>

                <div className={`${s.customField} ${s.customFieldFull}`}>
                  <span className={s.customFieldLabel}>
                    全局 Skills 目录 *（支持 ~/ 或绝对路径）
                  </span>
                  <div className={s.customPathRow}>
                    <input
                      className={`${s.repoInput} ${s.repoInputWide}`}
                      value={newGlobalSkillDir}
                      onChange={(e) => setNewGlobalSkillDir(e.target.value)}
                      placeholder="例如：~/.windsurf/skills"
                    />
                    <button
                      type="button"
                      className={s.btn}
                      onClick={() => void handleBrowseGlobalSkillDir()}
                    >
                      <Folder size={13} /> 浏览
                    </button>
                  </div>
                </div>

                <div className={`${s.customField} ${s.customFieldFull}`}>
                  <span className={s.customFieldLabel}>
                    项目级 Skills 相对目录（选填，相对于项目根目录）
                  </span>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={newProjectSkillDir}
                    onChange={(e) => setNewProjectSkillDir(e.target.value)}
                    placeholder="例如：.windsurf/skills（若填 .agents/skills 会自动与 T3/反重力联动合并）"
                  />
                </div>

                <div className={s.customField}>
                  <span className={s.customFieldLabel}>
                    全局 Commands 目录（选填，填后出现在「我的命令」）
                  </span>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={newGlobalCommandDir}
                    onChange={(e) => setNewGlobalCommandDir(e.target.value)}
                    placeholder="例如：~/.windsurf/commands"
                  />
                </div>

                <div className={s.customField}>
                  <span className={s.customFieldLabel}>
                    项目级 Commands 相对目录（选填）
                  </span>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={newProjectCommandDir}
                    onChange={(e) => setNewProjectCommandDir(e.target.value)}
                    placeholder="例如：.windsurf/commands"
                  />
                </div>

                <div className={s.customField}>
                  <span className={s.customFieldLabel}>
                    全局 MCP 配置文件路径（选填，填后接入 MCP 管理）
                  </span>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={newGlobalMcpPath}
                    onChange={(e) => setNewGlobalMcpPath(e.target.value)}
                    placeholder="例如：~/.config/opencode/opencode.json"
                  />
                </div>

                <div className={s.customField}>
                  <span className={s.customFieldLabel}>
                    项目级 MCP 相对配置文件（选填）
                  </span>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={newProjectMcpPath}
                    onChange={(e) => setNewProjectMcpPath(e.target.value)}
                    placeholder="例如：.mcp.json"
                  />
                </div>

                <div className={s.customField}>
                  <span className={s.customFieldLabel}>
                    MCP 配置格式（选填）
                  </span>
                  <select
                    className={s.select}
                    value={newMcpFormat}
                    onChange={(e) =>
                      setNewMcpFormat(e.target.value as "json" | "toml")
                    }
                  >
                    <option value="json">JSON</option>
                    <option value="toml">TOML</option>
                  </select>
                </div>

                <div className={s.customField}>
                  <span className={s.customFieldLabel}>
                    MCP 根节点键名（选填，默认 mcpServers / mcp_servers）
                  </span>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={newMcpRootKey}
                    onChange={(e) => setNewMcpRootKey(e.target.value)}
                    placeholder="例如：mcpServers 或 context_servers"
                  />
                </div>
              </div>

              <div className={s.customFormActions}>
                <button
                  type="button"
                  className={s.btn}
                  onClick={() => {
                    resetFormFields();
                    setShowAddAppForm(false);
                  }}
                >
                  取消
                </button>
                <button
                  type="button"
                  className={s.btnPrimary}
                  onClick={() => void handleSaveCustomApp()}
                >
                  {editingAppId ? "保存修改" : "保存添加"}
                </button>
              </div>
            </>
          )}
        </div>
      )}
    </section>
  );
}
