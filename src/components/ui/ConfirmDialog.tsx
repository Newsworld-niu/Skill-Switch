import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import s from "./ConfirmDialog.module.css";

export interface ConfirmOptions {
  title: string;
  message?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Destructive action: red confirm button, initial focus on Cancel. */
  danger?: boolean;
}

type ConfirmFn = (options: ConfirmOptions) => Promise<boolean>;

const ConfirmContext = createContext<ConfirmFn | null>(null);

interface PendingConfirm extends ConfirmOptions {
  resolve: (value: boolean) => void;
}

/** Provides an in-app replacement for `window.confirm` via `useConfirm()`. */
export function ConfirmProvider({ children }: { children: ReactNode }) {
  const [pending, setPending] = useState<PendingConfirm | null>(null);

  const confirm = useCallback<ConfirmFn>(
    (options) =>
      new Promise<boolean>((resolve) => setPending({ ...options, resolve })),
    []
  );

  const close = useCallback(
    (value: boolean) => {
      pending?.resolve(value);
      setPending(null);
    },
    [pending]
  );

  return (
    <ConfirmContext.Provider value={confirm}>
      {children}
      {pending && <ConfirmDialog options={pending} onClose={close} />}
    </ConfirmContext.Provider>
  );
}

export function useConfirm(): ConfirmFn {
  const ctx = useContext(ConfirmContext);
  if (!ctx) throw new Error("useConfirm must be used within ConfirmProvider");
  return ctx;
}

function ConfirmDialog({
  options,
  onClose,
}: {
  options: ConfirmOptions;
  onClose: (value: boolean) => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    (options.danger ? cancelRef : confirmRef).current?.focus();
  }, [options.danger]);

  return (
    <div
      className={s.overlay}
      data-modal-overlay
      onClick={() => onClose(false)}
    >
      <div
        className={s.dialog}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 id="confirm-dialog-title" className={s.title}>
          {options.title}
        </h2>
        {options.message && <div className={s.message}>{options.message}</div>}
        <div className={s.actions}>
          <button
            ref={cancelRef}
            type="button"
            className={s.cancel}
            onClick={() => onClose(false)}
          >
            {options.cancelLabel ?? "取消"}
          </button>
          <button
            ref={confirmRef}
            type="button"
            className={`${s.confirm} ${options.danger ? s.danger : ""}`}
            onClick={() => onClose(true)}
          >
            {options.confirmLabel ?? "确定"}
          </button>
        </div>
      </div>
    </div>
  );
}
