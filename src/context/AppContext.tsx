import { createContext, useContext, useMemo, type ReactNode } from "react";
import claudeCodeIcon from "../assets/cli-icons/claude-code.png";
import codexIcon from "../assets/cli-icons/codex.png";
import cursorIcon from "../assets/cli-icons/cursor.png";
import antigravityIcon from "../assets/cli-icons/antigravity.svg";
import t3CodeIcon from "../assets/cli-icons/t3-code.svg";
import { useSettings } from "./SettingsContext";
import type { CustomAppConfig } from "../types";

export interface TargetAppMeta {
  id: string;
  label: string;
  shortLabel: string;
  iconSrc?: string;
  accentColor: string;
  skillPathLabel: string;
  projectSkillRelDir: string | null;
  commandPathLabel: string | null;
  projectCommandRelDir: string | null;
  mcpSupported: boolean;
  mcpGlobalPathLabel: string | null;
  mcpProjectPathLabel: string | null;
  mcpUnsupportedNote?: string | null;
  isBuiltin: boolean;
  isSsh: boolean;
  sshHostLabel: string | null;
}

export interface ProjectSkillTargetGroup {
  groupKey: string;
  relPathLabel: string;
  apps: TargetAppMeta[];
  combinedLabel: string;
  accentColor: string;
  isShared: boolean;
}

