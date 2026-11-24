import { Globe, ExternalLink, Download } from "lucide-react";
import { type TargetAppMeta } from "../../context/AppContext";
import { showInFinder } from "../../services/skill";
import type { Skill, ExternalSkill } from "../../types";
import s from "../MyLibraryPage.module.css";
import { getIconColors, getProvenanceBadge } from "../../utils/display";
import { AppIcon } from "./shared";

// ── Skill Card (grid) ────────────────────────────────────────────────────────
export function SkillCard({
  skill,
  selected,
  enabledAppIds,
  visibleApps,
  onClick,
}: {
  skill: Skill;
  selected: boolean;
  enabledAppIds: string[];
  visibleApps: TargetAppMeta[];
  onClick: () => void;
}) {
  const iconColors = getIconColors(skill.name);
  const initial = skill.name.charAt(0).toUpperCase();
  const badge = getProvenanceBadge(skill.provenance);

  return (
    <div
      className={`${s.card} ${selected ? s.cardSelected : ""}`}
      data-list-id={skill.id}
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
            <span className={s.cardName} title={skill.name}>
              {skill.name}
            </span>
          </div>
          <p className={s.cardDesc}>{skill.description || "无描述"}</p>
        </div>
      </div>
      <div className={s.cardFooterRow}>
        {badge ? (
          <span className={s.cardBadge} title={badge}>
            <Globe size={10} /> {badge}
          </span>
        ) : (
          <span className={s.cardSlug}>{skill.slug}</span>
        )}
        <EnabledAppIcons apps={visibleApps} enabledAppIds={enabledAppIds} />
      </div>
    </div>
  );
}

/** Read-only icons of the apps a resource is enabled in globally. */
export function EnabledAppIcons({
  apps,
  enabledAppIds,
  max = 5,
}: {
  apps: TargetAppMeta[];
  enabledAppIds: string[];
  max?: number;
}) {
  const enabled = apps.filter((app) => enabledAppIds.includes(app.id));
  if (enabled.length === 0) {
    return <span className={s.cardAppsNone}>未启用</span>;
  }
  const shown = enabled.slice(0, max);
  const rest = enabled.length - shown.length;
  return (
    <span
      className={s.cardApps}
      title={`已全局启用：${enabled.map((app) => app.label).join("、")}`}
    >
      {shown.map((app) => (
        <span key={app.id} className={s.cardAppIcon}>
          <AppIcon app={app} />
        </span>
      ))}
      {rest > 0 && <span className={s.cardAppMore}>+{rest}</span>}
    </span>
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
