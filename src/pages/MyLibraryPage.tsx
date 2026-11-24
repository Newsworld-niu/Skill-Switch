import {
  useState,
  useEffect,
  useRef,
  useCallback,
  type CSSProperties,
} from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { readDir, watch } from "@tauri-apps/plugin-fs";
import {
  X,
  Search,
  Trash2,
  GitBranch,
  Clock,
  Globe,
  Folder,
  Plus,
  AlertTriangle,
  FolderOpen,
  Loader,
  FileUp,
  File,
  FileCode,
  FileText,
  Image,
  ChevronRight,
  ArrowLeft,
  ExternalLink,
  Download,
} from "lucide-react";
import { useSkills } from "../context/SkillContext";
import {
  useApps,
  type TargetAppMeta,
  type ProjectSkillTargetGroup,
} from "../context/AppContext";
import { useToast } from "../components/ui/Toast";
import {
  skillInstallToProject,
  skillUninstallFromProject,
  skillCheckSymlinkStatus,
  skillImportFromDialog,
  skillImportFromFolder,
  skillExportToZip,
  skillListDirectory,
  skillReadFile,
  skillShowInFinder,
  skillGetSourceDirPath,
  openWithTypora,
  showInFinder,
  readExternalSkillContent,
} from "../services/skill";
import { IconButton } from "../components/ui/IconButton";
import type {
  Skill,
  SkillDirectoryListing,
  SkillDirectoryEntry,
  SkillFileContent,
  ExternalSkill,
  Provenance,
} from "../types";
import modalStyles from "../components/layout/AppShell.module.css";
import s from "./MyLibraryPage.module.css";

const SKILL_MD_PATH_RE = /(?:^|[\\/])SKILL\.md$/i;

// Generate icon colors based on skill name
function getIconColors(name: string): { bg: string; fg: string } {
  const palettes = [
    { bg: "#6366f1", fg: "#ffffff" },
    { bg: "#22c55e", fg: "#ffffff" },
    { bg: "#ef4444", fg: "#ffffff" },
    { bg: "#06b6d4", fg: "#ffffff" },
    { bg: "#f97316", fg: "#ffffff" },
    { bg: "#ec4899", fg: "#ffffff" },
    { bg: "#8b5cf6", fg: "#ffffff" },
    { bg: "#0ea5e9", fg: "#ffffff" },
  ];
  return palettes[name.charCodeAt(0) % palettes.length];
}

// Format timestamp to readable date
function formatDate(ts: number): string {
  return new Date(ts).toLocaleDateString("zh-CN");
}

function getCompactAppName(app: TargetAppMeta): string {
  switch (app.id) {
    case "claude":
      return "Claude";
    case "codex":
      return "Codex";
    case "cursor":
      return "Cursor";
    case "antigravity":
      return "反重力";
    case "t3":
      return "T3";
    default:
      return app.label.length > 8 ? app.label.slice(0, 7) + "…" : app.label;
  }
}

function AppIcon({ app }: { app: TargetAppMeta }) {
  if (app.iconSrc) {
    return <img src={app.iconSrc} alt="" className={s.appMiniIcon} />;
  }
  return (
    <span className={s.appMiniMonogram} style={{ background: app.accentColor }}>
      {app.shortLabel}
    </span>
  );
}

// Get provenance badge label
function getProvenanceBadge(provenance?: Provenance): string {
  if (!provenance) return "";
  switch (provenance.kind) {
    case "manual":
      return "";
    case "file-import":
      return "导入";
    case "external-app":
      return provenance.label || "外部导入";
    case "marketplace":
      return "市场导入";
    case "repo-source":
      return provenance.sourceName
        ? `仓库源 · ${provenance.sourceName}`
        : "仓库源导入";
    case "plugin":
      return provenance.sourceName
        ? `插件 · ${provenance.sourceName}`
        : provenance.label || "插件投射";
    default:
      return "";
  }
}

interface ExternalImportPreviewState {
  skill: ExternalSkill;
  entries: Array<{
    name: string;
    kind: "file" | "directory";
    isSymlink: boolean;
  }>;
  duplicateSkill: Skill | null;
}

