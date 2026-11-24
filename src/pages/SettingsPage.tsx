import { useState, useCallback, useEffect, useMemo } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Folder,
  Github,
  Languages,
  HardDrive,
  Palette,
  AlertTriangle,
  Terminal,
  Upload,
  Download,
  Info,
  X,
  RefreshCw,
  Layers,
  Plus,
  Trash2,
  Server,
  Edit3,
  ChevronDown,
  ChevronUp,
} from "lucide-react";
import { useSettings } from "../context/SettingsContext";
import { useApps } from "../context/AppContext";
import { useSource } from "../context/SourceContext";
import { useSkills } from "../context/SkillContext";
import { useSlashCommands } from "../context/SlashCommandContext";
import { useMcp } from "../context/McpContext";
import { usePlugins } from "../context/PluginContext";
import { useUpdater } from "../context/UpdaterContext";
import {
  BACKUP_SOURCE_REPO_ID,
  backupSourceConnect,
  backupSourcePull,
  backupSourcePush,
} from "../services/backupSource";
import { showInFinder } from "../services/skill";
import {
  sshCalibrateTarget,
  sshDiscoverHosts,
  sshSyncAllEnabled,
  sshTestConnection,
} from "../services/ssh";
import type {
  BackupSource,
  BackupSourceStatus,
  CustomAppConfig,
  DiscoveredSshHost,
  SshTargetConfig,
} from "../types";
import s from "./SettingsPage.module.css";

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

function formatSyncTime(ts?: number | null): string {
  if (!ts) return "未同步";
  return new Date(ts).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

type BackupSourceDraft = {
  remoteUrl: string;
  branch: string;
  localPath?: string | null;
  lastSyncedAt?: number | null;
};

const DEFAULT_BACKUP_SOURCE_DRAFT: BackupSourceDraft = {
  remoteUrl: "git@github.com:DargonLee/My-Skills.git",
  branch: "main",
  localPath: null,
  lastSyncedAt: null,
};

function normalizeBackupSource(
  source: BackupSource | null | undefined
): BackupSourceDraft {
  if (!source) {
    return { ...DEFAULT_BACKUP_SOURCE_DRAFT };
  }

  return {
    remoteUrl: source.remoteUrl.trim(),
    branch: source.branch.trim() || "main",
    localPath: source.localPath ?? null,
    lastSyncedAt: source.lastSyncedAt ?? null,
  };
}

function parseRepoFromRemoteUrl(remoteUrl: string): string {
  const trimmed = remoteUrl.trim();
  if (!trimmed) return "";

  const sshPrefix = "git@github.com:";
  if (trimmed.startsWith(sshPrefix)) {
    return trimmed
      .slice(sshPrefix.length)
      .replace(/\.git$/i, "")
      .replace(/^\/+|\/+$/g, "");
  }

  const sshUrlPrefix = "ssh://git@github.com/";
  if (trimmed.startsWith(sshUrlPrefix)) {
    return trimmed
      .slice(sshUrlPrefix.length)
      .replace(/\.git$/i, "")
      .replace(/^\/+|\/+$/g, "");
  }

  return "";
}

function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      type="button"
      className={`${s.toggle} ${checked ? s.toggleOn : ""}`}
      onClick={() => onChange(!checked)}
      role="switch"
      aria-checked={checked}
    >
      <span className={`${s.toggleThumb} ${checked ? s.toggleThumbOn : ""}`} />
    </button>
  );
}

type RowFeedback = {
  tone: "ok" | "error" | "neutral";
  text: string;
};

