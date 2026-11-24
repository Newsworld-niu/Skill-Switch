import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { ChevronRight, Folder, FolderPlus, Pencil, Trash2 } from "lucide-react";
import { isWithin, useFolders } from "../../context/FolderContext";
import { useConfirm } from "./ConfirmDialog";
import { getIconColors } from "../../utils/display";
import type { LibraryFolder, LibraryFolderKind } from "../../types";
import s from "./FolderedGrid.module.css";

type DragPayload = { type: "item" | "folder"; id: string; label: string };

/** Drop key for the top level (folder ids are UUIDs, never this). */
const ROOT = "__root__";
/** Pointer travel before a press becomes a drag. */
const DRAG_THRESHOLD = 5;
/** Hovering a folder this long while dragging opens it (Finder spring-loading). */
const SPRING_OPEN_MS = 700;

function NameInput({
  initial,
  placeholder,
  onSubmit,
  onCancel,
}: {
  initial: string;
  placeholder: string;
  onSubmit: (name: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initial);
  return (
    <input
      className={s.nameInput}
      autoFocus
      value={value}
      placeholder={placeholder}
      onChange={(e) => setValue(e.target.value)}
      onClick={(e) => e.stopPropagation()}
      onFocus={(e) => e.target.select()}
      onBlur={() => (value.trim() ? onSubmit(value) : onCancel())}
      onKeyDown={(e) => {
        if (e.key === "Enter") onSubmit(value);
        if (e.key === "Escape") {
          // Keep the global Esc handler from closing anything else.
          e.preventDefault();
          e.stopPropagation();
          onCancel();
        }
      }}
    />
  );
}

function FolderGlyph({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 64 50"
      className={`${s.glyph} ${className ?? ""}`}
      aria-hidden="true"
    >
      <path
        className={s.glyphBack}
        d="M4 8a4 4 0 0 1 4-4h15.3a4 4 0 0 1 2.8 1.2L30 9h26a4 4 0 0 1 4 4v29a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4z"
      />
      <path
        className={s.glyphFront}
        d="M4 17a4 4 0 0 1 4-4h48a4 4 0 0 1 4 4v25a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4z"
      />
    </svg>
  );
}

/** Folder's parent, treating a dangling parent id as top level. */
function parentOf(folders: LibraryFolder[], folder: LibraryFolder) {
  const p = folder.parentId ?? null;
  return p && folders.some((f) => f.id === p) ? p : null;
}

/** Moved imperatively so a drag doesn't re-render the grid on every frame. */
function positionPreview(
  node: HTMLElement | null,
  { x, y }: { x: number; y: number }
) {
  if (node) node.style.transform = `translate(${x + 14}px, ${y + 10}px)`;
}

/** Swallows the click that follows a drag's pointerup. */
function suppressNextClick() {
  const stop = (e: MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
  };
  window.addEventListener("click", stop, { capture: true, once: true });
  setTimeout(() => window.removeEventListener("click", stop, true), 0);
}

/**
 * Finder-style browser over the user's display folders: folders show as
 * icons, click to open, drag cards or folders onto a folder (or a breadcrumb)
 * to move them. Folders are organisational only.
 */
export function FolderedGrid<T>({
  kind,
  items,
  getId,
  getLabel,
  renderItem,
  searching,
  empty,
}: {
  kind: LibraryFolderKind;
  items: T[];
  getId: (item: T) => string;
  /** Name shown on the drag preview. */
  getLabel: (item: T) => string;
  renderItem: (item: T) => ReactNode;
  /** While searching, every match is shown flat, ignoring folders. */
  searching: boolean;
  empty: ReactNode;
}) {
  const {
    foldersOf,
    createFolder,
    renameFolder,
    deleteFolder,
    moveFolder,
    moveItem,
  } = useFolders();
  const confirm = useConfirm();
  const folders = foldersOf(kind);
  const [openId, setOpenId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [dragging, setDragging] = useState<DragPayload | null>(null);
  const [overKey, setOverKey] = useState<string | null>(null);
  const previewRef = useRef<HTMLDivElement>(null);
  const pointerRef = useRef({ x: 0, y: 0 });

  // A folder deleted elsewhere drops us back to the top level.
  const currentId = folders.some((f) => f.id === openId) ? openId : null;

  const folderOfItem = new Map<string, string>();
  for (const f of folders) {
    for (const id of f.itemIds) {
      if (!folderOfItem.has(id)) folderOfItem.set(id, f.id);
    }
  }
  const locationOf = (itemId: string) => folderOfItem.get(itemId) ?? null;

  const childrenOf = (id: string | null) =>
    folders.filter((f) => parentOf(folders, f) === id);

  const itemIdSet = new Set(items.map(getId));
  const countIn = (id: string): number => {
    const own = folders
      .find((f) => f.id === id)
      ?.itemIds.filter((i) => itemIdSet.has(i) && locationOf(i) === id).length;
    return (own ?? 0) + childrenOf(id).reduce((n, c) => n + countIn(c.id), 0);
  };

  const path: LibraryFolder[] = [];
  let cursor = folders.find((x) => x.id === currentId);
  while (cursor && !path.includes(cursor)) {
    path.unshift(cursor);
    const parentId = parentOf(folders, cursor);
    cursor = folders.find((x) => x.id === parentId);
  }

  const canDrop = (d: DragPayload, target: string | null) => {
    if (d.type === "item") return locationOf(d.id) !== target;
    const folder = folders.find((f) => f.id === d.id);
    if (!folder || parentOf(folders, folder) === target) return false;
    return !(target && isWithin(folders, target, d.id));
  };

  const drop = (d: DragPayload, target: string | null) => {
    if (d.type === "item") void moveItem(kind, d.id, target);
    else void moveFolder(kind, d.id, target);
  };

  // Window listeners outlive a render; they read the latest logic from here.
  const latest = useRef({ canDrop, drop });
  latest.current = { canDrop, drop };

  // Spring-loading: hovering a folder tile opens it.
  useEffect(() => {
    if (!dragging || !overKey || overKey === ROOT) return;
    if (!folders.some((f) => f.id === overKey) || overKey === currentId) {
      return;
    }
    const timer = setTimeout(() => setOpenId(overKey), SPRING_OPEN_MS);
    return () => clearTimeout(timer);
  }, [dragging, overKey, folders, currentId]);

  const startPress = (
    e: ReactPointerEvent<HTMLElement>,
    payload: DragPayload
  ) => {
    if (e.button !== 0 || searching) return;
    if ((e.target as Element).closest("input, [data-no-drag]")) return;
    const startX = e.clientX;
    const startY = e.clientY;
    let active = false;
    let over: string | null = null;

    const targetAt = (x: number, y: number) => {
      const el = document
        .elementFromPoint(x, y)
        ?.closest<HTMLElement>("[data-drop-key]");
      const key = el?.dataset.dropKey ?? null;
      if (key === null) return null;
      return latest.current.canDrop(payload, key === ROOT ? null : key)
        ? key
        : null;
    };

    const place = (x: number, y: number) => {
      pointerRef.current = { x, y };
      positionPreview(previewRef.current, pointerRef.current);
    };

    const onMove = (ev: PointerEvent) => {
      if (!active) {
        if (
          Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD
        ) {
          return;
        }
        active = true;
        document.documentElement.classList.add(s.grabbing);
        window.getSelection()?.removeAllRanges();
        setDragging(payload);
      }
      place(ev.clientX, ev.clientY);
      const next = targetAt(ev.clientX, ev.clientY);
      if (next !== over) {
        over = next;
        setOverKey(next);
      }
    };

    const finish = (commit: boolean) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
      window.removeEventListener("keydown", onKey, true);
      if (!active) return;
      document.documentElement.classList.remove(s.grabbing);
      suppressNextClick();
      if (commit && over !== null) {
        latest.current.drop(payload, over === ROOT ? null : over);
      }
      setDragging(null);
      setOverKey(null);
    };
    const onUp = () => finish(true);
    const onCancel = () => finish(false);
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape" || !active) return;
      ev.preventDefault();
      ev.stopPropagation();
      finish(false);
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    window.addEventListener("keydown", onKey, true);
  };

  // The preview mounts one render after the drag starts; place it before paint.
  useLayoutEffect(() => {
    if (dragging) positionPreview(previewRef.current, pointerRef.current);
  }, [dragging]);

  const handleDelete = async (folder: LibraryFolder) => {
    const count = countIn(folder.id);
    const subfolders = childrenOf(folder.id).length;
    const ok = await confirm({
      title: `删除文件夹「${folder.name}」？`,
      message:
        count > 0 || subfolders > 0
          ? "里面的条目和子文件夹会移到上一级，内容本身不受影响。"
          : "这是一个空文件夹。",
      confirmLabel: "删除文件夹",
      danger: true,
    });
    if (ok) await deleteFolder(kind, folder.id);
  };

  const renderItemCell = (item: T) => {
    const id = getId(item);
    return (
      <div
        key={id}
        className={`${s.itemCell} ${dragging?.id === id ? s.dragSource : ""}`}
        onPointerDown={(e) =>
          startPress(e, { type: "item", id, label: getLabel(item) })
        }
      >
        {renderItem(item)}
      </div>
    );
  };

  if (searching) {
    return items.length > 0 ? (
      <div className={s.grid}>{items.map(renderItemCell)}</div>
    ) : (
      <>{empty}</>
    );
  }

  const subfolders = childrenOf(currentId);
  const here = items.filter((item) => locationOf(getId(item)) === currentId);
  const nothingHere = subfolders.length === 0 && here.length === 0;
  const hereKey = currentId ?? ROOT;
  const overName =
    overKey === ROOT ? "全部" : folders.find((f) => f.id === overKey)?.name;

  return (
    <div
      className={`${s.root} ${overKey === hereKey ? s.rootOver : ""}`}
      data-drop-key={hereKey}
    >
      <div className={s.toolbar}>
        <nav className={s.breadcrumb} aria-label="文件夹路径">
          {path.length > 0 && (
            <>
              <button
                type="button"
                className={`${s.crumb} ${dragging ? s.crumbHint : ""} ${overKey === ROOT ? s.crumbOver : ""}`}
                onClick={() => setOpenId(null)}
                data-drop-key={ROOT}
              >
                全部
              </button>
              {path.map((f, i) => {
                const last = i === path.length - 1;
                return (
                  <span key={f.id} className={s.crumbItem}>
                    <ChevronRight size={13} className={s.crumbSep} />
                    <button
                      type="button"
                      className={`${s.crumb} ${last ? s.crumbCurrent : dragging ? s.crumbHint : ""} ${overKey === f.id && !last ? s.crumbOver : ""}`}
                      onClick={() => setOpenId(f.id)}
                      aria-current={last ? "page" : undefined}
                      data-drop-key={last ? undefined : f.id}
                    >
                      {f.name}
                    </button>
                  </span>
                );
              })}
            </>
          )}
        </nav>
        <button
          type="button"
          className={s.newFolderBtn}
          onClick={() => setCreating(true)}
        >
          <FolderPlus size={14} /> 新建文件夹
        </button>
      </div>

      {(subfolders.length > 0 || creating) && (
        <div className={s.folderGrid}>
          {subfolders.map((folder) => {
            const isOver = overKey === folder.id;
            return (
              <div
                key={folder.id}
                className={`${s.folderTile} ${isOver ? s.folderOver : ""} ${dragging?.id === folder.id ? s.dragSource : ""}`}
                data-drop-key={folder.id}
                onPointerDown={(e) =>
                  startPress(e, {
                    type: "folder",
                    id: folder.id,
                    label: folder.name,
                  })
                }
              >
                <button
                  type="button"
                  className={s.folderOpen}
                  onClick={() => setOpenId(folder.id)}
                  title={folder.name}
                >
                  <FolderGlyph />
                  {renamingId !== folder.id && (
                    <>
                      <span className={s.folderName}>{folder.name}</span>
                      <span className={s.folderCount}>
                        {countIn(folder.id)} 项
                      </span>
                    </>
                  )}
                </button>
                {renamingId === folder.id ? (
                  <NameInput
                    initial={folder.name}
                    placeholder="文件夹名称"
                    onSubmit={(name) => {
                      setRenamingId(null);
                      void renameFolder(kind, folder.id, name);
                    }}
                    onCancel={() => setRenamingId(null)}
                  />
                ) : (
                  !dragging && (
                    <div className={s.folderActions} data-no-drag>
                      <button
                        type="button"
                        className={s.folderAction}
                        title="重命名"
                        aria-label={`重命名文件夹 ${folder.name}`}
                        onClick={() => setRenamingId(folder.id)}
                      >
                        <Pencil size={12} />
                      </button>
                      <button
                        type="button"
                        className={`${s.folderAction} ${s.folderActionDanger}`}
                        title="删除文件夹"
                        aria-label={`删除文件夹 ${folder.name}`}
                        onClick={() => void handleDelete(folder)}
                      >
                        <Trash2 size={12} />
                      </button>
                    </div>
                  )
                )}
              </div>
            );
          })}
          {creating && (
            <div className={s.folderTile}>
              <div className={s.folderOpen}>
                <FolderGlyph />
              </div>
              <NameInput
                initial="新建文件夹"
                placeholder="文件夹名称"
                onSubmit={(name) => {
                  setCreating(false);
                  void createFolder(kind, name, currentId);
                }}
                onCancel={() => setCreating(false)}
              />
            </div>
          )}
        </div>
      )}

      {here.length > 0 && (
        <div className={s.grid}>{here.map(renderItemCell)}</div>
      )}

      {nothingHere &&
        !creating &&
        (currentId ? (
          <div className={s.emptyFolder}>
            空文件夹。拖动卡片到文件夹图标上即可移入；拖住在文件夹上停留片刻会自动打开它。
          </div>
        ) : (
          empty
        ))}

      {dragging &&
        createPortal(
          <div ref={previewRef} className={s.dragPreview} aria-hidden="true">
            <div className={s.dragCard}>
              {dragging.type === "folder" ? (
                <FolderGlyph className={s.dragGlyph} />
              ) : (
                <DragIcon label={dragging.label} />
              )}
              <span className={s.dragLabel}>{dragging.label}</span>
            </div>
            <div
              className={`${s.dragHint} ${overName ? s.dragHintActive : ""}`}
            >
              {overName ? `移到「${overName}」` : "拖到文件夹上"}
            </div>
          </div>,
          document.body
        )}
    </div>
  );
}

function DragIcon({ label }: { label: string }) {
  const colors = getIconColors(label || "?");
  return (
    <span
      className={s.dragIcon}
      style={{ background: colors.bg, color: colors.fg }}
    >
      {(label || "?").charAt(0).toUpperCase()}
    </span>
  );
}

/** Compact "move to folder" select for detail headers. */
export function FolderPicker({
  kind,
  itemId,
}: {
  kind: LibraryFolderKind;
  itemId: string;
}) {
  const { foldersOf, moveItem } = useFolders();
  const folders = foldersOf(kind);
  const current = folders.find((f) => f.itemIds.includes(itemId))?.id ?? "";

  if (folders.length === 0) return null;

  // Depth-first so nested folders read as paths ("设计 / 组件").
  const options: { id: string; label: string }[] = [];
  const walk = (parent: string | null, prefix: string) => {
    for (const f of folders) {
      if (parentOf(folders, f) !== parent) continue;
      const label = prefix ? `${prefix} / ${f.name}` : f.name;
      options.push({ id: f.id, label });
      walk(f.id, label);
    }
  };
  walk(null, "");

  return (
    <label className={s.picker} title="移动到文件夹（仅影响显示）">
      <Folder size={13} />
      <select
        className={s.pickerSelect}
        value={current}
        onChange={(e) => void moveItem(kind, itemId, e.target.value || null)}
      >
        <option value="">不在文件夹中</option>
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}
