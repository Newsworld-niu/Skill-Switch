import { type TargetAppMeta } from "../../context/AppContext";
import type { Skill, ExternalSkill } from "../../types";
import s from "../MyLibraryPage.module.css";

export function AppIcon({ app }: { app: TargetAppMeta }) {
  if (app.iconSrc) {
    return <img src={app.iconSrc} alt="" className={s.appMiniIcon} />;
  }
  return (
    <span className={s.appMiniMonogram} style={{ background: app.accentColor }}>
      {app.shortLabel}
    </span>
  );
}

export interface ExternalImportPreviewState {
  skill: ExternalSkill;
  entries: Array<{
    name: string;
    kind: "file" | "directory";
    isSymlink: boolean;
  }>;
  duplicateSkill: Skill | null;
}

/** Detail panel tab for a managed skill. */
export type Tab = "enable" | "skillmd" | "files";
