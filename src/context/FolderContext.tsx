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
  createFolder: (kind: LibraryFolderKind, name: string) => Promise<void>;
  renameFolder: (
    kind: LibraryFolderKind,
    id: string,
    name: string
  ) => Promise<void>;
  deleteFolder: (kind: LibraryFolderKind, id: string) => Promise<void>;
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
    async (kind: LibraryFolderKind, name: string) => {
      const trimmed = name.trim();
      if (!trimmed) return;
      await save(kind, [
        ...ofKind(foldersRef.current, kind),
        { id: crypto.randomUUID(), name: trimmed, kind, itemIds: [] },
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
      await save(
        kind,
        ofKind(foldersRef.current, kind).filter((f) => f.id !== id)
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
      moveItem,
    }),
    [
      folders,
      refresh,
      foldersOf,
      createFolder,
      renameFolder,
      deleteFolder,
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
