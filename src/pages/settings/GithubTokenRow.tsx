import { useEffect, useState } from "react";
import { githubTokenConfigured, githubTokenSet } from "../../services/settings";
import s from "../SettingsPage.module.css";

interface Props {
  showToast: (message: string) => void;
}

/** Optional GitHub token for marketplace / online-search API calls. */
export function GithubTokenRow({ showToast }: Props) {
  const [configured, setConfigured] = useState(false);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void githubTokenConfigured().then((result) => {
      if (result.ok) setConfigured(result.value);
    });
  }, []);

  const save = async (token: string) => {
    setSaving(true);
    const result = await githubTokenSet(token);
    setSaving(false);
    if (!result.ok) {
      showToast(`保存失败：${result.error}`);
      return;
    }
    setConfigured(!!token.trim());
    setDraft("");
    showToast(token.trim() ? "GitHub Token 已保存" : "GitHub Token 已移除");
  };

  return (
    <div className={s.row}>
      <div className={s.rowBody}>
        <div className={s.rowLabel}>GitHub Token（可选）</div>
        <div className={s.rowHint}>
          {configured
            ? "已配置。仅保存在本机，不会同步到备份仓库"
            : "技能市场与在线搜索会调用 GitHub API，匿名每小时仅 60 次；配置只读 Token 可提高额度"}
        </div>
      </div>
      <input
        type="password"
        className={s.repoInput}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && draft.trim()) void save(draft);
        }}
        placeholder={
          configured ? "输入新 Token 以替换" : "ghp_… / github_pat_…"
        }
        aria-label="GitHub Token"
        autoComplete="off"
        disabled={saving}
      />
      {draft.trim() ? (
        <button
          type="button"
          className={s.btn}
          onClick={() => void save(draft)}
          disabled={saving}
        >
          保存
        </button>
      ) : (
        configured && (
          <button
            type="button"
            className={s.btn}
            onClick={() => void save("")}
            disabled={saving}
          >
            移除
          </button>
        )
      )}
    </div>
  );
}