function normalizeRelDir(raw?: string | null): string | null {
  if (!raw) return null;
  const cleaned = raw
    .trim()
    .replace(/^\.?\//, "")
    .replace(/^\\+/, "")
    .replace(/\/+$/, "");
  return cleaned ? cleaned : null;
}

function formatDirDisplay(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return "";
  return trimmed.endsWith("/") ? trimmed : `${trimmed}/`;
}

function deriveShortLabel(label: string): string {
  const trimmed = label.trim();
  if (!trimmed) return "AP";
  const words = trimmed.split(/[\s\-_]+/).filter(Boolean);
  if (words.length >= 2) {
    return `${words[0][0]}${words[1][0]}`.toUpperCase();
  }
  return trimmed.slice(0, 2).toUpperCase();
}

// Built-in AI CLI & Desktop applications
export const APP_LIST: readonly TargetAppMeta[] = [
  {
    id: "claude",
    label: "Claude Code",
    shortLabel: "CC",
    iconSrc: claudeCodeIcon,
    accentColor: "#2563eb",
    skillPathLabel: "~/.claude/skills/",
    projectSkillRelDir: ".claude/skills",
    commandPathLabel: "~/.claude/commands/",
    projectCommandRelDir: ".claude/commands",
    mcpSupported: true,
    mcpGlobalPathLabel: "~/.claude.json",
    mcpProjectPathLabel: "~/.claude.json (local) / .mcp.json",
    isBuiltin: true,
    isSsh: false,
    sshHostLabel: null,
  },
  {
    id: "codex",
    label: "Codex",
    shortLabel: "CX",
    iconSrc: codexIcon,
    accentColor: "#7c3aed",
    skillPathLabel: "~/.codex/skills/",
    projectSkillRelDir: ".codex/skills",
    commandPathLabel: "~/.codex/commands/",
    projectCommandRelDir: ".codex/commands",
    mcpSupported: true,
    mcpGlobalPathLabel: "~/.codex/config.toml",
    mcpProjectPathLabel: ".codex/config.toml",
    isBuiltin: true,
    isSsh: false,
    sshHostLabel: null,
  },
  {
    id: "cursor",
    label: "Cursor",
    shortLabel: "CU",
    iconSrc: cursorIcon,
    accentColor: "#0891b2",
    skillPathLabel: "~/.cursor/skills/",
    projectSkillRelDir: ".cursor/skills",
    commandPathLabel: "~/.cursor/commands/",
    projectCommandRelDir: ".cursor/commands",
    mcpSupported: true,
    mcpGlobalPathLabel: "~/.cursor/mcp.json",
    mcpProjectPathLabel: ".cursor/mcp.json",
    isBuiltin: true,
    isSsh: false,
    sshHostLabel: null,
  },
  {
    id: "antigravity",
    label: "Antigravity",
    shortLabel: "AG",
    iconSrc: antigravityIcon,
    accentColor: "#16a34a",
    skillPathLabel: "~/.gemini/config/skills/",
    projectSkillRelDir: ".agents/skills",
    commandPathLabel: null,
    projectCommandRelDir: null,
    mcpSupported: true,
    mcpGlobalPathLabel: "~/.gemini/config/mcp_config.json",
    mcpProjectPathLabel: ".agents/mcp_config.json",
    isBuiltin: true,
    isSsh: false,
    sshHostLabel: null,
  },
  {
    id: "gemini",
    label: "Gemini CLI",
    shortLabel: "GM",
    accentColor: "#0284c7",
    skillPathLabel: "~/.gemini/skills/",
    projectSkillRelDir: ".gemini/skills",
    commandPathLabel: "~/.gemini/commands/",
    projectCommandRelDir: ".gemini/commands",
    mcpSupported: true,
    mcpGlobalPathLabel: "~/.gemini/settings.json",
    mcpProjectPathLabel: ".gemini/settings.json",
    isBuiltin: true,
    isSsh: false,
    sshHostLabel: null,
  },
  {
    id: "windsurf",
    label: "Windsurf",
    shortLabel: "WS",
    accentColor: "#0d9488",
    skillPathLabel: "~/.codeium/windsurf/skills/",
    projectSkillRelDir: ".windsurf/skills",
    commandPathLabel: "~/.codeium/windsurf/workflows/",
    projectCommandRelDir: ".windsurf/workflows",
    mcpSupported: true,
    mcpGlobalPathLabel: "~/.codeium/windsurf/mcp_config.json",
    mcpProjectPathLabel: null,
    isBuiltin: true,
    isSsh: false,
    sshHostLabel: null,
  },
  {
    id: "t3",
    label: "T3 Code",
    shortLabel: "T3",
    iconSrc: t3CodeIcon,
    accentColor: "#db2777",
    skillPathLabel: "~/.agents/skills/",
    projectSkillRelDir: ".agents/skills",
    commandPathLabel: null,
    projectCommandRelDir: null,
    mcpSupported: false,
    mcpGlobalPathLabel: null,
    mcpProjectPathLabel: null,
    mcpUnsupportedNote:
      "T3 Code 通过底层 Claude / Codex / Antigravity 会话驱动 MCP，请直接在对应底层 CLI 启用",
    isBuiltin: true,
    isSsh: false,
    sshHostLabel: null,
  },
];

export type AppId = string;

function customConfigToMeta(custom: CustomAppConfig): TargetAppMeta {
  const sshHostTrimmed = custom.ssh?.host?.trim() || "";
  const isSsh = Boolean(sshHostTrimmed);
  const sshUserTrimmed = custom.ssh?.user?.trim() || "";
  const sshHostLabel = isSsh
    ? sshUserTrimmed && !sshHostTrimmed.includes("@")
      ? `${sshUserTrimmed}@${sshHostTrimmed}`
      : sshHostTrimmed
    : null;

  const projectSkillRelDir = isSsh
    ? null
    : normalizeRelDir(custom.projectSkillDir);
  const projectCommandRelDir = isSsh
    ? null
    : normalizeRelDir(custom.projectCommandDir);
  const globalCommandTrimmed = custom.globalCommandDir?.trim() || "";
  const globalMcpTrimmed = custom.globalMcpConfigPath?.trim() || "";
  const projectMcpTrimmed = isSsh
    ? ""
    : custom.projectMcpConfigPath?.trim() || "";
  const mcpSupported = Boolean(globalMcpTrimmed || projectMcpTrimmed);

  const formattedSkillDir = formatDirDisplay(custom.globalSkillDir);
  const formattedCommandDir = globalCommandTrimmed
    ? formatDirDisplay(globalCommandTrimmed)
    : null;

  return {
    id: custom.id,
    label: custom.label,
    shortLabel: deriveShortLabel(custom.label),
    accentColor: custom.accentColor || "#6366f1",
    skillPathLabel: isSsh
      ? `${sshHostLabel}:${formattedSkillDir}`
      : formattedSkillDir,
    projectSkillRelDir,
    commandPathLabel: formattedCommandDir
      ? isSsh
        ? `${sshHostLabel}:${formattedCommandDir}`
        : formattedCommandDir
      : null,
    projectCommandRelDir,
    mcpSupported,
    mcpGlobalPathLabel: globalMcpTrimmed
      ? isSsh
        ? `${sshHostLabel}:${globalMcpTrimmed}`
        : globalMcpTrimmed
      : null,
    mcpProjectPathLabel: isSsh ? null : projectMcpTrimmed || null,
    mcpUnsupportedNote: mcpSupported
      ? null
      : isSsh
        ? "未在设置中配置该 SSH 远程目标端的全局 MCP 配置文件路径"
        : "未在设置中配置该自定义客户端的 MCP 配置文件路径",
    isBuiltin: false,
    isSsh,
    sshHostLabel,
  };
}

interface AppContextValue {
  allApps: TargetAppMeta[];
  visibleSkillApps: TargetAppMeta[];
  visibleCommandApps: TargetAppMeta[];
  visibleMcpApps: TargetAppMeta[];
  projectSkillGroups: ProjectSkillTargetGroup[];
  defaultInstallAppIds: string[];
  getAppMeta: (appId: string) => TargetAppMeta | null;
}

const AppContext = createContext<AppContextValue | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
  const { settings } = useSettings();

  const value = useMemo<AppContextValue>(() => {
    const customMetas = (settings.customApps ?? []).map(customConfigToMeta);
    const allApps: TargetAppMeta[] = [...APP_LIST, ...customMetas];
    const hiddenSet = new Set(settings.hiddenAppIds ?? []);

    const visibleSkillApps = allApps.filter((app) => !hiddenSet.has(app.id));
    const visibleCommandApps = allApps.filter(
      (app) => !hiddenSet.has(app.id) && Boolean(app.commandPathLabel)
    );
    const visibleMcpApps = allApps.filter(
      (app) => !hiddenSet.has(app.id) && app.mcpSupported
    );

    const groupMap = new Map<string, TargetAppMeta[]>();
    for (const app of visibleSkillApps) {
      if (!app.projectSkillRelDir) continue;
      const key = app.projectSkillRelDir.toLowerCase();
      const bucket = groupMap.get(key);
      if (bucket) {
        bucket.push(app);
      } else {
        groupMap.set(key, [app]);
      }
    }

    const projectSkillGroups: ProjectSkillTargetGroup[] = Array.from(
      groupMap.entries()
    ).map(([groupKey, apps]) => ({
      groupKey,
      relPathLabel: `${apps[0].projectSkillRelDir}/`,
      apps,
      combinedLabel: apps.map((a) => a.label).join(" + "),
      accentColor: apps[0].accentColor,
      isShared: apps.length > 1,
    }));

    const validIds = new Set(allApps.map((a) => a.id));
    const defaultInstallAppIds = (settings.defaultInstallAppIds ?? []).filter(
      (id) => validIds.has(id)
    );

    const byId = new Map(allApps.map((app) => [app.id, app]));
    const getAppMeta = (appId: string) => byId.get(appId) ?? null;

    return {
      allApps,
      visibleSkillApps,
      visibleCommandApps,
      visibleMcpApps,
      projectSkillGroups,
      defaultInstallAppIds,
      getAppMeta,
    };
  }, [
    settings.customApps,
    settings.hiddenAppIds,
    settings.defaultInstallAppIds,
  ]);

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export function useApps(): AppContextValue {
  const context = useContext(AppContext);
  if (!context) {
    throw new Error("useApps must be used within AppProvider");
  }
  return context;
}
