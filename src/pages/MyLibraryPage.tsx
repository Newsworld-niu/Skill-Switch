import { useState, useEffect, useRef, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { readDir, watch } from "@tauri-apps/plugin-fs";
import {
  Search,
  Plus,
  AlertTriangle,
  Loader,
  ExternalLink,
} from "lucide-react";
import { useSkills } from "../context/SkillContext";
import { useApps } from "../context/AppContext";
import { useToast } from "../components/ui/Toast";
import {
  skillImportFromDialog,
  skillImportFromFolder,
  skillExportToZip,
  skillGetSourceDirPath,
} from "../services/skill";
import type { ExternalSkill } from "../types";
import s from "./MyLibraryPage.module.css";
import { useConfirm } from "../components/ui/ConfirmDialog";
import { useListKeyboardNav } from "../hooks/useListKeyboardNav";
import { type ExternalImportPreviewState } from "./library/shared";
import { SkillCard, ExternalSkillCard } from "./library/SkillCards";
import {
  ExternalDetailPanel,
  ExternalImportPreviewModal,
} from "./library/ExternalSkillPanels";
import { DetailPanel } from "./library/SkillDetailPanel";

const SKILL_MD_PATH_RE = /(?:^|[\\/])SKILL\.md$/i;

type LibraryGroupTab = "self-created" | "external";

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
  const confirm = useConfirm();
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
      const confirmed = await confirm({
        title: `删除 Skill「${selectedSkill.name}」？`,
        message: "会从库中删除并移除所有已启用的链接，同步到备份仓库。",
        confirmLabel: "删除",
        danger: true,
      });
      if (!confirmed) return;
      const tid = toast.loading(`正在卸载「${selectedSkill.name}」…`);
      const success = await remove(selectedSkill.id);
      if (success) {
        toast.resolve(tid, "success", `「${selectedSkill.name}」已卸载`);
        setSelectedId(skills[0]?.id ?? null);
      } else {
        toast.resolve(tid, "error", "卸载失败");
      }
    }
  }, [selectedSkill, remove, skills, toast, confirm]);

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

      // Live reload of external SKILL.md edits is best-effort; skip silently.
      if (!sourceDirResult.ok) return;

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
      } catch {
        // Watching is best-effort (e.g. path outside the fs scope).
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

  const keyboardNavIds =
    activeLibraryTab === "self-created"
      ? filteredSelfCreated.map((skill) => skill.id)
      : externalAppFilter
        ? filteredExternal.map((skill) => `${skill.appId}:${skill.slug}`)
        : [];
  useListKeyboardNav(
    keyboardNavIds,
    activeLibraryTab === "self-created" ? selectedId : selectedExternalKey,
    activeLibraryTab === "self-created"
      ? handleSelectManagedSkill
      : setSelectedExternalKey
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
              data-search-input
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
