// ─── App-level types ──────────────────────────────────────────────────────────

export type AppTheme = "light" | "dark" | "system";
export type AppLocale = "zh-CN" | "en-US";
export type AppBootStatus =
  | "loading"
  | "ready"
  | "needs-onboarding"
  | "needs-recovery"
  | "error";

export interface AppConfig {
  theme: AppTheme;
  locale: AppLocale;
  version: string;
}

export interface AppBootState {
  status: AppBootStatus;
  message: string | null;
  repoStatus: LibraryRepoStatus | null;
}

// ─── Async state ──────────────────────────────────────────────────────────────

export type AsyncStatus = "idle" | "loading" | "success" | "error";

export interface AsyncState<T> {
  status: AsyncStatus;
  data: T | null;
  error: string | null;
}

// ─── Result type (Rust-style) ─────────────────────────────────────────────────

export type Result<T, E = string> =
  | { ok: true; value: T }
  | { ok: false; error: E };

export function ok<T>(value: T): Result<T> {
  return { ok: true, value };
}

export function err<E = string>(error: E): Result<never, E> {
  return { ok: false, error };
}

// ─── Tauri event payloads ─────────────────────────────────────────────────────

export interface TauriEventPayload<T = unknown> {
  event: string;
  payload: T;
}

// ─── Common UI types ──────────────────────────────────────────────────────────

export interface SelectOption<T = string> {
  label: string;
  value: T;
  disabled?: boolean;
}

export type Size = "sm" | "md" | "lg";
export type Variant = "primary" | "secondary" | "danger" | "ghost";

// ─── Domain types ─────────────────────────────────────────────────────────────

export type ResourceKind =
  | "skill"
  | "slash-command"
  | "prompt"
  | "agents"
  | "mcp-server"
  | "plugin";
export type ResourceScope = "global" | "project";
export type ResourceOrigin = "private" | "vendor" | "forked-vendor";
export type SourceStatus =
  | "current"
  | "upstream-available"
  | "merge-applying"
  | "merge-blocked"
  | "local-only";
export type InstallStatus =
  | "not-installed"
  | "in-sync"
  | "stale"
  | "diverged"
  | "missing";

// ─── Provenance types ─────────────────────────────────────────────────────────

export type ProvenanceKind =
  | "manual"
  | "file-import"
  | "external-app"
  | "marketplace"
  | "repo-source"
  | "plugin";

export interface Provenance {
  kind: ProvenanceKind;
  label: string;
  sourceId?: string | null;
  sourceName?: string | null;
  sourceUrl?: string | null;
  sourcePath?: string | null;
  appId?: string | null;
}

// ─── Scan & preview types ─────────────────────────────────────────────────────

export type ProjectScanItemStatus =
  | "new"
  | "modified"
  | "deleted"
  | "tracked-conflict"
  | "missing"
  | "diverged";
export type PreviewScope =
  | "project-apply"
  | "project-capture"
  | "recovery"
  | "source-update"
  | "install-refresh";
export type PreviewAction = "apply" | "capture" | "ignore" | "refresh";
export type InstallTargetKind =
  | "global-skill"
  | "global-codex-slash-command"
  | "global-prompt"
  | "project-agents"
  | "project-skill"
  | "project-codex-slash-command";

