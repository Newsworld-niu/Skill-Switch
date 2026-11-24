import { type CSSProperties } from "react";
import { Globe, ExternalLink, Download } from "lucide-react";
import { type TargetAppMeta } from "../../context/AppContext";
import { showInFinder } from "../../services/skill";
import type { Skill, ExternalSkill } from "../../types";
import s from "../MyLibraryPage.module.css";
import { getIconColors, getProvenanceBadge } from "../../utils/display";
import { getCompactAppName } from "./shared";

// ── Skill Card (left list) ───────────────────────────────────────────────────
export function SkillCard({
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
      data-list-id={skill.id}
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
export function ExternalSkillCard({
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
      className={`${s.card} ${s.cardExternal} ${selected ? s.cardSelected : ""} ${onClick ? s.cardClickable : ""}`}
      data-list-id={`${skill.appId}:${skill.slug}`}
      onClick={onClick}
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
              className={`${s.cardBadgeExternal} ${s.cardBadgeLink}`}
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
