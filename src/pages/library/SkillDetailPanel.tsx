import { useState, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  X,
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
  Check,
} from "lucide-react";
import { useSkills } from "../../context/SkillContext";
import {
  useApps,
  type ProjectSkillTargetGroup,
} from "../../context/AppContext";
import { useToast } from "../../components/ui/Toast";
import {
  skillInstallToProject,
  skillUninstallFromProject,
  skillCheckSymlinkStatus,
  skillListDirectory,
  skillReadFile,
  skillShowInFinder,
  skillGetSourceDirPath,
  openSkillInEditor,
  readExternalSkillContent,
} from "../../services/skill";
import { IconButton } from "../../components/ui/IconButton";
import { FolderPicker } from "../../components/ui/FolderedGrid";
import type {
  Skill,
  SkillDirectoryListing,
  SkillDirectoryEntry,
  SkillFileContent,
} from "../../types";
import s from "../MyLibraryPage.module.css";
import { getIconColors, formatDate } from "../../utils/display";
import { AppIcon, type Tab } from "./shared";

// ── Project enable state interface
interface ProjectEnableState {
  projectId: string;
  projectName: string;
  projectPath: string;
  apps: Record<string, boolean>;
}

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
          <span className={s.filePreviewError}>{error}</span>
        ) : (
          content?.content || ""
        )}
      </div>
    </div>
  );
}

export function DetailPanel({
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
      toast.error(`无法打开目录选择框：${String(error)}`);
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
            <FolderPicker kind="skill" itemId={skill.id} />
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
                <Globe size={14} /> 全局启用
                <span className={s.enableSectionHint}>对所有项目生效</span>
              </div>
              <div className={s.globalApps}>
                {visibleSkillApps.map((a) => {
                  const on = enabledGlobalSet.has(a.id);
                  return (
                    <label
                      key={a.id}
                      className={`${s.globalAppItem} ${on ? s.globalAppItemOn : ""}`}
                    >
                      <input
                        type="checkbox"
                        className={s.visuallyHidden}
                        checked={on}
                        onChange={() =>
                          void toggleSkillGlobalApp(skill.id, a.id)
                        }
                      />
                      <AppIcon app={a} />
                      <span className={s.globalAppName}>{a.label}</span>
                      <span className={s.globalAppCheck}>
                        {on && <Check size={12} strokeWidth={3} />}
                      </span>
                    </label>
                  );
                })}
              </div>
            </div>

            {/* 项目级别 */}
            <div className={s.enableSection}>
              <div className={s.enableSectionLabel}>
                <Folder size={14} /> 项目启用
                <span className={s.enableSectionHint}>仅对指定项目生效</span>
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
                                    共用目录
                                  </span>
                                )}
                              </span>
                            </div>
                          </label>
                        );
                      })}
                    </div>
                  </div>
                ))}
                {projectEnables.length === 0 && (
                  <div className={s.noProjects}>
                    还没有添加项目。点击「添加项目」选择一个项目目录。
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
                  onClick={async () => {
                    const result = await openSkillInEditor(skill.id);
                    if (!result.ok) toast.error(result.error);
                  }}
                >
                  在编辑器中打开
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