export function SettingsPage() {
  const { settings, updateSettings } = useSettings();
  const { allApps } = useApps();
  const { refresh: refreshSources } = useSource();
  const { refresh: refreshSkills } = useSkills();
  const { refresh: refreshCommands } = useSlashCommands();
  const { refresh: refreshMcp } = useMcp();
  const { refresh: refreshPlugins } = usePlugins();
  const { currentVersion, isChecking, checkForUpdates } = useUpdater();

  const [toast, setToast] = useState<string | null>(null);
  const [backupDraft, setBackupDraft] = useState<BackupSourceDraft>(() =>
    normalizeBackupSource(settings.backupSource)
  );
  const [backupAction, setBackupAction] = useState<
    "test" | "push" | "pull" | null
  >(null);
  const [showBackupGuide, setShowBackupGuide] = useState(false);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);

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

  useEffect(() => {
    setBackupDraft(normalizeBackupSource(settings.backupSource));
  }, [settings.backupSource]);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 2600);
  }, []);

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
    const confirmed = window.confirm(
      `移除目标端「${label}」？（不会删除本机或远端已有的文件）`
    );
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

  const handleBackupDraftChange = useCallback(
    <K extends keyof BackupSourceDraft>(
      key: K,
      value: BackupSourceDraft[K]
    ) => {
      setBackupDraft((prev) => ({ ...prev, [key]: value }));
    },
    []
  );

  const handleSettingChange = useCallback(
    async (
      key: keyof Pick<
        typeof settings,
        | "theme"
        | "locale"
        | "autoCheckUpdates"
        | "autoStart"
        | "backupPath"
        | "maxBackups"
      >,
      value: string | boolean | number | null
    ) => {
      const success = await updateSettings({
        [key]: value,
      } as Partial<typeof settings>);
      if (!success) {
        showToast("设置保存失败");
      }
    },
    [settings, showToast, updateSettings]
  );

  const syncBackupDraftFromStatus = useCallback(
    (status: BackupSourceStatus) => {
      const nextSource: BackupSource = {
        repo: status.repo,
        label: status.label,
        remoteUrl: status.remoteUrl,
        branch: status.branch,
        localPath: status.localPath ?? null,
        lastSyncedAt: status.lastSyncedAt ?? null,
      };
      setBackupDraft(normalizeBackupSource(nextSource));
      return nextSource;
    },
    []
  );

  const buildBackupToast = useCallback(
    (message: string, notice?: string | null) => {
      return notice ? `${message}；${notice}` : message;
    },
    []
  );

  const persistBackupSource = useCallback(
    async (showSuccessToast: boolean) => {
      const remoteUrl = backupDraft.remoteUrl.trim();
      const repo = parseRepoFromRemoteUrl(remoteUrl);
      const branch = backupDraft.branch.trim() || "main";

      if (!repo) {
        showToast("请输入 GitHub SSH 地址，例如 git@github.com:owner/repo.git");
        return null;
      }
      if (
        !remoteUrl.startsWith("git@github.com:") &&
        !remoteUrl.startsWith("ssh://git@github.com/")
      ) {
        showToast("请使用 GitHub SSH 地址，例如 git@github.com:owner/repo.git");
        return null;
      }

      const nextSource: BackupSource = {
        repo,
        label: repo,
        remoteUrl,
        branch,
        localPath: backupDraft.localPath ?? null,
        lastSyncedAt: backupDraft.lastSyncedAt ?? null,
      };

      const success = await updateSettings({ backupSource: nextSource });
      if (success) {
        setBackupDraft(normalizeBackupSource(nextSource));
        if (showSuccessToast) {
          showToast("备份源已保存");
        }
        return nextSource;
      } else {
        showToast("备份源保存失败");
        return null;
      }
    },
    [backupDraft, showToast, updateSettings]
  );

  const handleBackupSourceAction = useCallback(
    async (action: "test" | "push" | "pull") => {
      const savedSource = await persistBackupSource(false);
      if (!savedSource) return;

      setBackupAction(action);

      const connectResult = await backupSourceConnect();
      if (!connectResult.ok) {
        showToast(`连接失败：${connectResult.error}`);
        setBackupAction(null);
        return;
      }

      const connectedSource = syncBackupDraftFromStatus(connectResult.value);
      const connectSaved = await updateSettings({
        backupSource: connectedSource,
      });
      if (!connectSaved) {
        showToast("连接状态保存失败");
        setBackupAction(null);
        return;
      }
      refreshSources(BACKUP_SOURCE_REPO_ID);
      await refreshSkills();

      if (action === "test") {
        showToast(buildBackupToast("备份源已连接", connectResult.value.notice));
        setBackupAction(null);
        return;
      }

      const syncResult =
        action === "push" ? await backupSourcePush() : await backupSourcePull();
      if (!syncResult.ok) {
        showToast(
          `${action === "push" ? "推送" : "拉取"}失败：${syncResult.error}`
        );
        setBackupAction(null);
        return;
      }

      const syncedSource = syncBackupDraftFromStatus(syncResult.value);
      const syncSaved = await updateSettings({ backupSource: syncedSource });
      if (!syncSaved) {
        showToast("同步状态保存失败");
        setBackupAction(null);
        return;
      }

      refreshSources(BACKUP_SOURCE_REPO_ID);
      await refreshSkills();
      showToast(
        buildBackupToast(
          action === "push"
            ? "备份已推送到 GitHub"
            : "已从 GitHub 拉取最新备份",
          syncResult.value.notice
        )
      );
      setBackupAction(null);
    },
    [
      buildBackupToast,
      persistBackupSource,
      refreshSkills,
      refreshSources,
      showToast,
      syncBackupDraftFromStatus,
      updateSettings,
    ]
  );

  const handleOpenBackupFolder = useCallback(async () => {
    if (!settings.backupPath) {
      showToast("备份目录正在加载");
      return;
    }

    const result = await showInFinder(settings.backupPath);
    if (!result.ok) {
      showToast("备份目录还没有生成，请先创建一次备份");
    }
  }, [settings.backupPath, showToast]);

  const lang =
    settings.locale === "zh-CN"
      ? "简体中文"
      : settings.locale === "en-US"
        ? "English"
        : "繁體中文";
  const theme =
    settings.theme === "light"
      ? "浅色"
      : settings.theme === "dark"
        ? "深色"
        : "跟随系统";

  const selectedDiscoveredHost = useMemo(
    () => discoveredHosts.find((h) => h.alias === sshHost.trim()) ?? null,
    [discoveredHosts, sshHost]
  );

  return (
    <div className={s.page}>
      <header className={s.header}>
        <h1 className={s.title}>设置</h1>
      </header>
      <div className={s.body}>
        <div className={s.inner}>
          {/* 目标客户端管理 */}
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
                          <img
                            src={app.iconSrc}
                            alt=""
                            className={s.appIconImg}
                          />
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
                                  {sshCfg?.injectSecrets
                                    ? "注入密钥"
                                    : "仅占位符"}
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
                            onClick={() =>
                              void handleRowTestSsh(app.id, sshCfg)
                            }
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
                          未在 ~/.ssh/config 检测到主机别名，请在下方手动输入
                          Host 或 IP。
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
                                <span className={s.hostChipAlias}>
                                  {h.alias}
                                </span>
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
                          <code className={s.code}>~/.ssh/config</code>{" "}
                          的连接参数。
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
                                sshPreset === preset.id
                                  ? s.presetChipActive
                                  : ""
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
                            {newGlobalMcpPath
                              ? ` · MCP: ${newGlobalMcpPath}`
                              : ""}
                            {sshGlobalPluginDir
                              ? ` · Plugins: ${sshGlobalPluginDir}`
                              : ""}
                          </div>
                          {sshPreset !== "custom" && (
                            <button
                              type="button"
                              className={s.sshInlineLinkBtn}
                              onClick={() =>
                                setShowSshAdvancedPaths((prev) => !prev)
                              }
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
                              onChange={(e) =>
                                setNewGlobalSkillDir(e.target.value)
                              }
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
                              onChange={(e) =>
                                setNewGlobalCommandDir(e.target.value)
                              }
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
                              onChange={(e) =>
                                setNewGlobalMcpPath(e.target.value)
                              }
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
                              onChange={(e) =>
                                setSshGlobalPluginDir(e.target.value)
                              }
                              placeholder="例如：~/.claude/plugins/local"
                            />
                          </div>

                          <div className={s.customField}>
                            <span className={s.customFieldLabel}>
                              MCP 配置格式
                            </span>
                            <select
                              className={s.select}
                              value={newMcpFormat}
                              onChange={(e) =>
                                setNewMcpFormat(
                                  e.target.value as "json" | "toml"
                                )
                              }
                            >
                              <option value="json">JSON</option>
                              <option value="toml">TOML</option>
                            </select>
                          </div>

                          <div className={s.customField}>
                            <span className={s.customFieldLabel}>
                              MCP 根节点键名
                            </span>
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
                            onChange={(e) =>
                              setNewGlobalSkillDir(e.target.value)
                            }
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
                          onChange={(e) =>
                            setNewProjectSkillDir(e.target.value)
                          }
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
                          onChange={(e) =>
                            setNewGlobalCommandDir(e.target.value)
                          }
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
                          onChange={(e) =>
                            setNewProjectCommandDir(e.target.value)
                          }
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

          {/* 通用 */}
          <section className={s.card} data-section="general">
            <div className={s.cardHeader}>
              <div className={s.cardTitleWrap}>
                <div className={s.cardTitleIcon}>
                  <Languages size={15} />
                </div>
                <h2 className={s.cardTitle}>通用</h2>
              </div>
            </div>
            <div className={s.rows}>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>界面语言</div>
                  <div className={s.rowHint}>切换显示语言</div>
                </div>
                <select
                  className={s.select}
                  value={lang}
                  onChange={(e) => {
                    const locale =
                      e.target.value === "简体中文"
                        ? "zh-CN"
                        : e.target.value === "English"
                          ? "en-US"
                          : "zh-TW";
                    void handleSettingChange("locale", locale);
                  }}
                >
                  <option>简体中文</option>
                  <option>English</option>
                  <option>繁體中文</option>
                </select>
              </div>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>自动检查更新</div>
                  <div className={s.rowHint}>
                    启动时从仓库源拉取最新版本信息
                  </div>
                </div>
                <Toggle
                  checked={settings.autoCheckUpdates}
                  onChange={(v) =>
                    void handleSettingChange("autoCheckUpdates", v)
                  }
                />
              </div>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>开机自启</div>
                  <div className={s.rowHint}>登录后自动在后台运行</div>
                </div>
                <Toggle
                  checked={settings.autoStart}
                  onChange={(v) => void handleSettingChange("autoStart", v)}
                />
              </div>
            </div>
          </section>

          {/* 应用更新 */}
          <section className={s.card} data-section="updates">
            <div className={s.cardHeader}>
              <div className={s.cardTitleWrap}>
                <div className={s.cardTitleIcon}>
                  <RefreshCw size={15} />
                </div>
                <h2 className={s.cardTitle}>应用更新</h2>
              </div>
            </div>
            <div className={s.rows}>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>自动检查应用更新</div>
                  <div className={s.rowHint}>启动时检查 SkillSwitch 新版本</div>
                </div>
                <Toggle
                  checked={settings.autoCheckAppUpdates ?? true}
                  onChange={(v) =>
                    void updateSettings({ autoCheckAppUpdates: v })
                  }
                />
              </div>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>当前版本</div>
                  <div className={s.rowHint + " " + s.mono}>
                    v{currentVersion || "加载中..."}
                  </div>
                </div>
                <button
                  className={s.btn}
                  onClick={async () => {
                    setIsCheckingUpdate(true);
                    await checkForUpdates(false);
                    setIsCheckingUpdate(false);
                  }}
                  disabled={isCheckingUpdate || isChecking}
                >
                  {isCheckingUpdate || isChecking ? (
                    <>检查中...</>
                  ) : (
                    <>
                      <RefreshCw size={12} /> 检查更新
                    </>
                  )}
                </button>
              </div>
            </div>
          </section>

          {/* 备份 */}
          <section className={s.card} data-section="backup">
            <div className={s.cardHeader}>
              <div className={s.cardTitleWrap}>
                <div className={s.cardTitleIcon}>
                  <HardDrive size={15} />
                </div>
                <h2 className={s.cardTitle}>备份</h2>
              </div>
            </div>
            <div className={s.rows}>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>备份文件夹</div>
                  <div className={s.rowHint + " " + s.mono}>
                    {settings.backupPath || "加载中…"}
                  </div>
                  <div className={s.rowHint}>当前版本固定写入应用数据目录</div>
                </div>
                <button className={s.btn} onClick={handleOpenBackupFolder}>
                  <Folder size={14} /> 查看
                </button>
              </div>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>最大保留备份数</div>
                  <div className={s.rowHint}>
                    设置已保留，自动清理逻辑后续接入
                  </div>
                </div>
                <input
                  type="number"
                  className={s.numInput}
                  value={settings.maxBackups}
                  onChange={(e) =>
                    void handleSettingChange(
                      "maxBackups",
                      parseInt(e.target.value) || 10
                    )
                  }
                  min={1}
                  max={100}
                />
              </div>
            </div>
          </section>

          {/* Git 同步 */}
          <section className={s.card} data-section="github">
            <div className={s.cardHeader}>
              <div className={s.cardTitleWrap}>
                <div className={s.cardTitleIcon}>
                  <Github size={15} />
                </div>
                <h2 className={s.cardTitle}>Git 同步</h2>
              </div>
              <div className={s.cardHeaderActions}>
                <button
                  className={s.guideTrigger}
                  onClick={() => setShowBackupGuide(true)}
                  aria-label="查看 SSH 备份说明"
                  title="查看 SSH 备份说明"
                >
                  <Info size={14} />
                </button>
                <span className={s.backupStateOn}>{"已配置"}</span>
              </div>
            </div>
            <div className={s.rows}>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>自动同步到远端</div>
                  <div className={s.rowHint}>
                    配置后，新建/编辑/删除/导入 Skill 都会自动推送到 Git
                    仓库；仅同步自建 Skills。
                  </div>
                </div>
              </div>

              <div className={`${s.row} ${s.rowFull}`}>
                <div className={s.backupField}>
                  <div className={s.rowLabel}>仓库 SSH 地址</div>
                  <div className={s.rowHint}>
                    使用{" "}
                    <code className={s.code}>
                      git@github.com:owner/repo.git
                    </code>{" "}
                    格式。
                  </div>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={backupDraft.remoteUrl}
                    onChange={(e) =>
                      handleBackupDraftChange("remoteUrl", e.target.value)
                    }
                    placeholder="git@github.com:DargonLee/My-Skills.git"
                  />
                </div>
              </div>

              <div className={`${s.row} ${s.rowFull}`}>
                <div className={s.backupField}>
                  <div className={s.rowLabel}>目标分支</div>
                  <input
                    className={`${s.repoInput} ${s.repoInputWide}`}
                    value={backupDraft.branch}
                    onChange={(e) =>
                      handleBackupDraftChange("branch", e.target.value)
                    }
                    placeholder="main"
                  />
                </div>
              </div>

              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>上次同步</div>
                  <div className={s.rowHint}>
                    {formatSyncTime(backupDraft.lastSyncedAt)}
                  </div>
                </div>
              </div>

              <div className={`${s.row} ${s.rowFull}`}>
                <div className={s.backupField}>
                  <div className={s.rowLabel}>操作</div>
                  <div className={s.rowHint}>
                    保存配置后自动连接远端；手动推拉可在自动同步之外额外操作。
                  </div>
                </div>
                <div className={s.syncBtns}>
                  <button
                    className={s.btnPrimary}
                    disabled={
                      backupAction !== null || !backupDraft.remoteUrl.trim()
                    }
                    onClick={async () => {
                      const saved = await persistBackupSource(false);
                      if (!saved) return;
                      setBackupAction("test");
                      const connectResult = await backupSourceConnect();
                      if (!connectResult.ok) {
                        showToast(`连接失败：${connectResult.error}`);
                        setBackupAction(null);
                        return;
                      }
                      const connectedSource = syncBackupDraftFromStatus(
                        connectResult.value
                      );
                      await updateSettings({ backupSource: connectedSource });
                      refreshSources(BACKUP_SOURCE_REPO_ID);
                      await refreshSkills();
                      showToast(
                        buildBackupToast(
                          "已保存并连接成功",
                          connectResult.value.notice
                        )
                      );
                      setBackupAction(null);
                    }}
                  >
                    {backupAction === "test" ? "连接中…" : "保存并连接"}
                  </button>
                  <button
                    className={s.btn}
                    disabled={backupAction !== null}
                    onClick={() => void handleBackupSourceAction("push")}
                  >
                    {backupAction === "push" ? (
                      "推送中…"
                    ) : (
                      <>
                        <Upload size={12} /> 手动推送
                      </>
                    )}
                  </button>
                  <button
                    className={s.btn}
                    disabled={backupAction !== null}
                    onClick={() => void handleBackupSourceAction("pull")}
                  >
                    {backupAction === "pull" ? (
                      "拉取中…"
                    ) : (
                      <>
                        <Download size={12} /> 手动拉取
                      </>
                    )}
                  </button>
                </div>
              </div>
            </div>
          </section>

          {/* 外观 */}
          <section className={s.card} data-section="appearance">
            <div className={s.cardHeader}>
              <div className={s.cardTitleWrap}>
                <div className={s.cardTitleIcon}>
                  <Palette size={15} />
                </div>
                <h2 className={s.cardTitle}>外观</h2>
              </div>
            </div>
            <div className={s.rows}>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>主题</div>
                </div>
                <div className={s.themeGroup}>
                  {(["浅色", "深色", "跟随系统"] as const).map((t, i) => (
                    <label key={t} className={s.themeOption}>
                      <div
                        className={`${s.themePreview} ${
                          i === 1
                            ? s.themePreviewDark
                            : i === 2
                              ? s.themePreviewSystem
                              : s.themePreviewLight
                        } ${theme === t ? s.themePreviewActive : ""}`}
                      />
                      <div className={s.themeLabel}>
                        <input
                          type="radio"
                          name="theme"
                          checked={theme === t}
                          onChange={() => {
                            const themeValue =
                              t === "浅色"
                                ? "light"
                                : t === "深色"
                                  ? "dark"
                                  : "system";
                            void handleSettingChange("theme", themeValue);
                          }}
                        />{" "}
                        {t}
                      </div>
                    </label>
                  ))}
                </div>
              </div>
            </div>
          </section>

          {/* 危险操作 */}
          <section className={s.card} data-variant="danger">
            <div className={s.cardHeader}>
              <div className={s.cardTitleWrap}>
                <div className={s.cardTitleIcon}>
                  <AlertTriangle size={15} />
                </div>
                <h2 className={s.cardTitle}>危险操作</h2>
              </div>
            </div>
            <div className={s.rows}>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>清空所有备份</div>
                  <div className={s.rowHint}>删除全部快照文件，不可恢复</div>
                </div>
                <button className={s.btnDanger}>清空备份</button>
              </div>
              <div className={s.row}>
                <div className={s.rowBody}>
                  <div className={s.rowLabel}>重置所有设置</div>
                  <div className={s.rowHint}>
                    恢复默认配置，已安装 Skills 不受影响
                  </div>
                </div>
                <button className={s.btnDanger}>重置设置</button>
              </div>
            </div>
          </section>

          <div className={s.versionRow}>SkillSwitch v0.1.0</div>
        </div>
      </div>
      {showBackupGuide && (
        <div
          className={s.guideModalOverlay}
          onClick={() => setShowBackupGuide(false)}
        >
          <div className={s.guideModal} onClick={(e) => e.stopPropagation()}>
            <div className={s.guideModalHeader}>
              <div>
                <div className={s.guideModalEyebrow}>SSH 备份说明</div>
                <h3 className={s.guideModalTitle}>
                  按这个顺序接入 GitHub 备份
                </h3>
              </div>
              <button
                className={s.guideModalClose}
                onClick={() => setShowBackupGuide(false)}
                aria-label="关闭说明"
              >
                <X size={14} />
              </button>
            </div>
            <div className={s.guideModalBody}>
              <ol className={s.backupSteps}>
                <li>
                  <strong>1. 在 GitHub 新建仓库</strong>
                  <span>
                    建议为 SkillSwitch
                    单独准备一个备份仓库。这个仓库只用来同步你自建的 Skills。
                  </span>
                </li>
                <li>
                  <strong>2. 在本机生成 SSH key</strong>
                  <span>
                    如果还没有 key，可以在终端运行{" "}
                    <code className={s.code}>
                      ssh-keygen -t ed25519 -C &quot;you@example.com&quot;
                    </code>
                    。
                  </span>
                </li>
                <li>
                  <strong>3. 把公钥上传到 GitHub</strong>
                  <span>
                    运行{" "}
                    <code className={s.code}>cat ~/.ssh/id_ed25519.pub</code>
                    ，复制输出内容后到 GitHub 的 SSH keys 页面添加。
                  </span>
                </li>
                <li>
                  <strong>4. 测试 SSH 连接</strong>
                  <span>
                    运行 <code className={s.code}>ssh -T git@github.com</code>
                    ，确认 GitHub 认得这台电脑。
                  </span>
                </li>
                <li>
                  <strong>5. 回到这里填写并同步</strong>
                  <span>
                    填入 SSH 地址和分支，界面会自动解析出{" "}
                    <code className={s.code}>owner/repo</code>
                    ，然后再连接、推送或拉取。
                  </span>
                </li>
              </ol>
              <div className={s.backupHintRow}>
                <Terminal size={12} />
                <span>
                  远端仓库会维护一份独立备份工作树，只包含自建
                  Skills；第三方仓库安装项不会被推送进去。
                </span>
              </div>
            </div>
          </div>
        </div>
      )}
      {toast && <div className={s.toast}>{toast}</div>}
    </div>
  );
}
