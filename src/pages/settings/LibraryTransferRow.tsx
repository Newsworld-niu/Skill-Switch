import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useSkills } from "../../context/SkillContext";
import { useSlashCommands } from "../../context/SlashCommandContext";
import { useToast } from "../../components/ui/Toast";
import { libraryExportToZip, libraryImportFromZip } from "../../services/skill";
import s from "../SettingsPage.module.css";

/** Export / import the whole library (skills + slash commands) as one ZIP. */
export function LibraryTransferRow() {
  const { refresh: refreshSkills } = useSkills();
  const { refresh: refreshCommands } = useSlashCommands();
  const toast = useToast();
  const [busy, setBusy] = useState<"export" | "import" | null>(null);

  const handleExport = async () => {
    const dir = await open({
      directory: true,
      multiple: false,
      title: "选择导出目录",
    });
    if (!dir) return;
    const date = new Date().toISOString().slice(0, 10);
    setBusy("export");
    const result = await libraryExportToZip(
      `${dir}/skill-switch-library-${date}.zip`
    );
    setBusy(null);
    if (result.ok) {
      toast.success(
        `已导出 ${result.value.skillCount} 个 Skill、${result.value.commandCount} 个 Command 到 ${result.value.path}`
      );
    } else {
      toast.error(`导出失败：${result.error}`);
    }
  };

  const handleImport = async () => {
    const file = await open({
      multiple: false,
      title: "选择库 ZIP",
      filters: [{ name: "ZIP", extensions: ["zip"] }],
    });
    if (!file || Array.isArray(file)) return;
    setBusy("import");
    const result = await libraryImportFromZip(file);
    setBusy(null);
    if (!result.ok) {
      toast.error(`导入失败：${result.error}`);
      return;
    }
    await Promise.all([refreshSkills(), refreshCommands()]);
    const { importedSkills, importedCommands, skipped } = result.value;
    const summary = `已导入 ${importedSkills.length} 个 Skill、${importedCommands.length} 个 Command`;
    if (skipped.length > 0) {
      toast.error(
        `${summary}；跳过 ${skipped.length} 项：${skipped.slice(0, 3).join("；")}${skipped.length > 3 ? "…" : ""}`
      );
    } else {
      toast.success(summary);
    }
  };

  return (
    <div className={s.row}>
      <div className={s.rowBody}>
        <div className={s.rowLabel}>导出 / 导入整个库</div>
        <div className={s.rowHint}>
          打包全部 Skills 与 Commands 为一个
          ZIP，便于分享给团队；导入时同名项会跳过，不会覆盖
        </div>
      </div>
      <button
        type="button"
        className={s.btn}
        onClick={() => void handleExport()}
        disabled={busy !== null}
      >
        {busy === "export" ? "导出中…" : "导出"}
      </button>
      <button
        type="button"
        className={s.btn}
        onClick={() => void handleImport()}
        disabled={busy !== null}
      >
        {busy === "import" ? "导入中…" : "导入"}
      </button>
    </div>
  );
}
