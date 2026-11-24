import { useCallback, useEffect, useState } from "react";
import { AppShell } from "./components/layout/AppShell";
import { MyLibraryPage } from "./pages/MyLibraryPage";
import { MySlashCommandsPage } from "./pages/MySlashCommandsPage";
import { McpServersPage } from "./pages/McpServersPage";
import { PluginsPage } from "./pages/PluginsPage";
import { RepoBrowsePage } from "./pages/RepoBrowsePage";
import { CreatePage } from "./pages/CreatePage";
import { SettingsPage } from "./pages/SettingsPage";
import { AppProvider } from "./context/AppContext";
import { SkillProvider } from "./context/SkillContext";
import { SlashCommandProvider } from "./context/SlashCommandContext";
import { McpProvider } from "./context/McpContext";
import { PluginProvider } from "./context/PluginContext";
import { ProjectProvider } from "./context/ProjectContext";
import { SettingsProvider, useSettings } from "./context/SettingsContext";
import { useSkills } from "./context/SkillContext";
import { useSlashCommands } from "./context/SlashCommandContext";
import { useMcp } from "./context/McpContext";
import { usePlugins } from "./context/PluginContext";
import { useProjects } from "./context/ProjectContext";
import { onLibraryChanged } from "./services/backupSource";
import { useGlobalShortcuts } from "./hooks/useGlobalShortcuts";
import { SourceProvider } from "./context/SourceContext";
import { UpdaterProvider } from "./context/UpdaterContext";
import { ToastProvider } from "./components/ui/Toast";
import { ConfirmProvider } from "./components/ui/ConfirmDialog";
import { ToastContainer } from "./components/ui/ToastContainer";
import { UpdateNotification } from "./components/ui/UpdateNotification";
import { BackupSetupModal } from "./components/ui/BackupSetupModal";
import "./App.css";

export type PageId =
  | "my-library"
  | "my-commands"
  | "mcp-servers"
  | "plugins"
  | "repo-browse"
  | "create"
  | "settings";
export type LibraryTab = "self-created" | "external";
export type CommandMode = "self-created" | "external";
export type McpMode = "managed" | "external";

function BackupGate({ children }: { children: React.ReactNode }) {
  const { settings, loading, updateSettings } = useSettings();
  const [ready, setReady] = useState(false);

  if (loading) return null;

  if (!ready && !settings.backupSource && !settings.backupSetupSkipped) {
    return (
      <BackupSetupModal
        onConnected={() => setReady(true)}
        onSkip={() => {
          setReady(true);
          void updateSettings({ backupSetupSkipped: true });
        }}
      />
    );
  }

  return <>{children}</>;
}

/** Reloads library data when the backend reports it changed (e.g. startup pull). */
function LibraryChangeListener() {
  const { refresh: refreshSkills } = useSkills();
  const { refresh: refreshCommands } = useSlashCommands();
  const { refresh: refreshMcp } = useMcp();
  const { refresh: refreshPlugins } = usePlugins();
  const { refresh: refreshProjects } = useProjects();

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void onLibraryChanged(() => {
      void refreshSkills();
      void refreshCommands();
      void refreshMcp();
      void refreshPlugins();
      void refreshProjects();
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [
    refreshSkills,
    refreshCommands,
    refreshMcp,
    refreshPlugins,
    refreshProjects,
  ]);

  return null;
}

export default function App() {
  const [activePage, setActivePage] = useState<PageId>("my-library");
  const [activeRepoId, setActiveRepoId] = useState<string | null>(null);
  const [activeLibraryTab, setActiveLibraryTab] =
    useState<LibraryTab>("self-created");
  const [activeCommandMode, setActiveCommandMode] =
    useState<CommandMode>("self-created");
  const [activeMcpMode, setActiveMcpMode] = useState<McpMode>("managed");
  const [externalAppFilter, setExternalAppFilter] = useState<string | null>(
    null
  );

  useGlobalShortcuts({
    onNew: useCallback(() => setActivePage("create"), []),
    onSettings: useCallback(() => setActivePage("settings"), []),
  });

  const navigateToRepo = (repoId: string) => {
    setActiveRepoId(repoId);
    setActivePage("repo-browse");
  };

  const navigateToLibraryTab = (tab: LibraryTab) => {
    setActiveLibraryTab(tab);
    setActivePage("my-library");
    if (tab !== "external") {
      setExternalAppFilter(null);
    }
  };

  const navigateToExternalApp = (appId: string) => {
    setActiveLibraryTab("external");
    setExternalAppFilter(appId);
    setActivePage("my-library");
  };

  const navigateToMcpMode = (mode: McpMode) => {
    setActiveMcpMode(mode);
    setActivePage("mcp-servers");
  };

  const renderPage = () => {
    switch (activePage) {
      case "my-library":
        return (
          <MyLibraryPage
            onNavigate={setActivePage}
            activeLibraryTab={activeLibraryTab}
            externalAppFilter={externalAppFilter}
          />
        );
      case "my-commands":
        return (
          <MySlashCommandsPage
            activeCommandMode={activeCommandMode}
            onToggleCommandMode={() =>
              setActiveCommandMode((prev) =>
                prev === "self-created" ? "external" : "self-created"
              )
            }
            onNavigateToSelfCreated={() => setActiveCommandMode("self-created")}
          />
        );
      case "mcp-servers":
        return (
          <McpServersPage
            activeMode={activeMcpMode}
            onChangeMode={setActiveMcpMode}
          />
        );
      case "plugins":
        return <PluginsPage onNavigate={setActivePage} />;
      case "repo-browse":
        return <RepoBrowsePage repoId={activeRepoId ?? ""} />;
      case "create":
        return <CreatePage onNavigate={setActivePage} />;
      case "settings":
        return <SettingsPage />;
    }
  };

  return (
    <SettingsProvider>
      <AppProvider>
        <ToastProvider>
          <ConfirmProvider>
            <BackupGate>
              <SourceProvider>
                <SkillProvider>
                  <SlashCommandProvider>
                    <McpProvider>
                      <PluginProvider>
                        <ProjectProvider>
                          <UpdaterProvider>
                            <AppShell
                              activePage={activePage}
                              activeRepoId={activeRepoId}
                              activeLibraryTab={activeLibraryTab}
                              activeCommandMode={activeCommandMode}
                              activeMcpMode={activeMcpMode}
                              externalAppFilter={externalAppFilter}
                              onNavigate={setActivePage}
                              onNavigateRepo={navigateToRepo}
                              onNavigateLibraryTab={navigateToLibraryTab}
                              onNavigateExternalApp={navigateToExternalApp}
                              onNavigateMcpMode={navigateToMcpMode}
                              onToggleCommandMode={() =>
                                setActiveCommandMode((prev) =>
                                  prev === "self-created"
                                    ? "external"
                                    : "self-created"
                                )
                              }
                            >
                              {renderPage()}
                            </AppShell>
                            <LibraryChangeListener />
                            <ToastContainer />
                            <UpdateNotification />
                          </UpdaterProvider>
                        </ProjectProvider>
                      </PluginProvider>
                    </McpProvider>
                  </SlashCommandProvider>
                </SkillProvider>
              </SourceProvider>
            </BackupGate>
          </ConfirmProvider>
        </ToastProvider>
      </AppProvider>
    </SettingsProvider>
  );
}
