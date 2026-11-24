import { type ReactNode } from "react";
import { X } from "lucide-react";
import s from "./Drawer.module.css";

/**
 * Right-side detail drawer. Clicking the backdrop (or Esc, via the global
 * shortcut that clicks the topmost `data-modal-overlay`) closes it.
 * `data-drawer` lets ↑/↓ list navigation keep working while it is open.
 */
export function Drawer({
  onClose,
  label,
  children,
}: {
  onClose: () => void;
  label: string;
  children: ReactNode;
}) {
  return (
    <>
      <div
        className={s.backdrop}
        data-modal-overlay
        data-drawer
        onClick={onClose}
      />
      <aside className={s.panel} role="dialog" aria-label={label}>
        <button
          type="button"
          className={s.close}
          onClick={onClose}
          aria-label="关闭详情"
          title="关闭 (Esc)"
        >
          <X size={16} />
        </button>
        <div className={s.content}>{children}</div>
      </aside>
    </>
  );
}
