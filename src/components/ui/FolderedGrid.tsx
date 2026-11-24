import { useState, type ReactNode } from "react";
import {
  ChevronRight,
  Folder,
  FolderInput,
  FolderPlus,
  Pencil,
  Trash2,
} from "lucide-react";
import { useFolders } from "../../context/FolderContext";
import { useConfirm } from "./ConfirmDialog";
import type { LibraryFolderKind } from "../../types";
import s from "./FolderedGrid.module.css";

const UNSORTED = "__unsorted__";

function collapsedKey(kind: LibraryFolderKind) {
  return `folder-collapsed-${kind}`;
}

function loadCollapsed(kind: LibraryFolderKind): Set<string> {
  try {
    const raw = localStorage.getItem(collapsedKey(kind));
    return new Set(raw ? (JSON.parse(raw) as string[]) : []);
  } catch {
    return new Set();
  }
}

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

/**
 * Card grid grouped into the user's display folders, with an "未分类" group
 * for everything else. Folders are organisational only.
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
  /** While searching, folders without matches are hidden and all expand. */
  searching: boolean;
  empty: ReactNode;
}) {
  const { foldersOf, createFolder, renameFolder, deleteFolder } = useFolders();
  const confirm = useConfirm();
  const folders = foldersOf(kind);
  const [collapsed, setCollapsed] = useState(() => loadCollapsed(kind));
  const [creating, setCreating] = useState(false);
  const [renamingId, setRenamingId] = useState<string | null>(null);

  const toggle = (id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      try {
        localStorage.setItem(collapsedKey(kind), JSON.stringify([...next]));
      } catch {
        // Collapse state is a convenience only.
      }
      return next;
    });
  };

  const byId = new Map(items.map((item) => [getId(item), item]));
  const placed = new Set<string>();
  const groups = folders.map((folder) => {
    const members = folder.itemIds
      .map((id) => byId.get(id))
      .filter((item): item is T => item !== undefined);
    members.forEach((item) => placed.add(getId(item)));
    return { folder, members };
  });
  const unsorted = items.filter((item) => !placed.has(getId(item)));

  const handleDelete = async (id: string, name: string, count: number) => {
    const ok = await confirm({
      title: `删除文件夹「${name}」？`,
      message:
        count > 0
          ? `其中的 ${count} 项会回到「未分类」，内容本身不受影响。`
          : "这是一个空文件夹。",
      confirmLabel: "删除文件夹",
      danger: true,
    });
    if (ok) await deleteFolder(kind, id);
  };

  const renderGroup = (
    id: string,
    title: ReactNode,
    members: T[],
    actions?: ReactNode
  ) => {
    const isCollapsed = !searching && collapsed.has(id);
    return (
      <section key={id} className={s.group}>
        <div className={s.groupHeader}>
          <button
            type="button"
            className={s.groupToggle}
            aria-expanded={!isCollapsed}
            onClick={() => toggle(id)}
          >
            <ChevronRight
              size={14}
              className={`${s.chevron} ${isCollapsed ? "" : s.chevronOpen}`}
            />
            {id === UNSORTED ? (
              <FolderInput size={14} className={s.groupIcon} />
            ) : (
              <Folder size={14} className={s.groupIcon} />
            )}
            {title}
            <span className={s.groupCount}>{members.length}</span>
          </button>
          {actions && <div className={s.groupActions}>{actions}</div>}
        </div>
        {!isCollapsed &&
          (members.length > 0 ? (
            <div className={s.grid}>{members.map(renderItem)}</div>
          ) : (
            <div className={s.groupEmpty}>
              空文件夹：打开任意条目，在详情顶部的「文件夹」里把它移到这里。
            </div>
          ))}
      </section>
    );
  };

  const visibleGroups = searching
    ? groups.filter((g) => g.members.length > 0)
    : groups;

  return (
    <div className={s.root}>
      <div className={s.toolbar}>
        {creating ? (
          <NameInput
            initial=""
            placeholder="文件夹名称，回车确认"
            onSubmit={(name) => {
              setCreating(false);
              void createFolder(kind, name);
            }}
            onCancel={() => setCreating(false)}
          />
        ) : (
          <button
            type="button"
            className={s.newFolderBtn}
            onClick={() => setCreating(true)}
          >
            <FolderPlus size={14} /> 新建文件夹
          </button>
        )}
      </div>

      {items.length === 0 && folders.length === 0 ? (
        empty
      ) : (
        <>
          {visibleGroups.map(({ folder, members }) =>
            renderGroup(
              folder.id,
              renamingId === folder.id ? (
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
                <span className={s.groupTitle}>{folder.name}</span>
              ),
              members,
              <>
                <button
                  type="button"
                  className={s.groupAction}
                  title="重命名"
                  aria-label={`重命名文件夹 ${folder.name}`}
                  onClick={() => setRenamingId(folder.id)}
                >
                  <Pencil size={13} />
                </button>
                <button
                  type="button"
                  className={`${s.groupAction} ${s.groupActionDanger}`}
                  title="删除文件夹"
                  aria-label={`删除文件夹 ${folder.name}`}
                  onClick={() =>
                    void handleDelete(folder.id, folder.name, members.length)
                  }
                >
                  <Trash2 size={13} />
                </button>
              </>
            )
          )}
          {folders.length === 0 ? (
            items.length > 0 ? (
              <div className={s.grid}>{items.map(renderItem)}</div>
            ) : (
              empty
            )
          ) : (
            unsorted.length > 0 &&
            renderGroup(
              UNSORTED,
              <span className={s.groupTitle}>未分类</span>,
              unsorted
            )
          )}
        </>
      )}
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

  return (
    <label className={s.picker} title="移动到文件夹（仅影响显示）">
      <Folder size={13} />
      <select
        className={s.pickerSelect}
        value={current}
        onChange={(e) => void moveItem(kind, itemId, e.target.value || null)}
      >
        <option value="">未分类</option>
        {folders.map((f) => (
          <option key={f.id} value={f.id}>
            {f.name}
          </option>
        ))}
      </select>
    </label>
  );
}