// ── Skill Card (left list) ───────────────────────────────────────────────────
function SkillCard({
  skill,
  selected,
  enabledAppIds,
  visibleApps,
  onToggleApp,
  onClick,
}: {
  skill: Skill;
  selected: boolean;
  enabledAppIds: string[];
  visibleApps: TargetAppMeta[];
  onToggleApp: (appId: string) => void;
  onClick: () => void;
}) {
  const iconColors = getIconColors(skill.name);
  const initial = skill.name.charAt(0).toUpperCase();
  const badge = getProvenanceBadge(skill.provenance);
  const enabledSet = new Set(enabledAppIds);

  return (
    <div
      className={`${s.card} ${selected ? s.cardSelected : ""}`}
      onClick={onClick}
    >
      <div className={s.cardContent}>
        {/* 左侧图标容器 */}
        <div
          className={s.cardIcon}
          style={{ background: iconColors.bg, color: iconColors.fg }}
        >
          <span>{initial}</span>
        </div>

        {/* 主体内容 */}
        <div className={s.cardBody}>
          <div className={s.cardHeader}>
            <span className={s.cardName}>{skill.name}</span>
            {badge && (
              <span className={s.cardBadge}>
                <Globe size={10} /> {badge}
              </span>
            )}
          </div>
          <p className={s.cardDesc}>{skill.description || "无描述"}</p>
          <div className={s.cardFooter}>
            <span className={s.cardSlug}>{skill.slug}</span>
            {visibleApps.length > 0 && (
              <div className={s.cardAppPills}>
                {visibleApps.map((app) => {
                  const active = enabledSet.has(app.id);
                  const pillStyle = {
                    "--pill-accent": app.accentColor,
                  } as CSSProperties;
                  return (
                    <button
                      key={app.id}
                      type="button"
                      className={`${s.cardAppPill} ${active ? s.cardAppPillActive : ""}`}
                      style={pillStyle}
                      title={
                        active
                          ? `已在全局 ${app.label} 启用（点击关闭）`
                          : `点击在全局 ${app.label} 启用 (${app.skillPathLabel})`
                      }
                      onClick={(event) => {
                        event.stopPropagation();
                        onToggleApp(app.id);
                      }}
                    >
                      {app.iconSrc ? (
                        <img
                          src={app.iconSrc}
                          alt=""
                          className={s.cardAppPillIcon}
                        />
                      ) : (
                        <span className={s.cardAppPillDot} />
                      )}
                      <span>{getCompactAppName(app)}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── External Skill Card (from app directory, not managed by SkillSwitch) ───────
function ExternalSkillCard({
  skill,
  selected,
  onImport,
  onClick,
}: {
  skill: ExternalSkill;
  selected?: boolean;
  onImport: () => void;
  onClick?: () => void;
}) {
  const iconColors = getIconColors(skill.name);
  const initial = skill.name.charAt(0).toUpperCase();

  return (
    <div
      className={`${s.card} ${s.cardExternal} ${selected ? s.cardSelected : ""}`}
      onClick={onClick}
      style={{ cursor: onClick ? "pointer" : undefined }}
    >
      <div className={s.cardContent}>
        <div
          className={s.cardIcon}
          style={{ background: iconColors.bg, color: iconColors.fg }}
        >
          <span>{initial}</span>
        </div>
        <div className={s.cardBody}>
          <div className={s.cardHeader}>
            <span className={s.cardName}>{skill.name}</span>
            <span
              className={s.cardBadgeExternal}
              style={{ cursor: "pointer" }}
              title="在 Finder 中显示"
              onClick={(e) => {
                e.stopPropagation();
                void showInFinder(skill.path);
              }}
            >
              <ExternalLink size={10} /> 外部
            </span>
          </div>
          <p className={s.cardDesc}>{skill.description || "无描述"}</p>
          <div className={s.cardFooter}>
            <span className={s.cardSlug}>{skill.slug}</span>
          </div>
        </div>
        <button
          className={s.cardImportBtn}
          onClick={(e) => {
            e.stopPropagation();
            onImport();
          }}
          title="导入到 SkillSwitch 管理"
        >
          <Download size={14} />
        </button>
      </div>
    </div>
  );
}

// ── External Detail Panel (read-only) ────────────────────────────────────────
function ExternalDetailPanel({
  skill,
  onImport,
}: {
  skill: ExternalSkill;
  onImport: () => void;
}) {
  const { getAppMeta } = useApps();
  const iconColors = getIconColors(skill.name);
  const initial = skill.name.charAt(0).toUpperCase();
  const appMeta = getAppMeta(skill.appId);
  const [content, setContent] = useState<string | null>(null);
  const [contentLoading, setContentLoading] = useState(true);
  const [contentError, setContentError] = useState<string | null>(null);

  useEffect(() => {
    setContentLoading(true);
    setContentError(null);
    readExternalSkillContent(skill.path)
      .then((result) => {
        if (result.ok) {
          setContent(result.value);
        } else {
          setContentError(result.error);
        }
        setContentLoading(false);
      })
      .catch((err) => {
        setContentError(String(err));
        setContentLoading(false);
      });
  }, [skill.path]);

  return (
    <aside className={s.detail}>
      <div className={s.detailInner}>
        {/* Header */}
        <div className={s.detailHero}>
          <div
            className={s.detailIcon}
            style={{ background: iconColors.bg, color: iconColors.fg }}
          >
            <span>{initial}</span>
          </div>
          <div className={s.detailHeroText}>
            <h2 className={s.detailName}>{skill.name}</h2>
            <span className={s.detailSlug}>{skill.slug}</span>
          </div>
          <IconButton
            icon={<ExternalLink size={16} />}
            variant="default"
            title="在 Finder 中显示"
            onClick={() => void showInFinder(skill.path)}
            aria-label="在 Finder 中显示"
          />
          <IconButton
            icon={<Download size={16} />}
            variant="default"
            title="导入到 SkillSwitch 管理"
            onClick={onImport}
            aria-label="导入到 SkillSwitch 管理"
          />
        </div>

        {/* Meta info */}
        <div className={s.externalDetailMeta}>
          {appMeta && (
            <div className={s.externalDetailRow}>
              <span className={s.externalDetailLabel}>来源</span>
              <span className={s.externalDetailValue}>
                <AppIcon app={appMeta} /> {appMeta.label}
              </span>
            </div>
          )}
          <div className={s.externalDetailRow}>
            <span className={s.externalDetailLabel}>路径</span>
            <span
              className={s.externalDetailValue}
              style={{ fontSize: "0.72rem", wordBreak: "break-all" }}
            >
              {skill.path}
            </span>
          </div>
          {skill.isSymlink && (
            <div className={s.externalDetailRow}>
              <span className={s.externalDetailLabel}>类型</span>
              <span className={s.externalDetailValue}>
                符号链接{skill.symlinkTarget ? ` → ${skill.symlinkTarget}` : ""}
              </span>
            </div>
          )}
        </div>

        {/* Description */}
        {skill.description && (
          <div className={s.externalDetailDesc}>
            <p>{skill.description}</p>
          </div>
        )}

        {/* SKILL.md content preview */}
        <div className={s.externalDetailContentSection}>
          <div className={s.externalDetailContentHeader}>
            <FileText size={14} />
            <span>SKILL.md 内容预览</span>
          </div>
          <div className={s.externalDetailContentBody}>
            {contentLoading ? (
              <div className={s.externalDetailContentLoading}>
                <Loader size={16} className={s.btnSpin} />
                <span>加载中...</span>
              </div>
            ) : contentError ? (
              <div className={s.externalDetailContentError}>
                <AlertTriangle size={16} />
                <span>{contentError}</span>
              </div>
            ) : content ? (
              <pre className={s.externalDetailContentCode}>{content}</pre>
            ) : (
              <div className={s.externalDetailContentEmpty}>无内容</div>
            )}
          </div>
        </div>
      </div>
    </aside>
  );
}

// ── Project enable state interface
interface ProjectEnableState {
  projectId: string;
  projectName: string;
  projectPath: string;
  apps: Record<string, boolean>;
}

// ── Detail Panel Tabs ────────────────────────────────────────────────────────
type Tab = "enable" | "skillmd" | "files";
type LibraryGroupTab = "self-created" | "external";

// ── File icon based on extension ─────────────────────────────────────────────
function getFileIcon(entry: SkillDirectoryEntry): React.ReactNode {
  if (entry.kind === "directory") {
    return <Folder size={18} />;
  }

  const ext = entry.extension?.toLowerCase();
  switch (ext) {
    case "md":
      return <FileText size={18} />;
    case "ts":
    case "tsx":
    case "js":
    case "jsx":
    case "json":
      return <FileCode size={18} />;
    case "png":
    case "jpg":
    case "jpeg":
    case "gif":
    case "svg":
      return <Image size={18} />;
    default:
      return <File size={18} />;
  }
}

// ── Format file size ─────────────────────────────────────────────────────────
function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// ── Directory Browser Component ──────────────────────────────────────────────
function DirectoryBrowser({
  skill,
  onOpenFile,
}: {
  skill: Skill;
  onOpenFile: (path: string) => void;
}) {
  const [listing, setListing] = useState<SkillDirectoryListing | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [currentPath, setCurrentPath] = useState<string>("");

  const loadDirectory = useCallback(
    async (subPath: string = "") => {
      setLoading(true);
      setError(null);
      const result = await skillListDirectory({
        skillId: skill.id,
        subPath: subPath || null,
      });
      if (result.ok) {
        setListing(result.value);
        setCurrentPath(result.value.currentPath);
      } else {
        setError(result.error);
      }
      setLoading(false);
    },
    [skill.id]
  );

  useEffect(() => {
    void loadDirectory("");
  }, [loadDirectory]);

  const handleEntryClick = (entry: SkillDirectoryEntry) => {
    if (entry.kind === "directory") {
      void loadDirectory(entry.path);
    } else {
      onOpenFile(entry.path);
    }
  };

  const handleBackToParent = () => {
    if (listing?.parentPath !== undefined) {
      void loadDirectory(listing.parentPath || "");
    }
  };

  const handlePathClick = (path: string) => {
    void loadDirectory(path);
  };

  if (loading) {
    return (
      <div className={s.directoryBrowser}>
        <div className={s.emptyDirectory}>
          <Loader size={24} className={s.btnSpin} />
          <span>加载中...</span>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className={s.directoryBrowser}>
        <div className={s.emptyDirectory}>
          <AlertTriangle size={24} />
          <span>{error}</span>
        </div>
      </div>
    );
  }

  if (!listing) return null;

  const pathSegments = currentPath.split("/").filter(Boolean);

  return (
    <div className={s.directoryBrowser}>
      <div className={s.directoryHeader}>
        <Folder size={14} />
        <div className={s.directoryPath}>
          <span
            className={s.pathSegmentLink}
            onClick={() => handlePathClick("")}
          >
            {skill.slug}
          </span>
          {pathSegments.map((segment, idx) => (
            <span key={idx} className={s.pathSegment}>
              <ChevronRight size={12} className={s.pathSeparator} />
              <span
                className={s.pathSegmentLink}
                onClick={() =>
                  handlePathClick(pathSegments.slice(0, idx + 1).join("/"))
                }
              >
                {segment}
              </span>
            </span>
          ))}
        </div>
        {currentPath && (
          <button className={s.backBtn} onClick={handleBackToParent}>
            <ArrowLeft size={12} /> 上级
          </button>
        )}
      </div>
      <div className={s.directoryList}>
        {listing.entries.length === 0 ? (
          <div className={s.emptyDirectory}>
            <FolderOpen size={24} />
            <span>此目录为空</span>
          </div>
        ) : (
          listing.entries.map((entry) => (
            <div
              key={entry.path}
              className={s.directoryEntry}
              onClick={() => handleEntryClick(entry)}
            >
              <div
                className={`${s.entryIcon} ${entry.kind === "directory" ? s.entryIconDir : s.entryIconFile}`}
              >
                {getFileIcon(entry)}
              </div>
              <div className={s.entryInfo}>
                <div className={s.entryName}>{entry.name}</div>
                {entry.kind === "file" && entry.size != null && (
                  <div className={s.entryMeta}>{formatSize(entry.size)}</div>
                )}
              </div>
              {entry.kind === "directory" && (
                <ChevronRight size={16} className={s.pathSeparator} />
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}

// ── File Preview Component ───────────────────────────────────────────────────
function FilePreview({
  skill,
  filePath,
  onBack,
}: {
  skill: Skill;
  filePath: string;
  onBack: () => void;
}) {
  const [content, setContent] = useState<SkillFileContent | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const loadFile = async () => {
      setLoading(true);
      setError(null);
      const result = await skillReadFile({ skillId: skill.id, filePath });
      if (result.ok) {
        setContent(result.value);
      } else {
        setError(result.error);
      }
      setLoading(false);
    };
    void loadFile();
  }, [skill.id, filePath]);

  const fileName = filePath.split("/").pop() || filePath;

  return (
    <div className={s.filePreview}>
      <div className={s.filePreviewHeader}>
        <button className={s.backBtn} onClick={onBack}>
          <ArrowLeft size={12} /> 返回
        </button>
        <span className={s.fileName}>{fileName}</span>
      </div>
      <div className={s.filePreviewContent}>
        {loading ? (
          <span>加载中...</span>
        ) : error ? (
          <span style={{ color: "var(--error)" }}>{error}</span>
        ) : (
          content?.content || ""
        )}
      </div>
    </div>
  );
}

function DetailPanel({
  skill,
  onDelete,
  onExport,
  onSyncFromSource,
}: {
  skill: Skill;
  onDelete: () => void;
  onExport: () => void;
  onSyncFromSource: (id: string) => void;
}) {
  const { visibleSkillApps, projectSkillGroups, allApps } = useApps();
  const { globalEnabledBySkill, toggleSkillGlobalApp, refreshGlobalStatuses } =
    useSkills();
  const [tab, setTab] = useState<Tab>("enable");
  const [previewingFile, setPreviewingFile] = useState<string | null>(null);
  const iconColors = getIconColors(skill.name);
  const initial = skill.name.charAt(0).toUpperCase();
  const toast = useToast();
  const [skillmdContent, setSkillmdContent] = useState(skill.content ?? "");
  const [skillmdLoading, setSkillmdLoading] = useState(false);
  const [skillmdError, setSkillmdError] = useState<string | null>(null);

  const enabledGlobalSet = new Set(globalEnabledBySkill[skill.id] ?? []);

  // Refresh global statuses from real disk symlinks on skill switch / tab switch
  useEffect(() => {
    if (tab === "enable") {
      void refreshGlobalStatuses();
    }
  }, [refreshGlobalStatuses, skill.id, tab]);

  // Read SKILL.md fresh from disk on skill switch, then poll for external edits
  useEffect(() => {
    let disposed = false;
    let sourceDir = "";
    let inFlight = false;
    let prevContent: string | null = null;

    const readContent = async () => {
      if (!sourceDir || inFlight) return;
      inFlight = true;
      const result = await readExternalSkillContent(sourceDir);
      inFlight = false;
      if (disposed) return;
      if (result.ok) {
        const newContent = result.value;
        if (prevContent !== null && prevContent !== newContent) {
          void onSyncFromSource(skill.id);
        }
        prevContent = newContent;
        setSkillmdContent(newContent);
        setSkillmdError(null);
      } else {
        setSkillmdError(result.error);
      }
      setSkillmdLoading(false);
    };

    setSkillmdLoading(true);
    setSkillmdError(null);

    void skillGetSourceDirPath(skill.id).then((dirResult) => {
      if (disposed) return;
      if (!dirResult.ok) {
        setSkillmdError(dirResult.error);
        setSkillmdLoading(false);
        return;
      }
      sourceDir = dirResult.value;
      void readContent();
    });

    const timer = setInterval(readContent, 2000);

    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, [skill.id, onSyncFromSource]);

  // Project-level list persisted per skillId, with real-time symlink status hydrated from disk
  const storageKey = `skill-projects-${skill.id}`;
  const [projectEnables, setProjectEnablesRaw] = useState<ProjectEnableState[]>(
    () => {
      try {
        const stored = localStorage.getItem(storageKey);
        if (stored) return JSON.parse(stored) as ProjectEnableState[];
      } catch {}
      return [];
    }
  );

  const setProjectEnables = useCallback(
    (
      updater:
        | ProjectEnableState[]
        | ((prev: ProjectEnableState[]) => ProjectEnableState[])
    ) => {
      setProjectEnablesRaw((prev) => {
        const next = typeof updater === "function" ? updater(prev) : updater;
        try {
          localStorage.setItem(storageKey, JSON.stringify(next));
        } catch {}
        return next;
      });
    },
    [storageKey]
  );

  const syncProjectDiskStatuses = useCallback(
    async (projects: ProjectEnableState[]) => {
      if (projects.length === 0) return;
      const checks = await Promise.all(
        projects.map((proj) =>
          skillCheckSymlinkStatus({
            skillId: skill.id,
            scope: "project",
            projectPath: proj.projectPath,
          })
        )
      );

      setProjectEnables((prev) =>
        prev.map((proj) => {
          const idx = projects.findIndex((p) => p.projectId === proj.projectId);
          const check = idx >= 0 ? checks[idx] : null;
          if (!check || !check.ok) return proj;
          const nextApps: Record<string, boolean> = {};
          for (const st of check.value.statuses) {
            nextApps[st.appId] = st.exists;
          }
          return { ...proj, apps: nextApps };
        })
      );
    },
    [setProjectEnables, skill.id]
  );

  useEffect(() => {
    if (tab === "enable" && projectEnables.length > 0) {
      void syncProjectDiskStatuses(projectEnables);
    }
    // Only trigger when switching skill or returning to enable tab
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [skill.id, tab]);

  useEffect(() => {
    setTab("enable");
    setPreviewingFile(null);
  }, [skill.id]);

  const handleTabChange = (nextTab: Tab) => {
    if (nextTab === tab) return;
    setTab(nextTab);
  };

  const toggleProjectGroup = async (
    projectIdx: number,
    group: ProjectSkillTargetGroup
  ) => {
    const project = projectEnables[projectIdx];
    if (!project) return;

    const groupAppIds = group.apps.map((a) => a.id);
    const currentState = groupAppIds.some((id) => !!project.apps[id]);
    const newState = !currentState;

    // Optimistically update all apps in this shared path group
    setProjectEnables((prev) =>
      prev.map((p, i) => {
        if (i !== projectIdx) return p;
        const nextApps = { ...p.apps };
        for (const id of groupAppIds) {
          nextApps[id] = newState;
        }
        return { ...p, apps: nextApps };
      })
    );

    const tid = newState
      ? toast.loading(`正在写入 ${group.combinedLabel}…`)
      : toast.loading(`正在移除 ${group.combinedLabel}…`);

    if (newState) {
      // One symlink writes the target directory for the whole shared group
      const result = await skillInstallToProject({
        skillId: skill.id,
        projectPath: project.projectPath,
        apps: [groupAppIds[0]],
      });
      if (result.ok && result.value.installedApps.length > 0) {
        toast.resolve(
          tid,
          "success",
          `已将 Skill 写入 ${project.projectName} / ${group.combinedLabel}`
        );
      } else {
        toast.resolve(
          tid,
          "error",
          `写入失败：${!result.ok ? result.error : group.combinedLabel}`
        );
        void syncProjectDiskStatuses([project]);
      }
    } else {
      // Remove across all aliases in the group (e.g. .agents/skills + .agent/skills)
      const result = await skillUninstallFromProject({
        skillId: skill.id,
        projectPath: project.projectPath,
        apps: groupAppIds,
      });
      if (result.ok) {
        toast.resolve(
          tid,
          "success",
          `已从 ${project.projectName} / ${group.combinedLabel} 移除 Skill`
        );
      } else {
        toast.resolve(tid, "error", `移除失败：${result.error}`);
        void syncProjectDiskStatuses([project]);
      }
    }
  };

  const removeProjectEnable = async (projectIdx: number) => {
    const project = projectEnables[projectIdx];
    if (!project) return;

    const enabledApps = Object.entries(project.apps)
      .filter(([, enabled]) => enabled)
      .map(([id]) => id);

    if (enabledApps.length > 0) {
      const tid = toast.loading(
        `正在移除 ${project.projectName} 中的此 Skill 软链接…`
      );
      const result = await skillUninstallFromProject({
        skillId: skill.id,
        projectPath: project.projectPath,
        apps: enabledApps,
      });

      if (result.ok) {
        toast.resolve(
          tid,
          "success",
          `已从 ${project.projectName} 移除此 Skill`
        );
      } else {
        toast.resolve(tid, "error", `移除失败：${result.error}`);
      }
    }
    setProjectEnables((prev) => prev.filter((_, i) => i !== projectIdx));
  };

  const addProjectEnable = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择项目文件夹",
      });
      if (selected) {
        const path = selected as string;
        if (projectEnables.some((p) => p.projectPath === path)) {
          toast.info("该项目已在列表中");
          return;
        }
        const folderName = path.split(/[/\\]/).pop() || "未命名项目";
        const check = await skillCheckSymlinkStatus({
          skillId: skill.id,
          scope: "project",
          projectPath: path,
        });
        const initialApps: Record<string, boolean> = Object.fromEntries(
          allApps.map((a) => [a.id, false])
        );
        if (check.ok) {
          for (const st of check.value.statuses) {
            initialApps[st.appId] = st.exists;
          }
        }
        setProjectEnables((prev) => [
          ...prev,
          {
            projectId: `project-${Date.now()}`,
            projectName: folderName,
            projectPath: path,
            apps: initialApps,
          },
        ]);
        toast.success(`已添加项目「${folderName}」`);
      }
    } catch (error) {
      console.error("Failed to open directory dialog:", error);
    }
  };

  return (
    <div className={s.detail}>
      {/* Header */}
      <div className={s.detailHeader}>
        <div className={s.detailTop}>
          <div
            className={s.detailIcon}
            style={{ background: iconColors.bg, color: iconColors.fg }}
          >
            <span>{initial}</span>
          </div>
          <div className={s.detailMeta}>
            <div className={s.detailNameRow}>
              <h2 className={s.detailName}>{skill.name}</h2>
            </div>
            <p className={s.detailDesc}>{skill.description || "无描述"}</p>
            <div className={s.detailMetaRow}>
              <span>
                <GitBranch size={12} /> {skill.slug}
              </span>
              <span>
                <Clock size={12} /> {formatDate(skill.updatedAt)}
              </span>
            </div>
          </div>
          <div className={s.detailActions}>
            <IconButton
              icon={<ExternalLink size={16} />}
              variant="default"
              title="在 Finder 中显示"
              onClick={async () => {
                const result = await skillShowInFinder(skill.id);
                if (!result.ok) {
                  toast.error("无法打开 Finder");
                }
              }}
              aria-label="在 Finder 中显示"
            />
            <IconButton
              icon={<FileUp size={16} />}
              variant="default"
              title="导出 ZIP"
              onClick={onExport}
              aria-label="导出 ZIP"
            />
            <IconButton
              icon={<Trash2 size={16} />}
              variant="danger"
              title="卸载"
              onClick={onDelete}
              aria-label={`卸载 ${skill.name}`}
            />
          </div>
        </div>
        {/* Tabs */}
        <div className={s.tabs}>
          {(["enable", "skillmd", "files"] as Tab[]).map((t) => (
            <button
              key={t}
              className={`${s.tab} ${tab === t ? s.tabActive : ""}`}
              onClick={() => handleTabChange(t)}
            >
              {
                {
                  enable: "启用状态",
                  skillmd: "SKILL.md",
                  files: "目录",
                }[t]
              }
            </button>
          ))}
        </div>
      </div>

      {/* Tab content */}
      <div className={s.detailBody}>
        {/* ── 启用状态 ── */}
        {tab === "enable" && (
          <div className={s.tabContent}>
            {/* 全局级别 */}
            <div className={s.enableSection}>
              <div className={s.enableSectionLabel}>
                <Globe size={14} /> 全局级别{" "}
                <span className={s.enableSectionHint}>
                  (所有项目生效 · 各客户端完全独立)
                </span>
              </div>
              <div className={s.globalApps}>
                {visibleSkillApps.map((a) => (
                  <label key={a.id} className={s.globalAppItem}>
                    <input
                      type="checkbox"
                      checked={enabledGlobalSet.has(a.id)}
                      onChange={() => void toggleSkillGlobalApp(skill.id, a.id)}
                      style={{ accentColor: a.accentColor }}
                    />
                    <AppIcon app={a} />
                    <div>
                      <div className={s.globalAppName}>{a.label}</div>
                      <div className={s.globalAppPath}>{a.skillPathLabel}</div>
                    </div>
                  </label>
                ))}
              </div>
            </div>

            {/* 项目级别 */}
            <div className={s.enableSection}>
              <div className={s.enableSectionLabel}>
                <Folder size={14} /> 项目级别{" "}
                <span className={s.enableSectionHint}>
                  (仅特定项目生效 · 同目录客户端自动合并防误删)
                </span>
                <button className={s.addProjectBtn} onClick={addProjectEnable}>
                  <Plus size={14} /> 添加项目
                </button>
              </div>
              <div className={s.projectList}>
                {projectEnables.map((proj, idx) => (
                  <div key={proj.projectId} className={s.projectItem}>
                    <div className={s.projectHeader}>
                      <div className={s.projectNameWrap}>
                        <span className={s.projectName}>
                          {proj.projectName}
                        </span>
                        <span className={s.projectPath}>
                          {proj.projectPath}
                        </span>
                      </div>
                      <IconButton
                        icon={<X size={14} />}
                        variant="danger"
                        size="sm"
                        className={s.removeProjectBtn}
                        onClick={() => void removeProjectEnable(idx)}
                        aria-label={`移除项目 ${proj.projectName}`}
                        title="移除项目"
                      />
                    </div>
                    <div className={s.projectApps}>
                      {projectSkillGroups.map((group) => {
                        const checked = group.apps.some(
                          (a) => !!proj.apps[a.id]
                        );
                        return (
                          <label
                            key={group.groupKey}
                            className={s.projectAppItem}
                            title={
                              group.isShared
                                ? `${group.combinedLabel} 在项目级共用 ${group.relPathLabel} 目录，联动开关以防误删`
                                : `${group.combinedLabel} (${group.relPathLabel})`
                            }
                          >
                            <input
                              type="checkbox"
                              checked={checked}
                              onChange={() =>
                                void toggleProjectGroup(idx, group)
                              }
                              style={{ accentColor: group.accentColor }}
                            />
                            <div className={s.projectAppMeta}>
                              <span>
                                {group.combinedLabel}
                                {group.isShared && (
                                  <span className={s.sharedPathBadge}>
                                    共用 {group.relPathLabel}
                                  </span>
                                )}
                              </span>
                              {!group.isShared && (
                                <span className={s.projectAppSub}>
                                  {group.relPathLabel}
                                </span>
                              )}
                            </div>
                          </label>
                        );
                      })}
                    </div>
                  </div>
                ))}
                {projectEnables.length === 0 && (
                  <div className={s.noProjects}>
                    暂无项目级别配置，点击上方按钮添加
                  </div>
                )}
              </div>
            </div>
          </div>
        )}

        {/* ── SKILL.md ── */}
        {tab === "skillmd" && (
          <div className={s.skillmdWrap}>
            <div className={s.skillmdHeader}>
              <div className={s.skillmdMeta}>
                <span className={s.skillmdFilename}>SKILL.md</span>
              </div>
              <div className={s.skillmdActions}>
                <button
                  className={`${s.skillmdActionBtn} ${s.skillmdActionPrimary}`}
                  onClick={() => void openWithTypora(skill.id)}
                >
                  用Typora打开
                </button>
              </div>
            </div>
            {skillmdLoading ? (
              <div className={s.skillmdLoading}>
                <Loader size={16} className={s.btnSpin} />
                <span>加载中...</span>
              </div>
            ) : skillmdError ? (
              <div className={s.skillmdError}>
                <AlertTriangle size={16} />
                <span>{skillmdError}</span>
              </div>
            ) : (
              <pre className={s.skillmdCode}>{skillmdContent}</pre>
            )}
          </div>
        )}

        {/* ── 目录浏览 ── */}
        {tab === "files" &&
          (previewingFile ? (
            <FilePreview
              skill={skill}
              filePath={previewingFile}
              onBack={() => setPreviewingFile(null)}
            />
          ) : (
            <DirectoryBrowser
              skill={skill}
              onOpenFile={(path) => setPreviewingFile(path)}
            />
          ))}
      </div>
    </div>
  );
}

// ── Loading Skeleton ────────────────────────────────────────────────────────
function LoadingSkeleton() {
  return (
    <div className={s.list}>
      {[1, 2, 3, 4].map((i) => (
        <div key={i} className={s.card}>
          <div className={s.cardContent}>
            <div className={s.skeletonIcon} />
            <div className={s.skeletonBody}>
              <div className={s.skeletonTitle} />
              <div className={s.skeletonDesc} />
              <div className={s.skeletonFooter} />
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Main InstalledPage ───────────────────────────────────────────────────────
export function MyLibraryPage({
  onNavigate,
  activeLibraryTab,
  externalAppFilter,
}: {
  onNavigate: (page: import("../App").PageId) => void;
  activeLibraryTab: LibraryGroupTab;
  externalAppFilter: string | null;
}) {
  const {
    skills,
    externalSkills,
    globalEnabledBySkill,
    toggleSkillGlobalApp,
    autoEnableDefaultApps,
    loading,
    error,
    search,
    remove,
    refresh,
    syncFromSource,
  } = useSkills();
  const { visibleSkillApps, getAppMeta } = useApps();
  const toast = useToast();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedExternalKey, setSelectedExternalKey] = useState<string | null>(
    null
  );
  const [searchQuery, setSearchQuery] = useState("");
  const [importing, setImporting] = useState(false);
  const [externalImportPreview, setExternalImportPreview] =
    useState<ExternalImportPreviewState | null>(null);
  const [loadingExternalPreview, setLoadingExternalPreview] = useState(false);
  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const searchQueryRef = useRef(searchQuery);
  const sourceSyncInFlightRef = useRef(false);

  useEffect(() => {
    searchQueryRef.current = searchQuery;
  }, [searchQuery]);

  // Debounced search (300ms)
  useEffect(() => {
    if (searchTimeoutRef.current) {
      clearTimeout(searchTimeoutRef.current);
    }
    searchTimeoutRef.current = setTimeout(() => {
      void search(searchQuery);
    }, 300);
    return () => {
      if (searchTimeoutRef.current) {
        clearTimeout(searchTimeoutRef.current);
      }
    };
  }, [searchQuery, search]);

  const selectedSkill =
    activeLibraryTab === "external"
      ? null
      : (skills.find((skill) => skill.id === selectedId) ?? null);
  const selectedSkillId = selectedSkill?.id ?? null;

  const selectedExternal =
    activeLibraryTab === "external" && selectedExternalKey
      ? (externalSkills.find(
          (sk) => `${sk.appId}:${sk.slug}` === selectedExternalKey
        ) ?? null)
      : null;

  const handleDelete = useCallback(async () => {
    if (selectedSkill) {
      const tid = toast.loading(`正在卸载「${selectedSkill.name}」…`);
      const success = await remove(selectedSkill.id);
      if (success) {
        toast.resolve(tid, "success", `「${selectedSkill.name}」已卸载`);
        setSelectedId(skills[0]?.id ?? null);
      } else {
        toast.resolve(tid, "error", "卸载失败");
      }
    }
  }, [selectedSkill, remove, skills, toast]);

  // Export handler
  const handleExport = useCallback(async () => {
    if (!selectedSkill) return;
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择导出目录",
      });
      if (!selected) return;

      const exportPath = `${selected}/${selectedSkill.slug}.zip`;
      const tid = toast.loading("正在导出 ZIP...");

      const result = await skillExportToZip(selectedSkill.id, exportPath);
      if (result.ok) {
        toast.resolve(
          tid,
          "success",
          `「${selectedSkill.name}」已导出到 ${result.value}`
        );
      } else {
        toast.resolve(tid, "error", result.error);
      }
    } catch (e) {
      toast.error(`导出失败：${e}`);
    }
  }, [selectedSkill, toast]);

  const handleImport = useCallback(async () => {
    if (importing) return;

    setImporting(true);
    const tid = toast.loading("请选择 Skill 文件夹或 ZIP 包...");

    try {
      const result = await skillImportFromDialog();

      if (!result.ok) {
        toast.resolve(tid, "error", result.error);
        return;
      }

      if (!result.value) {
        toast.dismiss(tid);
        return;
      }

      await refresh();
      const enabledIds = await autoEnableDefaultApps(result.value.id);
      const suffix =
        enabledIds.length > 0
          ? `（已自动启用到 ${enabledIds.map((id) => getAppMeta(id)?.label ?? id).join(", ")}）`
          : "";
      toast.resolve(
        tid,
        "success",
        `「${result.value.name}」导入成功${suffix}`
      );
      setSelectedId(result.value.id);
    } catch (error) {
      toast.resolve(tid, "error", `导入失败：${String(error)}`);
    } finally {
      setImporting(false);
    }
  }, [autoEnableDefaultApps, getAppMeta, importing, refresh, toast]);

  // Import external skill from app directory
  const handleImportExternal = useCallback(
    async (skill: ExternalSkill) => {
      setImporting(true);
      const tid = toast.loading(`正在导入「${skill.name}」...`);

      const result = await skillImportFromFolder(skill.path);
      setImporting(false);

      if (result.ok) {
        await refresh();
        await autoEnableDefaultApps(result.value.id);
        toast.resolve(
          tid,
          "success",
          `「${result.value.name}」已导入 SkillSwitch`
        );
        setSelectedId(result.value.id);
      } else {
        toast.resolve(tid, "error", result.error);
      }
    },
    [autoEnableDefaultApps, refresh, toast]
  );

  const handlePreviewExternalImport = useCallback(
    async (skill: ExternalSkill) => {
      setLoadingExternalPreview(true);
      try {
        const entries = await readDir(skill.path);
        const previewEntries = entries
          .filter((entry) => !!entry.name && !entry.name.startsWith("."))
          .map((entry) => ({
            name: entry.name,
            kind: entry.isDirectory
              ? ("directory" as const)
              : ("file" as const),
            isSymlink: entry.isSymlink,
          }))
          .sort((left, right) => {
            if (left.kind !== right.kind) {
              return left.kind === "directory" ? -1 : 1;
            }
            return left.name.localeCompare(right.name, "zh-CN");
          });

        setExternalImportPreview({
          skill,
          entries: previewEntries,
          duplicateSkill:
            skills.find((managedSkill) => managedSkill.slug === skill.slug) ??
            null,
        });
      } catch (error) {
        toast.error(`读取导入预览失败：${String(error)}`);
      } finally {
        setLoadingExternalPreview(false);
      }
    },
    [skills, toast]
  );

  const filteredSkills = searchQuery.trim()
    ? skills.filter(
        (s) =>
          s.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
          (s.description?.toLowerCase().includes(searchQuery.toLowerCase()) ??
            false)
      )
    : skills;

  const filteredSelfCreated = filteredSkills;

  const appFilteredExternal = externalAppFilter
    ? externalSkills.filter((skill) => skill.appId === externalAppFilter)
    : externalSkills;
  const filteredExternal = searchQuery.trim()
    ? appFilteredExternal.filter((skill) => {
        const query = searchQuery.toLowerCase();
        const appLabel = getAppMeta(skill.appId)?.label.toLowerCase() ?? "";
        return (
          skill.name.toLowerCase().includes(query) ||
          skill.slug.toLowerCase().includes(query) ||
          (skill.description?.toLowerCase().includes(query) ?? false) ||
          appLabel.includes(query)
        );
      })
    : appFilteredExternal;

  useEffect(() => {
    const activeManagedSkills =
      activeLibraryTab === "self-created" ? skills : [];

    if (activeLibraryTab === "external") return;

    if (activeManagedSkills.length === 0) {
      if (selectedId !== null) {
        setSelectedId(null);
      }
      return;
    }

    const selectedStillVisible = activeManagedSkills.some(
      (skill) => skill.id === selectedId
    );
    if (!selectedStillVisible) {
      setSelectedId(activeManagedSkills[0].id);
    }
  }, [activeLibraryTab, selectedId, skills]);

  useEffect(() => {
    if (activeLibraryTab !== "external" || !externalAppFilter) {
      if (selectedExternalKey !== null) {
        setSelectedExternalKey(null);
      }
      return;
    }

    if (filteredExternal.length === 0) {
      if (selectedExternalKey !== null) {
        setSelectedExternalKey(null);
      }
      return;
    }

    const selectedStillVisible = filteredExternal.some(
      (skill) => `${skill.appId}:${skill.slug}` === selectedExternalKey
    );

    if (!selectedStillVisible) {
      setSelectedExternalKey(
        `${filteredExternal[0].appId}:${filteredExternal[0].slug}`
      );
    }
  }, [
    activeLibraryTab,
    externalAppFilter,
    filteredExternal,
    selectedExternalKey,
  ]);

  useEffect(() => {
    if (activeLibraryTab !== "self-created" || !selectedSkillId) {
      return;
    }

    let disposed = false;
    let unwatch: (() => void) | undefined;

    const syncSelectedSkill = async () => {
      if (sourceSyncInFlightRef.current) {
        return;
      }

      sourceSyncInFlightRef.current = true;
      const result = await syncFromSource(selectedSkillId);
      sourceSyncInFlightRef.current = false;

      if (disposed) {
        return;
      }

      if (!result.ok) {
        if (!result.error.includes("SKILL.md 文件不存在")) {
          toast.error(`同步 SKILL.md 失败：${result.error}`);
        }
        return;
      }

      const currentQuery = searchQueryRef.current.trim();
      if (currentQuery) {
        void search(currentQuery);
      }
    };

    void (async () => {
      const sourceDirResult = await skillGetSourceDirPath(selectedSkillId);
      if (disposed) return;

      if (!sourceDirResult.ok) {
        console.warn(
          "Failed to resolve skill source directory:",
          sourceDirResult.error
        );
        return;
      }

      try {
        unwatch = await watch(
          sourceDirResult.value,
          (event) => {
            if (!event.paths.some((path) => SKILL_MD_PATH_RE.test(path))) {
              return;
            }
            void syncSelectedSkill();
          },
          { recursive: false, delayMs: 400 }
        );
      } catch (error) {
        console.error("Failed to watch skill source directory:", error);
      }
    })();

    return () => {
      disposed = true;
      sourceSyncInFlightRef.current = false;
      unwatch?.();
    };
  }, [activeLibraryTab, search, selectedSkillId, syncFromSource, toast]);

  const handleSelectManagedSkill = useCallback(
    (skillId: string) => {
      if (skillId === selectedId) return;
      setSelectedId(skillId);
    },
    [selectedId]
  );

  const renderActiveTabContent = () => {
    if (activeLibraryTab === "self-created") {
      if (filteredSelfCreated.length === 0) {
        return (
          <div className={s.empty}>
            {searchQuery
              ? "自建 Skills 中未找到匹配项"
              : "还没有自建 Skill，点击「创建」试试"}
          </div>
        );
      }

      return filteredSelfCreated.map((skill) => (
        <SkillCard
          key={skill.id}
          skill={skill}
          selected={selectedId === skill.id}
          enabledAppIds={globalEnabledBySkill[skill.id] ?? []}
          visibleApps={visibleSkillApps}
          onToggleApp={(appId) => void toggleSkillGlobalApp(skill.id, appId)}
          onClick={() => handleSelectManagedSkill(skill.id)}
        />
      ));
    }

    if (!externalAppFilter) {
      return (
        <div className={s.empty}>
          从左侧二级列表选择一个来源后，这里会显示对应的外部 Skills。
        </div>
      );
    }

    if (filteredExternal.length === 0) {
      return (
        <div className={s.empty}>
          {searchQuery
            ? "外部 Skills 中未找到匹配项"
            : "还没有发现可导入的外部 Skills"}
        </div>
      );
    }

    return filteredExternal.map((skill) => {
      const key = `${skill.appId}:${skill.slug}`;
      return (
        <ExternalSkillCard
          key={key}
          skill={skill}
          selected={selectedExternalKey === key}
          onClick={() => setSelectedExternalKey(key)}
          onImport={() => void handlePreviewExternalImport(skill)}
        />
      );
    });
  };

  return (
    <div className={s.page}>
      {/* ── Header ── */}
      <header className={s.header}>
        <div className={s.headerLeft}>
          <h1 className={s.headerTitle}>
            {activeLibraryTab === "self-created"
              ? "自建 Skills"
              : "外部 Skills"}
          </h1>
          <span className={s.headerSub}>
            {activeLibraryTab === "self-created" &&
              `${filteredSelfCreated.length} 个`}
            {activeLibraryTab === "external" && `${filteredExternal.length} 个`}
          </span>
        </div>
        <div className={s.headerRight}>
          <div className={s.searchWrap}>
            <Search size={14} className={s.searchIcon} />
            <input
              className={s.search}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="搜索 Skills..."
            />
          </div>
          <button
            className={s.importBtn}
            onClick={() => void handleImport()}
            disabled={importing}
          >
            {importing ? (
              <>
                <Loader size={12} className={s.btnSpin} /> 导入中
              </>
            ) : (
              "导入"
            )}
          </button>
          <button className={s.createBtn} onClick={() => onNavigate("create")}>
            <Plus size={14} /> 创建 Skill
          </button>
        </div>
      </header>

      {/* ── Error Banner ── */}
      {error && (
        <div className={s.errorBanner}>
          <span>
            <AlertTriangle size={14} /> {error}
          </span>
          <button onClick={() => window.location.reload()}>重试</button>
        </div>
      )}

      {/* ── Body: list + detail ── */}
      <div className={s.body}>
        {/* Left: card list */}
        {loading ? (
          <LoadingSkeleton />
        ) : (
          <div className={s.list}>
            <div className={s.groupTabPanel}>{renderActiveTabContent()}</div>
          </div>
        )}

        {/* Right: detail panel */}
        {selectedSkill && (
          <DetailPanel
            key={selectedSkill.id}
            skill={selectedSkill}
            onDelete={handleDelete}
            onExport={handleExport}
            onSyncFromSource={syncFromSource}
          />
        )}
        {!selectedSkill &&
          activeLibraryTab === "external" &&
          selectedExternal && (
            <ExternalDetailPanel
              key={`${selectedExternal.appId}:${selectedExternal.slug}`}
              skill={selectedExternal}
              onImport={() =>
                void handlePreviewExternalImport(selectedExternal)
              }
            />
          )}
        {!selectedSkill &&
          activeLibraryTab === "external" &&
          !selectedExternal && (
            <div className={s.detailPlaceholder}>
              <div className={s.detailPlaceholderIcon}>
                <ExternalLink size={18} />
              </div>
              <div className={s.detailPlaceholderTitle}>外部 Skills</div>
              <div className={s.detailPlaceholderHint}>
                从左侧二级列表选择一个来源，再查看并导入该目录下发现的外部
                Skill。
              </div>
            </div>
          )}
      </div>
      {externalImportPreview && (
        <ExternalImportPreviewModal
          preview={externalImportPreview}
          importing={importing}
          loading={loadingExternalPreview}
          onClose={() => setExternalImportPreview(null)}
          onConfirm={async () => {
            await handleImportExternal(externalImportPreview.skill);
            setExternalImportPreview(null);
          }}
        />
      )}
    </div>
  );
}

function ExternalImportPreviewModal({
  preview,
  importing,
  loading,
  onClose,
  onConfirm,
}: {
  preview: ExternalImportPreviewState;
  importing: boolean;
  loading: boolean;
  onClose: () => void;
  onConfirm: () => void | Promise<void>;
}) {
  const { getAppMeta } = useApps();
  const appMeta = getAppMeta(preview.skill.appId);

  return (
    <div className={modalStyles.modalOverlay} onClick={onClose}>
      <div
        className={`${modalStyles.modal} ${s.externalPreviewModal}`}
        onClick={(event) => event.stopPropagation()}
      >
        <div className={modalStyles.modalHeader}>
          <span className={modalStyles.modalTitle}>导入前预览</span>
          <button className={modalStyles.modalClose} onClick={onClose}>
            <X size={14} />
          </button>
        </div>
        <div className={modalStyles.modalBody}>
          <div className={s.externalPreviewMeta}>
            <div className={s.externalPreviewRow}>
              <span className={s.externalPreviewLabel}>Skill</span>
              <span className={s.externalPreviewValue}>
                {preview.skill.name} · {preview.skill.slug}
              </span>
            </div>
            {appMeta && (
              <div className={s.externalPreviewRow}>
                <span className={s.externalPreviewLabel}>来源</span>
                <span className={s.externalPreviewValue}>{appMeta.label}</span>
              </div>
            )}
            <div className={s.externalPreviewRow}>
              <span className={s.externalPreviewLabel}>路径</span>
              <span
                className={`${s.externalPreviewValue} ${s.externalPreviewPath}`}
              >
                {preview.skill.path}
              </span>
            </div>
            <div className={s.externalPreviewRow}>
              <span className={s.externalPreviewLabel}>符号链接</span>
              <span className={s.externalPreviewValue}>
                {preview.skill.isSymlink
                  ? `是${preview.skill.symlinkTarget ? ` → ${preview.skill.symlinkTarget}` : ""}`
                  : "否"}
              </span>
            </div>
          </div>

          {preview.duplicateSkill && (
            <div className={s.externalPreviewWarning}>
              <AlertTriangle size={14} />
              <span>
                当前库中已存在同 slug 的 Skill：{preview.duplicateSkill.name}
                。继续导入可能失败或需要重命名。
              </span>
            </div>
          )}

          <div className={s.externalPreviewSection}>
            <div className={s.externalPreviewSectionTitle}>
              即将导入的顶层文件
            </div>
            {loading ? (
              <div className={s.externalPreviewEmpty}>正在读取目录...</div>
            ) : preview.entries.length > 0 ? (
              <div className={s.externalPreviewList}>
                {preview.entries.map((entry) => (
                  <div key={entry.name} className={s.externalPreviewEntry}>
                    {entry.kind === "directory" ? (
                      <Folder size={13} />
                    ) : (
                      <File size={13} />
                    )}
                    <span className={s.externalPreviewEntryName}>
                      {entry.name}
                    </span>
                    <span className={s.externalPreviewEntryMeta}>
                      {entry.kind === "directory" ? "目录" : "文件"}
                      {entry.isSymlink ? " · 符号链接" : ""}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <div className={s.externalPreviewEmpty}>
                目录为空或没有可展示的文件。
              </div>
            )}
          </div>

          <div className={s.externalPreviewActions}>
            <button
              className={s.previewCancelBtn}
              onClick={onClose}
              disabled={importing}
            >
              取消
            </button>
            <button
              className={s.previewConfirmBtn}
              onClick={() => void onConfirm()}
              disabled={importing || loading}
            >
              {importing ? (
                <>
                  <Loader size={14} className={s.btnSpin} /> 导入中...
                </>
              ) : (
                <>
                  <Download size={14} /> 确认导入
                </>
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
