import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { LibraryFolder, LibraryFolderKind } from "../types";
import { libraryFoldersList, libraryFoldersSet } from "../services/folders";
import { useToast } from "../components/ui/Toast";

interface FolderContextValue {
  folders: LibraryFolder[];
  refresh: () => Promise<void>;
  foldersOf: (kind: LibraryFolderKind) => LibraryFolder[];
  createFolder: (
    kind: LibraryFolderKind,
    name: string,
    parentId: string | null
  ) => Promise<void>;
  renameFolder: (
    kind: LibraryFolderKind,
    id: string,
    name: string
  ) => Promise<void>;
  /** Deletes a folder; its subfolders and items move up to its parent. */
  deleteFolder: (kind: LibraryFolderKind, id: string) => Promise<void>;
  /** Moves a folder under `parentId` (null = top level); ignores cycles. */
  moveFolder: (
    kind: LibraryFolderKind,
    id: string,
    parentId: string | null
  ) => Promise<void>;
  /** Moves an item into `folderId`, or out of every folder when null. */
  moveItem: (
    kind: LibraryFolderKind,
    itemId: string,
    folderId: string | null
  ) => Promise<void>;
}

function ofKind(list: LibraryFolder[], kind: LibraryFolderKind) {
  return list.filter((f) => f.kind === kind);
}

/** Whether `id` is `ancestorId` or nested anywhere below it. */
export function isWithin(
  list: LibraryFolder[],
  id: string | null,
  ancestorId: string
): boolean {
  const seen = new Set<string>();
  let current = id;
  while (current && !seen.has(current)) {
    if (current === ancestorId) return true;
    seen.add(current);
    current = list.find((f) => f.id === current)?.parentId ?? null;
  }
  return false;
}

const FolderContext = createContext<FolderContextValue | null>(null);

export function FolderProvider({ children }: { children: ReactNode }) {
  const toast = useToast();
  const [folders, setFolders] = useState<LibraryFolder[]>([]);
  // Latest folders for the async mutators; updated synchronously on every change.
  const foldersRef = useRef(folders);
  const apply = useCallback((next: LibraryFolder[]) => {
    foldersRef.current = next;
    setFolders(next);
  }, []);

  const refresh = useCallback(async () => {
    const result = await libraryFoldersList();
    if (result.ok) apply(result.value);
  }, [apply]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const foldersOf = useCallback(
    (kind: LibraryFolderKind) => ofKind(folders, kind),
    [folders]
  );

  const save = useCallback(
    async (kind: LibraryFolderKind, next: LibraryFolder[]) => {
      const previous = foldersRef.current;
      // Optimistic: replace this kind in place, keep the others untouched.
      apply([...previous.filter((f) => f.kind !== kind), ...next]);
      const result = await libraryFoldersSet(kind, next);
      if (result.ok) {
        apply(result.value);
      } else {
        apply(previous);
        toast.error(`保存文件夹失败：${result.error}`);
      }
    },
    [apply, toast]
  );

  const createFolder = useCallback(
    async (kind: LibraryFolderKind, name: string, parentId: string | null) => {
      const trimmed = name.trim();
      if (!trimmed) return;
      await save(kind, [
        ...ofKind(foldersRef.current, kind),
        { id: crypto.randomUUID(), name: trimmed, kind, parentId, itemIds: [] },
      ]);
    },
    [save]
  );

  const renameFolder = useCallback(
    async (kind: LibraryFolderKind, id: string, name: string) => {
      const trimmed = name.trim();
      if (!trimmed) return;
      await save(
        kind,
        ofKind(foldersRef.current, kind).map((f) =>
          f.id === id ? { ...f, name: trimmed } : f
        )
      );
    },
    [save]
  );

  const deleteFolder = useCallback(
    async (kind: LibraryFolderKind, id: string) => {
      const list = ofKind(foldersRef.current, kind);
      const target = list.find((f) => f.id === id);
      if (!target) return;
      const parentId = target.parentId ?? null;
      await save(
        kind,
        list
          .filter((f) => f.id !== id)
          .map((f) => {
            let next = f.parentId === id ? { ...f, parentId } : f;
            if (f.id === parentId) {
              next = { ...next, itemIds: [...next.itemIds, ...target.itemIds] };
            }
            return next;
          })
      );
    },
    [save]
  );

  const moveFolder = useCallback(
    async (kind: LibraryFolderKind, id: string, parentId: string | null) => {
      const list = ofKind(foldersRef.current, kind);
      const target = list.find((f) => f.id === id);
      if (!target || (target.parentId ?? null) === parentId) return;
      if (parentId && isWithin(list, parentId, id)) return;
      await save(
        kind,
        list.map((f) => (f.id === id ? { ...f, parentId } : f))
      );
    },
    [save]
  );

  const moveItem = useCallback(
    async (
      kind: LibraryFolderKind,
      itemId: string,
      folderId: string | null
    ) => {
      await save(
        kind,
        ofKind(foldersRef.current, kind).map((f) => {
          const without = f.itemIds.filter((id) => id !== itemId);
          return {
            ...f,
            itemIds: f.id === folderId ? [...without, itemId] : without,
          };
        })
      );
    },
    [save]
  );

  const value = useMemo(
    () => ({
      folders,
      refresh,
      foldersOf,
      createFolder,
      renameFolder,
      deleteFolder,
      moveFolder,
      moveItem,
    }),
    [
      folders,
      refresh,
      foldersOf,
      createFolder,
      renameFolder,
      deleteFolder,
      moveFolder,
      moveItem,
    ]
  );

  return (
    <FolderContext.Provider value={value}>{children}</FolderContext.Provider>
  );
}

export function useFolders(): FolderContextValue {
  const ctx = useContext(FolderContext);
  if (!ctx) throw new Error("useFolders must be used within FolderProvider");
  return ctx;
}
