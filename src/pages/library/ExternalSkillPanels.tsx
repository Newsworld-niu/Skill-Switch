import { useState, useEffect } from "react";
import {
  X,
  Folder,
  AlertTriangle,
  Loader,
  File,
  FileText,
  ExternalLink,
  Download,
} from "lucide-react";
import { useApps } from "../../context/AppContext";
import { showInFinder, readExternalSkillContent } from "../../services/skill";
import { IconButton } from "../../components/ui/IconButton";
import type { ExternalSkill } from "../../types";
import modalStyles from "../../components/layout/AppShell.module.css";
import s from "../MyLibraryPage.module.css";
import { getIconColors } from "../../utils/display";
import { AppIcon, type ExternalImportPreviewState } from "./shared";

// ── External Detail Panel (read-only) ────────────────────────────────────────
export function ExternalDetailPanel({
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
              className={`${s.externalDetailValue} ${s.externalDetailPath}`}
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

export function ExternalImportPreviewModal({
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
    <div
      className={modalStyles.modalOverlay}
      data-modal-overlay
      onClick={onClose}
    >
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
