import { useRef, useState, type DragEvent, type ReactNode } from "react";
import { ChevronRight, Folder, FolderPlus, Pencil, Trash2 } from "lucide-react";
import { isWithin, useFolders } from "../../context/FolderContext";
import { useConfirm } from "./ConfirmDialog";
import type { LibraryFolder, LibraryFolderKind } from "../../types";
import s from "./FolderedGrid.module.css";

type DragPayload = { type: "item" | "folder"; id: string };

/** Drop-target key for the top level (folder ids are UUIDs, never this). */
const ROOT = "__root__";

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

function FolderGlyph() {
  return (
    <svg viewBox="0 0 64 50" className={s.glyph} aria-hidden="true">
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

/**
 * Finder-style browser over the user's display folders: folders show as
 * icons, click to open, drag cards or folders onto a folder (or a breadcrumb)
 * to move them. Folders are organisational only.
 */
export function FolderedGrid<T>({
  kind,
  items,
  getId,
  renderItem,
  searching,
  empty,
}: {
  kind: LibraryFolderKind;
  items: T[];
  getId: (item: T) => string;
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
  const [overKey, setOverKey] = useState<string | null>(null);
  const drag = useRef<DragPayload | null>(null);

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

  const canDrop = (target: string | null) => {
    const d = drag.current;
    if (!d) return false;
    if (d.type === "item") return locationOf(d.id) !== target;
    const folder = folders.find((f) => f.id === d.id);
    if (!folder || parentOf(folders, folder) === target) return false;
    return !(target && isWithin(folders, target, d.id));
  };

  const dropProps = (target: string | null) => {
    const key = target ?? ROOT;
    return {
      onDragOver: (e: DragEvent) => {
        if (!canDrop(target)) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
        if (overKey !== key) setOverKey(key);
      },
      onDragLeave: (e: DragEvent) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) {
          setOverKey((k) => (k === key ? null : k));
        }
      },
      onDrop: (e: DragEvent) => {
        e.preventDefault();
        setOverKey(null);
        const d = drag.current;
        drag.current = null;
        if (!d || !canDrop(target)) return;
        if (d.type === "item") void moveItem(kind, d.id, target);
        else void moveFolder(kind, d.id, target);
      },
    };
  };

  const dragSource = (payload: DragPayload) => ({
    draggable: true,
    onDragStart: (e: DragEvent) => {
      drag.current = payload;
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", payload.id);
    },
    onDragEnd: () => {
      drag.current = null;
      setOverKey(null);
    },
  });

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
        className={s.itemCell}
        {...(searching ? {} : dragSource({ type: "item", id }))}
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

  return (
    <div className={s.root}>
      <div className={s.toolbar}>
        <nav className={s.breadcrumb} aria-label="文件夹路径">
          {path.length > 0 && (
            <>
              <button
                type="button"
                className={`${s.crumb} ${overKey === ROOT ? s.dropOver : ""}`}
                onClick={() => setOpenId(null)}
                {...dropProps(null)}
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
                      className={`${s.crumb} ${last ? s.crumbCurrent : ""} ${overKey === f.id ? s.dropOver : ""}`}
                      onClick={() => setOpenId(f.id)}
                      aria-current={last ? "page" : undefined}
                      {...(last ? {} : dropProps(f.id))}
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
          {subfolders.map((folder) => (
            <div
              key={folder.id}
              className={`${s.folderTile} ${overKey === folder.id ? s.dropOver : ""}`}
              {...dragSource({ type: "folder", id: folder.id })}
              {...dropProps(folder.id)}
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
                <div className={s.folderActions}>
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
              )}
            </div>
          ))}
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
            空文件夹 —— 回到上一级，把卡片拖到这个文件夹上即可移入。
          </div>
        ) : (
          empty
        ))}
    </div>
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
