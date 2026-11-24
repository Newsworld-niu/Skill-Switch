import { useEffect, useState } from "react";
import { AlertTriangle, Check, Loader, RefreshCw } from "lucide-react";
import type { BackupSyncState } from "../../types";
import {
  backupSyncRetry,
  backupSyncStateGet,
  onBackupSyncState,
} from "../../services/backupSource";
import s from "./BackupSyncIndicator.module.css";

function formatTime(timestamp?: number | null): string {
  if (!timestamp) return "";
  return new Date(timestamp).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

interface Props {
  /** Fallback from persisted settings, used before the worker has run this session. */
  persistedLastSyncedAt?: number | null;
  persistedLastError?: string | null;
}

/** Compact backup push status shown at the bottom of the sidebar. */
export function BackupSyncIndicator({
  persistedLastSyncedAt,
  persistedLastError,
}: Props) {
  const [state, setState] = useState<BackupSyncState | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void backupSyncStateGet().then((result) => {
      if (!disposed && result.ok) setState(result.value);
    });
    void onBackupSyncState((next) => setState(next)).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const status =
    state && state.status !== "idle"
      ? state.status
      : persistedLastError
        ? "failed"
        : "synced";
  const lastError = state?.lastError ?? persistedLastError;
  const lastSyncedAt = state?.lastSyncedAt ?? persistedLastSyncedAt;

  if (status === "syncing") {
    return (
      <div className={s.row}>
        <Loader size={11} className={s.spin} />
        <span className={s.label}>正在推送到备份源…</span>
      </div>
    );
  }

  if (status === "failed") {
    return (
      <div className={`${s.row} ${s.failed}`} title={lastError ?? undefined}>
        <AlertTriangle size={11} />
        <span className={s.label}>备份推送失败</span>
        <button
          type="button"
          className={s.retry}
          onClick={() => void backupSyncRetry()}
          aria-label="重试推送到备份源"
        >
          <RefreshCw size={11} /> 重试
        </button>
      </div>
    );
  }

  const time = formatTime(lastSyncedAt);
  return (
    <div className={s.row}>
      <Check size={11} />
      <span className={s.label}>{time ? `已同步 · ${time}` : "已同步"}</span>
    </div>
  );
}
