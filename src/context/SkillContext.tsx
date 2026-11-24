import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  type ReactNode,
} from "react";
import type { Skill, ExternalSkill, CreateSkillResult } from "../types";
import {
  skillList,
  skillSearch,
  skillCreate,
  skillUpdate,
  skillDelete,
  scanExternalSkills,
  skillSyncFromSource,
  skillListGlobalEnabledApps,
  skillInstallGlobal,
  skillUninstallGlobal,
} from "../services/skill";
import type { CreateSkillInput, UpdateSkillInput } from "../types";
import { useApps } from "./AppContext";
import { useToast } from "../components/ui/Toast";
import type { Result } from "../services/tauri";

interface SkillContextValue {
  skills: Skill[];
  externalSkills: ExternalSkill[];
  globalEnabledBySkill: Record<string, string[]>;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  refreshGlobalStatuses: () => Promise<void>;
  toggleSkillGlobalApp: (skillId: string, appId: string) => Promise<boolean>;
  autoEnableDefaultApps: (skillId: string) => Promise<string[]>;
  search: (query: string) => Promise<void>;
  create: (input: CreateSkillInput) => Promise<Result<CreateSkillResult>>;
  update: (input: UpdateSkillInput) => Promise<Result<Skill>>;
  syncFromSource: (id: string) => Promise<Result<Skill>>;
  remove: (id: string) => Promise<boolean>;
}

const SkillContext = createContext<SkillContextValue | null>(null);

export function SkillProvider({ children }: { children: ReactNode }) {
  const { allApps, defaultInstallAppIds, getAppMeta } = useApps();
  const toast = useToast();
  const [skills, setSkills] = useState<Skill[]>([]);
  const [externalSkills, setExternalSkills] = useState<ExternalSkill[]>([]);
  const [globalEnabledBySkill, setGlobalEnabledBySkill] = useState<
    Record<string, string[]>
  >({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refreshGlobalStatuses = useCallback(async () => {
    const statusResult = await skillListGlobalEnabledApps();
    if (statusResult.ok) {
      setGlobalEnabledBySkill(statusResult.value);
    }
  }, []);

  // Load skills on mount and when configured apps change
  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);

    const [managedResult, statusResult, externalResults] = await Promise.all([
      skillList(),
      skillListGlobalEnabledApps(),
      Promise.allSettled(allApps.map((app) => scanExternalSkills(app.id))),
    ]);

    if (managedResult.ok) {
      setSkills(managedResult.value);
    } else {
      setError(managedResult.error);
    }

    if (statusResult.ok) {
      setGlobalEnabledBySkill(statusResult.value);
    }

    const unmanagedExternal = externalResults.flatMap((result, index) => {
      const appId = allApps[index]?.id;
      if (!appId || result.status !== "fulfilled" || !result.value.ok) {
        return [];
      }

      return result.value.value.filter((skill) => !skill.isSymlink);
    });
    setExternalSkills(unmanagedExternal);

    setLoading(false);
  }, [allApps]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggleSkillGlobalApp = useCallback(
    async (skillId: string, appId: string): Promise<boolean> => {
      const currentList = globalEnabledBySkill[skillId] ?? [];
      const isEnabled = currentList.includes(appId);
      const nextEnabled = !isEnabled;

      setGlobalEnabledBySkill((prev) => {
        const list = prev[skillId] ?? [];
        const updated = nextEnabled
          ? Array.from(new Set([...list, appId]))
          : list.filter((id) => id !== appId);
        return { ...prev, [skillId]: updated };
      });

      const appLabel = getAppMeta(appId)?.label ?? appId;
      const tid = toast.loading(
        nextEnabled ? `正在启用全局 ${appLabel}…` : `正在关闭全局 ${appLabel}…`
      );

      const result = nextEnabled
        ? await skillInstallGlobal({ skillId, apps: [appId] })
        : await skillUninstallGlobal({ skillId, apps: [appId] });

      const succeeded = result.ok && result.value.installedApps.includes(appId);

      if (succeeded) {
        toast.resolve(
          tid,
          "success",
          nextEnabled
            ? `已在全局 ${appLabel} 启用`
            : `已从全局 ${appLabel} 移除`
        );
        return true;
      }

      const errMsg = !result.ok ? result.error : `操作 ${appLabel} 失败`;
      toast.resolve(tid, "error", errMsg);
      setGlobalEnabledBySkill((prev) => ({
        ...prev,
        [skillId]: currentList,
      }));
      return false;
    },
    [getAppMeta, globalEnabledBySkill, toast]
  );

  const autoEnableDefaultApps = useCallback(
    async (skillId: string): Promise<string[]> => {
      if (defaultInstallAppIds.length === 0) {
        return [];
      }
      const installResult = await skillInstallGlobal({
        skillId,
        apps: defaultInstallAppIds,
      });
      if (!installResult.ok) {
        return [];
      }
      const installed = installResult.value.installedApps;
      if (installed.length > 0) {
        setGlobalEnabledBySkill((prev) => ({
          ...prev,
          [skillId]: Array.from(
            new Set([...(prev[skillId] ?? []), ...installed])
          ),
        }));
      }
      return installed;
    },
    [defaultInstallAppIds]
  );

  // Search skills
  const search = useCallback(
    async (query: string) => {
      if (!query.trim()) {
        await refresh();
        return;
      }
      setLoading(true);
      setError(null);
      const result = await skillSearch(query);
      if (result.ok) {
        setSkills(result.value);
      } else {
        setError(result.error);
      }
      setLoading(false);
    },
    [refresh]
  );

  // Create skill
  const create = useCallback(
    async (input: CreateSkillInput): Promise<Result<CreateSkillResult>> => {
      const result = await skillCreate(input);
      if (result.ok) {
        setSkills((prev) => [...prev, result.value.skill]);
        setGlobalEnabledBySkill((prev) => ({
          ...prev,
          [result.value.skill.id]: prev[result.value.skill.id] ?? [],
        }));
        return result;
      }
      setError(result.error);
      return result;
    },
    []
  );

  // Update skill
  const update = useCallback(
    async (input: UpdateSkillInput): Promise<Result<Skill>> => {
      const result = await skillUpdate(input);
      if (result.ok) {
        setSkills((prev) =>
          prev.map((s) => (s.id === input.id ? result.value : s))
        );
        return result;
      }
      setError(result.error);
      return result;
    },
    []
  );

  const syncFromSource = useCallback(
    async (id: string): Promise<Result<Skill>> => {
      const result = await skillSyncFromSource(id);
      if (result.ok) {
        setSkills((prev) => prev.map((s) => (s.id === id ? result.value : s)));
      }
      return result;
    },
    []
  );

  // Delete skill
  const remove = useCallback(async (id: string): Promise<boolean> => {
    const result = await skillDelete(id);
    if (result.ok) {
      setSkills((prev) => prev.filter((s) => s.id !== id));
      setGlobalEnabledBySkill((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      return true;
    }
    setError(result.error);
    return false;
  }, []);

  return (
    <SkillContext.Provider
      value={{
        skills,
        externalSkills,
        globalEnabledBySkill,
        loading,
        error,
        refresh,
        refreshGlobalStatuses,
        toggleSkillGlobalApp,
        autoEnableDefaultApps,
        search,
        create,
        update,
        syncFromSource,
        remove,
      }}
    >
      {children}
    </SkillContext.Provider>
  );
}

export function useSkills() {
  const context = useContext(SkillContext);
  if (!context) {
    throw new Error("useSkills must be used within SkillProvider");
  }
  return context;
}