export interface Project {
  id: string;
  name: string;
  path?: string | null;
  color?: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface Skill {
  id: string;
  slug: string;
  name: string;
  description?: string | null;
  content: string;
  tags: string[];
  projectIds: string[];
  createdAt: number;
  updatedAt: number;
  provenance?: Provenance;
}

export interface SlashCommand {
  id: string;
  slug: string;
  name: string;
  description?: string | null;
  content: string;
  tags: string[];
  projectIds: string[];
  createdAt: number;
  updatedAt: number;
  provenance?: Provenance;
}

/// A skill found in an external app directory (not managed by SkillSwitch)
export interface ExternalSkill {
  slug: string;
  name: string;
  description?: string | null;
  appId: string;
  path: string;
  isSymlink: boolean;
  symlinkTarget?: string | null;
}

export interface ExternalSlashCommand {
  slug: string;
  name: string;
  description?: string | null;
  appId: string;
  path: string;
  isSymlink: boolean;
  symlinkTarget?: string | null;
}

export interface Resource {
  id: string;
  slug: string;
  title: string;
  name?: string;
  description?: string | null;
  content: string;
  tags: string[];
  kind: ResourceKind;
  scope: ResourceScope;
  origin: ResourceOrigin;
  projectId?: string | null;
  sourceUrl?: string | null;
  sourceRef?: string | null;
  sourcePath?: string | null;
  revision: string;
  upstreamRevision?: string | null;
  forkedFrom?: string | null;
  sourceStatus: SourceStatus;
  createdAt: number;
  updatedAt: number;
  provenance?: Provenance;
}

export interface ProjectProfile {
  id: string;
  slug: string;
  name: string;
  description?: string | null;
  color?: string | null;
  defaultResourceIds?: string[];
  attachedResourceIds: string[];
  agentsResourceId?: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface LocalProjectBinding {
  projectId: string;
  path: string;
  isActive?: boolean;
  lastUsedAt?: number;
  detectedRepoRoot?: string | null;
  updatedAt?: number;
}

export interface InstallRecord {
  id: string;
  resourceId: string;
  projectId?: string | null;
  targetKind: InstallTargetKind;
  targetPath: string;
  installedRevision: string;
  lastScannedRevision?: string | null;
  status: InstallStatus;
  updatedAt: number;
}

export interface ProjectScanItem {
  id?: string;
  path?: string;
  kind?: ResourceKind;
  status: ProjectScanItemStatus;
  tracked: boolean;
  resourceId?: string | null;
  projectId?: string | null;
  slug?: string | null;
  summary?: string;
  currentRevision?: string | null;
  expectedRevision?: string | null;
  installRecordId?: string | null;
  relativePath?: string;
  absolutePath?: string;
  note?: string | null;
}

export interface ProjectScanResult {
  projectId?: string | null;
  projectPath: string;
  projectName?: string | null;
  gitTrackedConflicts?: number;
  hasAnyChanges?: boolean;
  items: ProjectScanItem[];
  repoRoot?: string | null;
  isGitRepo?: boolean;
  profile?: ProjectProfile | null;
  binding?: LocalProjectBinding | null;
  summary?: {
    totalItems: number;
    trackedConflicts: number;
    divergentItems: number;
  };
}

export interface PreviewPlanItem {
  id?: string;
  path?: string;
  kind?: ResourceKind;
  action?: PreviewAction;
  status: ProjectScanItemStatus | "same";
  summary?: string;
  resourceId?: string | null;
  projectId?: string | null;
  currentRevision?: string | null;
  nextRevision?: string | null;
  relativePath?: string;
  absolutePath?: string;
  note?: string | null;
}

export interface PreviewDecision {
  path: string;
  action: PreviewAction;
}

export interface PreviewPlan {
  scope?: PreviewScope;
  kind?: "apply" | "capture";
  title?: string;
  projectId?: string;
  projectPath?: string;
  summary:
    | string
    | {
        totalItems: number;
        trackedConflicts: number;
        divergentItems: number;
      };
  items: PreviewPlanItem[];
}

export interface UpdateItem {
  id: string;
  resourceId: string;
  resourceName: string;
  projectId?: string | null;
  projectName?: string | null;
  targetPath?: string | null;
  origin: ResourceOrigin;
  sourceStatus: SourceStatus;
  installStatus: InstallStatus;
  currentRevision?: string | null;
  nextRevision?: string | null;
  summary: string;
}

export interface ProjectSessionSummary {
  projectId: string;
  projectName: string;
  projectPath?: string | null;
  installStatus: InstallStatus;
  sourceStatus: SourceStatus;
  driftStatus: "clean" | "changes-detected" | "blocked";
  pendingUpdateCount: number;
  pendingCaptureCount: number;
}

export type StandardSkillDirectory = "scripts" | "references" | "assets";

export interface CreateSkillInput {
  name: string;
  description?: string | null;
  content: string;
  directories: StandardSkillDirectory[];
  tags: string[];
  projectIds: string[];
}

export interface CreateSlashCommandInput {
  name: string;
  description?: string | null;
  content: string;
  directories: StandardSkillDirectory[];
  tags: string[];
  projectIds: string[];
}

export interface BackupSyncResult {
  status: "skipped" | "success" | "failed";
  attempts: number;
  message?: string | null;
  lastError?: string | null;
}

export interface CreateSkillResult {
  skill: Skill;
  backupSync: BackupSyncResult;
}

export interface CreateSlashCommandResult {
  command: SlashCommand;
  backupSync: BackupSyncResult;
}

export interface SyncStatus {
  status: string;
  message?: string | null;
  lastError?: string | null;
  ahead: number;
  behind: number;
}

export interface SkillMutationResult {
  skill: Skill;
  sync: SyncStatus;
}

export interface SkillDeleteResult {
  deleted: boolean;
  sync: SyncStatus;
}

export interface UpdateSkillInput {
  id: string;
  name?: string;
  description?: string | null;
  content?: string;
  tags?: string[];
  projectIds?: string[];
}

export interface UpdateSlashCommandInput {
  id: string;
  name?: string;
  description?: string | null;
  content?: string;
  tags?: string[];
  projectIds?: string[];
}

export interface CreateProjectInput {
  name: string;
  path?: string | null;
  color?: string | null;
}

export interface UpdateProjectInput {
  id: string;
  name?: string;
  path?: string | null;
  color?: string | null;
}

export interface ImportRepoSourceSkillInput {
  repoId: string;
  skillSlug: string;
  skillPath: string;
}

export interface SkillFilter {
  query?: string;
  projectId?: string;
  tags?: string[];
}

export interface RepoPreflightCheck {
  code: string;
  ok: boolean;
  message: string;
}

export interface RepoPreflightResult {
  canProceed?: boolean;
  normalizedPath?: string | null;
  localPath?: string | null;
  remoteUrl?: string | null;
  branch?: string | null;
  pathExists?: boolean;
  pathIsDirectory?: boolean;
  gitAvailable?: boolean;
  isGitRepo?: boolean;
  manifestExists?: boolean;
  legacyStoreExists?: boolean;
  codexHome?: string | null;
  checks?: RepoPreflightCheck[];
}

export interface RepoConnectInput {
  mode?: "existing" | "clone";
  path?: string;
  localPath?: string;
  remoteUrl?: string | null;
  branch?: string | null;
  initializeIfMissing?: boolean;
}

export interface LibraryRepoStatus {
  connected: boolean;
  path?: string | null;
  localPath?: string | null;
  remoteUrl?: string | null;
  branch?: string | null;
  head?: string | null;
  ahead: number;
  behind: number;
  dirty: boolean;
  needsMigration: boolean;
  lastSyncAt?: number | null;
  manifestExists?: boolean;
  gitAvailable?: boolean;
  isGitRepo?: boolean;
  resourcesCount?: number;
  projectProfilesCount?: number;
}

export interface RecoveryEntry {
  id: string;
  kind: ResourceKind | "project";
  name: string;
  path: string;
  status: "repo-only" | "local-only" | "different" | "same";
  repoRevision?: string | null;
  localRevision?: string | null;
}

export interface RecoveryScanResult {
  repoStatus?: LibraryRepoStatus | null;
  summary: string;
  globalItems: RecoveryEntry[];
  projectItems: RecoveryEntry[];
}

// ─── Import / Export types ────────────────────────────────────────────────────

export type ImportStrategy = "skip" | "overwrite" | "rename";

export interface ExportBundle {
  version: string;
  exportedAt: number;
  skills: Skill[];
  projects: Project[];
}

export interface ImportOptions {
  strategy: ImportStrategy;
}

export interface ImportResult {
  skillsImported: number;
  skillsSkipped: number;
  projectsImported: number;
}

// ─── Settings types ────────────────────────────────────────────────────────────

export interface SshTargetConfig {
  host: string;
  user?: string | null;
  port?: number | null;
  identityFile?: string | null;
  remoteAppPreset?: string | null;
  globalPluginDir?: string | null;
  injectSecrets?: boolean;
}

export interface CustomAppConfig {
  id: string;
  label: string;
  accentColor: string;
  globalSkillDir: string;
  projectSkillDir?: string | null;
  globalCommandDir?: string | null;
  projectCommandDir?: string | null;
  globalMcpConfigPath?: string | null;
  projectMcpConfigPath?: string | null;
  mcpFormat?: "json" | "toml" | null;
  mcpRootKey?: string | null;
  ssh?: SshTargetConfig | null;
}

export interface SshAppSyncLedger {
  skills: string[];
  commands: string[];
  mcpServers: string[];
  plugins: string[];
  remoteHome?: string | null;
  lastSyncedAt?: number | null;
}

export interface DiscoveredSshHost {
  alias: string;
  hostname?: string | null;
  user?: string | null;
  port?: number | null;
  identityFile?: string | null;
  sourceFile: string;
}

export interface SshConnectionTestResult {
  ok: boolean;
  remoteHome?: string | null;
  remoteOs?: string | null;
  latencyMs: number;
  message: string;
}

export interface SshSyncSummaryResult {
  appId: string;
  syncedSkills: number;
  syncedCommands: number;
  syncedMcps: number;
  syncedPlugins: number;
  warnings: string[];
}

export interface AppSettings {
  theme: string;
  locale: string;
  autoCheckUpdates: boolean;
  autoCheckAppUpdates: boolean;
  autoStart: boolean;
  backupPath: string | null;
  maxBackups: number;
  backupSource: BackupSource | null;
  thirdPartyRepos: ThirdPartyRepo[];
  customApps: CustomAppConfig[];
  hiddenAppIds: string[];
  defaultInstallAppIds: string[];
}

// ─── Third-party repo source types ────────────────────────────────────────────

export interface BackupSource {
  repo: string;
  label: string;
  remoteUrl: string;
  branch: string;
  localPath?: string | null;
  lastSyncedAt?: number | null;
  lastSyncedCommit?: string | null;
  lastError?: string | null;
}

export interface BackupSourceStatus {
  configured: boolean;
  repo: string;
  label: string;
  remoteUrl: string;
  branch: string;
  localPath: string;
  lastSyncedAt?: number | null;
  lastSyncedCommit?: string | null;
  connected: boolean;
  gitAvailable: boolean;
  isGitRepo: boolean;
  head?: string | null;
  ahead: number;
  behind: number;
  dirty: boolean;
  lastError?: string | null;
  notice?: string | null;
}

export interface BootstrapBackupInput {
  remoteUrl: string;
  branch?: string | null;
}

export interface ThirdPartyRepo {
  id: string;
  url: string; // e.g. https://github.com/anthropics/skills
  label: string; // display name
  enabled: boolean;
  addedAt: number;
  localPath?: string | null;
  lastSyncedAt?: number | null;
}

export interface RemoteSkill {
  id: string; // repoId + path slug
  repoId: string;
  repoLabel: string;
  repoUrl: string;
  name: string;
  description: string;
  content: string;
  tags: string[];
  path: string; // file path in repo
  rawUrl: string;
}

export interface RepoFetchState {
  repoId: string;
  status: "idle" | "loading" | "success" | "error";
  skills: RemoteSkill[];
  error: string | null;
  fetchedAt: number | null;
}

// ─── Global skill install types ────────────────────────────────────────────────

export interface InstallSkillGlobalInput {
  skillId: string;
  apps: string[]; // "claude", "codex", "cursor"
}

export interface InstallSkillGlobalResult {
  installedApps: string[];
  failedApps: string[];
}

export interface InstallSlashCommandGlobalInput {
  commandId: string;
  apps: string[];
}

export interface InstallSlashCommandGlobalResult {
  installedApps: string[];
  failedApps: string[];
}

// ─── Symlink status types ──────────────────────────────────────────────────────

export interface SkillSymlinkStatus {
  appId: string;
  isSymlink: boolean;
  isBroken: boolean;
  targetPath: string | null;
  exists: boolean;
}

export interface CheckSymlinkStatusInput {
  skillId: string;
  scope: "global" | "project";
  projectPath?: string | null;
}

export interface CheckSymlinkStatusResult {
  skillId: string;
  statuses: SkillSymlinkStatus[];
}

export interface CheckSlashCommandSymlinkInput {
  commandId: string;
  scope: "global" | "project";
  projectPath?: string | null;
}

export interface CheckSlashCommandSymlinkResult {
  commandId: string;
  statuses: SkillSymlinkStatus[];
}

export interface RepairBrokenSymlinksResult {
  removedSymlinks: string[];
  errors: string[];
}

// ─── Skill directory browsing types ───────────────────────────────────────────

export type SkillEntryKind = "file" | "directory";

export interface SkillDirectoryEntry {
  name: string;
  kind: SkillEntryKind;
  path: string;
  extension?: string | null;
  size?: number | null;
}

export interface SkillDirectoryListing {
  skillId: string;
  skillSlug: string;
  rootPath: string;
  currentPath: string;
  parentPath?: string | null;
  entries: SkillDirectoryEntry[];
}

export interface SkillFileContent {
  skillId: string;
  path: string;
  content: string;
  size: number;
}

export interface SkillDirectoryInput {
  skillId: string;
  subPath?: string | null;
}

export interface SkillFileInput {
  skillId: string;
  filePath: string;
}

export interface SlashCommandDirectoryListing {
  commandId: string;
  commandSlug: string;
  rootPath: string;
  currentPath: string;
  parentPath?: string | null;
  entries: SkillDirectoryEntry[];
}

export interface SlashCommandFileContent {
  commandId: string;
  path: string;
  content: string;
  size: number;
}

export interface SlashCommandDirectoryInput {
  commandId: string;
  subPath?: string | null;
}

export interface SlashCommandFileInput {
  commandId: string;
  filePath: string;
}

// ─── Marketplace types ────────────────────────────────────────────────────────

/** Source kind for distinguishing different skill sources */
export type SourceKind = "backup" | "repo" | "market" | "registry";

/** A skill item from the marketplace feed (array format from skills-desktop) */
export interface MarketplaceSkillFeedItem {
  id: string;
  name: string;
  author: string;
  authorAvatar?: string;
  description: string;
  githubUrl: string;
  stars: number;
  forks?: number;
  updatedAt: number;
  hasMarketplace?: boolean;
  path: string;
  branch: string;
  descriptionCn?: string;
  deleted?: boolean;
}

/** Paginated marketplace feed response */
export interface MarketplaceFeedPage {
  items: MarketplaceSkillFeedItem[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}

/** Input for loading marketplace feed with pagination and search */
export interface MarketplaceFeedInput {
  page?: number;
  pageSize?: number;
  search?: string;
}

/** Market source configuration */
export interface MarketSource {
  id: string;
  kind: "market";
  label: string;
  enabled: boolean;
}

/** Unified source type for registry */
export type Source =
  | { kind: "backup"; id: string; label: string; repo: ThirdPartyRepo }
  | { kind: "repo"; id: string; label: string; repo: ThirdPartyRepo }
  | { kind: "market"; id: string; label: string }
  | { kind: "registry"; id: string; label: string };

/** Input for importing a skill from marketplace */
export interface ImportMarketSkillInput {
  githubUrl: string;
  branch: string;
  skillPath: string;
  skillName: string;
}

// ─── Skills.sh Registry types ─────────────────────────────────────────────────

/** A skill from the skills.sh registry */
export interface RegistrySkill {
  id: string;
  skillId: string;
  name: string;
  installs: number;
  source: string; // e.g., "anthropics/skills"
}

/** Search result from skills.sh API */
export interface RegistrySearchResult {
  skills: RegistrySkill[];
  count: number;
}

/** Content fetched for a registry skill */
export interface RegistrySkillContent {
  content: string;
  branch: string;
  skillPath: string;
}

/** Input for installing a registry skill */
export interface RegistryInstallInput {
  skillId: string;
  skillName: string;
  content: string;
  source: string;
  apps: string[];
}

/** Result of installing a registry skill */
export interface RegistryInstallResult {
  installedApps: string[];
  failedApps: string[];
}

// ─── MCP Server types ─────────────────────────────────────────────────────────

export type McpTransport = "stdio" | "http" | "sse";
export type ClaudeProjectMcpMode = "local" | "project";

export interface McpEnvVarSpec {
  name: string;
  description?: string | null;
  required: boolean;
  isSecret: boolean;
  defaultValue?: string | null;
}

export interface McpHeaderSpec {
  name: string;
  description?: string | null;
  isSecret: boolean;
  valueTemplate?: string | null;
}

export interface McpServer {
  id: string;
  slug: string;
  name: string;
  description?: string | null;
  transport: McpTransport;
  command?: string | null;
  args: string[];
  url?: string | null;
  envSpecs: McpEnvVarSpec[];
  headerSpecs: McpHeaderSpec[];
  tags: string[];
  projectIds: string[];
  pluginId?: string | null;
  configuredSecretKeys: string[];
  createdAt: number;
  updatedAt: number;
  provenance?: Provenance;
}

export interface CreateMcpServerInput {
  name: string;
  slug?: string | null;
  description?: string | null;
  transport: McpTransport;
  command?: string | null;
  args: string[];
  url?: string | null;
  envSpecs: McpEnvVarSpec[];
  headerSpecs: McpHeaderSpec[];
  secretValues?: Record<string, string>;
  tags: string[];
  projectIds: string[];
}

export interface UpdateMcpServerInput {
  id: string;
  name?: string | null;
  description?: string | null;
  transport?: McpTransport | null;
  command?: string | null;
  args?: string[] | null;
  url?: string | null;
  envSpecs?: McpEnvVarSpec[] | null;
  headerSpecs?: McpHeaderSpec[] | null;
  secretValues?: Record<string, string> | null;
  tags?: string[] | null;
  projectIds?: string[] | null;
}

export interface CreateMcpServerResult {
  server: McpServer;
  backupSync: BackupSyncResult;
}

export interface InstallMcpGlobalInput {
  serverId: string;
  apps: string[];
  useEnvPlaceholders?: boolean;
}

export interface InstallMcpToProjectInput {
  serverId: string;
  projectPath: string;
  apps: string[];
  claudeMode?: ClaudeProjectMcpMode;
}

export interface InstallMcpResult {
  installedApps: string[];
  failedApps: string[];
  warnings: string[];
}

export interface McpAppInstallStatus {
  appId: string;
  supported: boolean;
  installed: boolean;
  configPath?: string | null;
  scopeDetail?: string | null;
  hasDrift: boolean;
  unsupportedReason?: string | null;
}

export interface CheckMcpStatusInput {
  serverId: string;
  scope: "global" | "project";
  projectPath?: string | null;
}

export interface CheckMcpStatusResult {
  serverId: string;
  statuses: McpAppInstallStatus[];
}

export interface ExternalMcpServer {
  slug: string;
  name: string;
  appId: string;
  configPath: string;
  transport: McpTransport;
  command?: string | null;
  args: string[];
  url?: string | null;
  detectedEnvKeys: string[];
  detectedSecretKeys: string[];
  isManaged: boolean;
}

export interface ImportExternalMcpInput {
  appId: string;
  configPath: string;
  slug: string;
  importSecretsToLocal?: boolean;
}

// ─── Plugin / Extension types ─────────────────────────────────────────────────

export type PluginManifestKind =
  | "agent-plugins"
  | "claude-plugin"
  | "codex-plugin"
  | "cursor-plugin"
  | "antigravity-plugin"
  | "gemini-extension"
  | "generic";

export type PluginInstallMode = "native" | "projected";

export interface PluginUserConfigField {
  key: string;
  title: string;
  description?: string | null;
  required: boolean;
  sensitive: boolean;
  defaultValue?: string | null;
}

export interface PluginComponentSummary {
  skills: string[];
  commands: string[];
  mcpServers: string[];
  hooks: string[];
  agents: string[];
  rules: string[];
}

export interface Plugin {
  id: string;
  slug: string;
  name: string;
  version?: string | null;
  author?: string | null;
  description?: string | null;
  homepage?: string | null;
  repositoryUrl?: string | null;
  manifestKind: PluginManifestKind;
  sourceDirPath: string;
  userConfigSchema: PluginUserConfigField[];
  userConfigValues: Record<string, string>;
  configuredSecretKeys: string[];
  components: PluginComponentSummary;
  projectedSkillIds: string[];
  projectedCommandIds: string[];
  projectedMcpServerIds: string[];
  tags: string[];
  createdAt: number;
  updatedAt: number;
  provenance?: Provenance;
}

export interface ImportPluginFromFolderInput {
  folderPath: string;
  projectComponentsToLibrary?: boolean;
}

export interface ImportPluginFromGitInput {
  repoUrl: string;
  subPath?: string | null;
  branch?: string | null;
  projectComponentsToLibrary?: boolean;
}

export interface DiscoveredPluginCandidate {
  subPath: string;
  name: string;
  version?: string | null;
  author?: string | null;
  description?: string | null;
  category?: string | null;
  manifestKind: PluginManifestKind;
  components: PluginComponentSummary;
}

export type ImportPluginOutcome =
  | {
      status: "imported";
      plugins: Plugin[];
      warnings: string[];
      backupSync: BackupSyncResult;
    }
  | {
      status: "selectPlugins";
      stagingId: string;
      sourceLabel: string;
      marketplaceName?: string | null;
      candidates: DiscoveredPluginCandidate[];
    };

export interface ImportStagedPluginsInput {
  stagingId: string;
  subPaths: string[];
  projectComponentsToLibrary?: boolean;
}

export interface ImportPluginBatchResult {
  plugins: Plugin[];
  warnings: string[];
  backupSync: BackupSyncResult;
}

export interface UpdatePluginConfigInput {
  pluginId: string;
  userConfigValues?: Record<string, string> | null;
  secretValues?: Record<string, string> | null;
  tags?: string[] | null;
}

export interface InstallPluginGlobalInput {
  pluginId: string;
  apps: string[];
  preferNative?: boolean;
}

export interface InstallPluginToProjectInput {
  pluginId: string;
  projectPath: string;
  apps: string[];
  claudeMcpMode?: ClaudeProjectMcpMode;
}

export interface InstallPluginResult {
  installedApps: string[];
  failedApps: string[];
  appModes: Record<string, PluginInstallMode>;
  warnings: string[];
}

export interface PluginAppInstallStatus {
  appId: string;
  installed: boolean;
  mode?: PluginInstallMode | null;
  details?: string | null;
}

export interface CheckPluginStatusInput {
  pluginId: string;
  scope: "global" | "project";
  projectPath?: string | null;
}

export interface CheckPluginStatusResult {
  pluginId: string;
  statuses: PluginAppInstallStatus[];
}
