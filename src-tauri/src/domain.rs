use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    Skill,
    SlashCommand,
    Prompt,
    Agents,
    McpServer,
    Plugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceScope {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceOrigin {
    Private,
    Vendor,
    ForkedVendor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceStatus {
    Current,
    UpstreamAvailable,
    MergeApplying,
    MergeBlocked,
    LocalOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallStatus {
    NotInstalled,
    InSync,
    Stale,
    Diverged,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectFileStatus {
    New,
    Modified,
    Deleted,
    TrackedConflict,
    Missing,
    Diverged,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewPlanKind {
    Apply,
    Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewDecisionAction {
    Apply,
    Capture,
    Ignore,
    Refresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallTargetKind {
    GlobalCodexSkill,
    GlobalCodexSlashCommand,
    GlobalCodexPrompt,
    ProjectAgents,
    ProjectCodexSkill,
    ProjectCodexSlashCommand,
}

// ─── Provenance types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProvenanceKind {
    #[default]
    Manual,
    FileImport,
    ExternalApp,
    Marketplace,
    RepoSource,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    pub kind: ProvenanceKind,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
}

// ─── Core resource types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub kind: ResourceKind,
    pub scope: ResourceScope,
    pub origin: ResourceOrigin,
    pub source_status: SourceStatus,
    pub project_id: Option<String>,
    pub tags: Vec<String>,
    pub content: String,
    pub revision: String,
    pub source_url: Option<String>,
    pub source_ref: Option<String>,
    pub source_path: Option<String>,
    pub upstream_revision: Option<String>,
    pub forked_from: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectProfile {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub agents_resource_id: Option<String>,
    pub attached_resource_ids: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalProjectBinding {
    pub project_id: String,
    pub path: String,
    pub detected_repo_root: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallTarget {
    pub kind: InstallTargetKind,
    pub project_id: Option<String>,
    pub path: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    pub id: String,
    pub resource_id: String,
    pub target: InstallTarget,
    pub revision: String,
    pub status: InstallStatus,
    pub last_scanned_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoConfig {
    pub path: String,
    pub connected_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileEntry {
    pub relative_path: String,
    pub absolute_path: String,
    pub status: ProjectFileStatus,
    pub resource_id: Option<String>,
    pub install_record_id: Option<String>,
    pub tracked: bool,
    pub current_revision: Option<String>,
    pub expected_revision: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectScanSummary {
    pub total_items: usize,
    pub tracked_conflicts: usize,
    pub divergent_items: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectScanResult {
    pub project_path: String,
    pub repo_root: Option<String>,
    pub is_git_repo: bool,
    pub profile: Option<ProjectProfile>,
    pub binding: Option<LocalProjectBinding>,
    pub items: Vec<ProjectFileEntry>,
    pub summary: ProjectScanSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPlan {
    pub kind: PreviewPlanKind,
    pub project_id: String,
    pub project_path: String,
    pub items: Vec<ProjectFileEntry>,
    pub summary: ProjectScanSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoPreflightInput {
    pub path: Option<String>,
    pub local_path: Option<String>,
    pub remote_url: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoPreflightResult {
    pub normalized_path: String,
    pub path_exists: bool,
    pub path_is_directory: bool,
    pub git_available: bool,
    pub is_git_repo: bool,
    pub manifest_exists: bool,
    pub legacy_store_exists: bool,
    pub codex_home: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoConnectInput {
    pub path: Option<String>,
    pub local_path: Option<String>,
    pub remote_url: Option<String>,
    pub branch: Option<String>,
    pub initialize_if_missing: Option<bool>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub migrated: bool,
    pub resources_migrated: usize,
    pub project_profiles_migrated: usize,
    pub bindings_migrated: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub connected: bool,
    pub path: Option<String>,
    pub manifest_exists: bool,
    pub git_available: bool,
    pub is_git_repo: bool,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub dirty: bool,
    pub resources_count: usize,
    pub project_profiles_count: usize,
    pub migration: Option<MigrationReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResourceListFilter {
    pub kind: Option<ResourceKind>,
    pub scope: Option<ResourceScope>,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPathInput {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPreviewInput {
    pub project_id: String,
    pub path: String,
    #[serde(default)]
    pub decisions: Vec<PreviewDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDecision {
    pub path: String,
    pub action: PreviewDecisionAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoLibrary {
    pub version: String,
    pub resources: Vec<Resource>,
    pub project_profiles: Vec<ProjectProfile>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerRecord>,
    #[serde(default)]
    pub plugins: Vec<PluginRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalState {
    pub version: String,
    pub repo: Option<RepoConfig>,
    pub project_bindings: Vec<LocalProjectBinding>,
    pub install_records: Vec<InstallRecord>,
    pub last_active_project_id: Option<String>,
    pub recent_project_ids: Vec<String>,
    pub migrated_legacy_store: bool,
    #[serde(default)]
    pub migrated_symlinks: bool, // Track if copied skills have been migrated to symlinks
    #[serde(default)]
    pub ssh_ledgers: std::collections::HashMap<String, SshAppSyncLedger>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacySkillDto {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub tags: Vec<String>,
    pub project_ids: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommandDto {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub tags: Vec<String>,
    pub project_ids: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSyncResult {
    pub status: String,
    pub attempts: usize,
    pub message: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLegacySkillResult {
    pub skill: LegacySkillDto,
    pub backup_sync: BackupSyncResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSlashCommandResult {
    pub command: SlashCommandDto,
    pub backup_sync: BackupSyncResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub status: String,
    pub message: Option<String>,
    pub last_error: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMutationResult {
    pub skill: LegacySkillDto,
    pub sync: SyncStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SkillDeleteResult {
    pub deleted: bool,
    pub sync: SyncStatus,
}

/// A skill found in an external app directory (not managed by SkillSwitch)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSkillDto {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub app_id: String,                 // which app this skill was found in
    pub path: String,                   // absolute path to the skill directory
    pub is_symlink: bool,               // whether it's a symlink (to SkillSwitch's skill-sources)
    pub symlink_target: Option<String>, // if symlink, where it points to
}

/// A slash command found in an external app directory (not managed by SkillSwitch)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSlashCommandDto {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub app_id: String,
    pub path: String,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyProjectDto {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub color: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSkillInput {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub directories: Vec<String>,
    pub tags: Vec<String>,
    pub project_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSlashCommandInput {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub directories: Vec<String>,
    pub tags: Vec<String>,
    pub project_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSkillInput {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub project_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSlashCommandInput {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub project_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub path: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectInput {
    pub id: String,
    pub name: Option<String>,
    pub path: Option<Option<String>>,
    pub color: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRepoSourceSkillInput {
    pub repo_id: String,
    pub skill_slug: String,
    pub skill_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryEntry {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub path: String,
    pub status: String,
    pub repo_revision: Option<String>,
    pub local_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryScanResult {
    pub summary: String,
    pub global_items: Vec<RecoveryEntry>,
    pub project_items: Vec<RecoveryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItem {
    pub id: String,
    pub resource_id: String,
    pub resource_name: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub target_path: Option<String>,
    pub origin: ResourceOrigin,
    pub source_status: SourceStatus,
    pub install_status: InstallStatus,
    pub current_revision: Option<String>,
    pub next_revision: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceIdInput {
    pub resource_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecordIdInput {
    pub record_id: String,
}

// ─── Backup source types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSourceConfig {
    pub repo: String,
    pub label: String,
    pub remote_url: String,
    pub branch: String,
    pub local_path: Option<String>,
    pub last_synced_at: Option<i64>,
    #[serde(default)]
    pub last_synced_commit: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSourceStatus {
    pub configured: bool,
    pub repo: String,
    pub label: String,
    pub remote_url: String,
    pub branch: String,
    pub local_path: String,
    pub last_synced_at: Option<i64>,
    pub last_synced_commit: Option<String>,
    pub connected: bool,
    pub git_available: bool,
    pub is_git_repo: bool,
    pub head: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub dirty: bool,
    pub last_error: Option<String>,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapBackupInput {
    pub remote_url: String,
    pub branch: Option<String>,
}

// ─── Settings & SSH Target types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SshTargetConfig {
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_app_preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_plugin_dir: Option<String>,
    #[serde(default)]
    pub inject_secrets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SshAppSyncLedger {
    pub skills: Vec<String>,
    pub commands: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub plugins: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_home: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct DiscoveredSshHost {
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    pub source_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SshConnectionTestResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_home: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_os: Option<String>,
    pub latency_ms: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SshSyncSummaryResult {
    pub app_id: String,
    pub synced_skills: usize,
    pub synced_commands: usize,
    pub synced_mcps: usize,
    pub synced_plugins: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomAppConfig {
    pub id: String,
    pub label: String,
    pub accent_color: String,
    pub global_skill_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_skill_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_command_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_command_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_mcp_config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_mcp_config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_root_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshTargetConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub theme: String,
    pub locale: String,
    pub auto_check_updates: bool,
    pub auto_check_app_updates: bool,
    pub auto_start: bool,
    pub backup_path: Option<String>,
    pub max_backups: u32,
    pub backup_source: Option<BackupSourceConfig>,
    pub third_party_repos: Vec<ThirdPartyRepo>,
    pub custom_apps: Vec<CustomAppConfig>,
    pub hidden_app_ids: Vec<String>,
    pub default_install_app_ids: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            locale: "zh-CN".to_string(),
            auto_check_updates: true,
            auto_check_app_updates: true,
            auto_start: false,
            backup_path: None,
            max_backups: 10,
            backup_source: None,
            third_party_repos: Vec::new(),
            custom_apps: Vec::new(),
            hidden_app_ids: Vec::new(),
            default_install_app_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ThirdPartyRepo {
    pub id: String,
    pub url: String,
    pub label: String,
    pub enabled: bool,
    pub added_at: i64,
    pub local_path: Option<String>,
    pub last_synced_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RemoteSkill {
    pub id: String,
    pub repo_id: String,
    pub repo_label: String,
    pub repo_url: String,
    pub name: String,
    pub description: String,
    pub content: String,
    pub tags: Vec<String>,
    pub path: String,
    pub raw_url: String,
}

// ─── Project skill install types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSkillToProjectInput {
    pub skill_id: String,
    pub project_path: String,
    pub apps: Vec<String>, // "claude", "codex", "cursor"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSkillToProjectResult {
    pub installed_apps: Vec<String>,
    pub failed_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSlashCommandToProjectInput {
    pub command_id: String,
    pub project_path: String,
    pub apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSlashCommandToProjectResult {
    pub installed_apps: Vec<String>,
    pub failed_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveProjectCliInput {
    pub project_path: String,
    pub apps: Vec<String>, // "claude", "codex", "cursor"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveProjectCliResult {
    pub removed_apps: Vec<String>,
    pub failed_apps: Vec<String>,
}

// ─── Global skill install types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSkillGlobalInput {
    pub skill_id: String,
    pub apps: Vec<String>, // "claude", "codex", "cursor"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSkillGlobalResult {
    pub installed_apps: Vec<String>,
    pub failed_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSlashCommandGlobalInput {
    pub command_id: String,
    pub apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSlashCommandGlobalResult {
    pub installed_apps: Vec<String>,
    pub failed_apps: Vec<String>,
}

// ─── Symlink status types ──────────────────────────────────────────────────────

/// Status of a skill installation symlink for a specific CLI app
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSymlinkStatus {
    pub app_id: String,              // "claude", "codex", "cursor"
    pub is_symlink: bool,            // true if installed as symlink
    pub is_broken: bool,             // true if symlink target doesn't exist
    pub target_path: Option<String>, // the symlink target path, if it's a symlink
    pub exists: bool,                // true if some installation exists (symlink or directory)
}

/// Input for checking symlink status of a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckSymlinkStatusInput {
    pub skill_id: String,
    pub scope: String,                // "global" or "project"
    pub project_path: Option<String>, // required if scope is "project"
}

/// Result of checking symlink status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckSymlinkStatusResult {
    pub skill_id: String,
    pub statuses: Vec<SkillSymlinkStatus>,
}

/// Input for checking symlink status of a slash command
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckSlashCommandSymlinkInput {
    pub command_id: String,
    pub scope: String,
    pub project_path: Option<String>,
}

/// Result of checking symlink status of a slash command
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckSlashCommandSymlinkResult {
    pub command_id: String,
    pub statuses: Vec<SkillSymlinkStatus>,
}

/// Result of repairing broken symlinks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairBrokenSymlinksResult {
    pub removed_symlinks: Vec<String>, // paths of removed broken symlinks
    pub errors: Vec<String>,           // any errors encountered
}

// ─── Skill directory browsing types ───────────────────────────────────────────

/// Entry type in a skill directory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillEntryKind {
    File,
    Directory,
}

/// A single entry in a skill directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDirectoryEntry {
    pub name: String,              // file or directory name
    pub kind: SkillEntryKind,      // file or directory
    pub path: String,              // relative path from skill root
    pub extension: Option<String>, // file extension if it's a file
    pub size: Option<u64>,         // file size in bytes
}

/// Result of listing a skill directory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDirectoryListing {
    pub skill_id: String,
    pub skill_slug: String,
    pub root_path: String,    // absolute path to the skill root directory
    pub current_path: String, // current directory path being listed
    pub parent_path: Option<String>, // parent directory path, if not root
    pub entries: Vec<SkillDirectoryEntry>,
}

/// Result of reading a skill file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileContent {
    pub skill_id: String,
    pub path: String,    // relative path from skill root
    pub content: String, // file content (text only)
    pub size: u64,       // file size in bytes
}

/// Input for listing a skill directory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDirectoryInput {
    pub skill_id: String,
    pub sub_path: Option<String>, // optional subdirectory path, relative to skill root
}

/// Input for reading a skill file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileInput {
    pub skill_id: String,
    pub file_path: String, // relative path from skill root
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommandDirectoryListing {
    pub command_id: String,
    pub command_slug: String,
    pub root_path: String,
    pub current_path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<SkillDirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommandFileContent {
    pub command_id: String,
    pub path: String,
    pub content: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommandDirectoryInput {
    pub command_id: String,
    pub sub_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommandFileInput {
    pub command_id: String,
    pub file_path: String,
}

// ─── Marketplace types ────────────────────────────────────────────────────────

/// A skill item from the marketplace feed (array format from skills-desktop)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceSkillFeedItem {
    pub id: String,
    pub name: String,
    pub author: String,
    #[serde(default)]
    pub author_avatar: Option<String>,
    pub description: String,
    pub github_url: String,
    pub stars: u64,
    #[serde(default)]
    pub forks: Option<u64>,
    pub updated_at: i64,
    #[serde(default)]
    pub has_marketplace: Option<bool>,
    pub path: String,
    pub branch: String,
    #[serde(default)]
    pub description_cn: Option<String>,
    #[serde(default)]
    pub deleted: Option<bool>,
}

/// Paginated marketplace feed response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceFeedPage {
    pub items: Vec<MarketplaceSkillFeedItem>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// Input for loading marketplace feed with pagination and search
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceFeedInput {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub search: Option<String>,
}

/// Input for importing a skill from marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMarketSkillInput {
    pub github_url: String,
    pub branch: String,
    pub skill_path: String,
    pub skill_name: String,
}

// ─── Skills.sh Registry types ─────────────────────────────────────────────────

/// A skill from the skills.sh registry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySkill {
    pub id: String,
    pub skill_id: String,
    pub name: String,
    pub installs: u64,
    pub source: String,
}

/// Search result from skills.sh API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySearchResult {
    pub skills: Vec<RegistrySkill>,
    pub count: u64,
}

/// Content fetched for a registry skill
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySkillContent {
    pub content: String,
    pub branch: String,
    pub skill_path: String,
}

/// Input for installing a registry skill
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryInstallInput {
    pub skill_id: String,
    pub skill_name: String,
    pub content: String,
    pub source: String,
    pub apps: Vec<String>,
}

/// Result of installing a registry skill
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryInstallResult {
    pub installed_apps: Vec<String>,
    pub failed_apps: Vec<String>,
}

// ─── MCP Server types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
    Sse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ClaudeProjectMcpMode {
    #[default]
    Local,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpEnvVarSpec {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub is_secret: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHeaderSpec {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub is_secret: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_template: Option<String>,
}

/// Sanitized MCP Server definition persisted in `RepoLibrary` (`library.json`).
/// Secret values are NEVER stored here; they live exclusively in `local-secrets.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerRecord {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub transport: McpTransport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub env_specs: Vec<McpEnvVarSpec>,
    #[serde(default)]
    pub header_specs: Vec<McpHeaderSpec>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub project_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDto {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env_specs: Vec<McpEnvVarSpec>,
    pub header_specs: Vec<McpHeaderSpec>,
    pub tags: Vec<String>,
    pub project_ids: Vec<String>,
    pub plugin_id: Option<String>,
    pub configured_secret_keys: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMcpServerInput {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub transport: McpTransport,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub env_specs: Vec<McpEnvVarSpec>,
    #[serde(default)]
    pub header_specs: Vec<McpHeaderSpec>,
    #[serde(default)]
    pub secret_values: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub project_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMcpServerInput {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub transport: Option<McpTransport>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub env_specs: Option<Vec<McpEnvVarSpec>>,
    pub header_specs: Option<Vec<McpHeaderSpec>>,
    pub secret_values: Option<std::collections::HashMap<String, String>>,
    pub tags: Option<Vec<String>>,
    pub project_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMcpServerResult {
    pub server: McpServerDto,
    pub backup_sync: BackupSyncResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallMcpGlobalInput {
    pub server_id: String,
    pub apps: Vec<String>,
    #[serde(default)]
    pub use_env_placeholders: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallMcpToProjectInput {
    pub server_id: String,
    pub project_path: String,
    pub apps: Vec<String>,
    pub claude_mode: Option<ClaudeProjectMcpMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallMcpResult {
    pub installed_apps: Vec<String>,
    pub failed_apps: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAppInstallStatus {
    pub app_id: String,
    pub supported: bool,
    pub installed: bool,
    pub config_path: Option<String>,
    pub scope_detail: Option<String>,
    pub has_drift: bool,
    pub unsupported_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckMcpStatusInput {
    pub server_id: String,
    pub scope: String,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckMcpStatusResult {
    pub server_id: String,
    pub statuses: Vec<McpAppInstallStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMcpServerDto {
    pub slug: String,
    pub name: String,
    pub app_id: String,
    pub config_path: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub detected_env_keys: Vec<String>,
    pub detected_secret_keys: Vec<String>,
    pub is_managed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportExternalMcpInput {
    pub app_id: String,
    pub config_path: String,
    pub slug: String,
    pub import_secrets_to_local: Option<bool>,
}

// ─── Plugin / Extension types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PluginManifestKind {
    AgentPlugins,
    ClaudePlugin,
    CodexPlugin,
    CursorPlugin,
    AntigravityPlugin,
    GeminiExtension,
    #[default]
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PluginInstallMode {
    Native,
    #[default]
    Projected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUserConfigField {
    pub key: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginComponentSummary {
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub rules: Vec<String>,
}

/// Sanitized Plugin definition persisted in `RepoLibrary` (`library.json`).
/// Sensitive userConfig values are NEVER stored in `user_config_values`; they live in `local-secrets.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRecord {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    pub manifest_kind: PluginManifestKind,
    #[serde(default)]
    pub user_config_schema: Vec<PluginUserConfigField>,
    #[serde(default)]
    pub user_config_values: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub components: PluginComponentSummary,
    #[serde(default)]
    pub projected_skill_ids: Vec<String>,
    #[serde(default)]
    pub projected_command_ids: Vec<String>,
    #[serde(default)]
    pub projected_mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDto {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub repository_url: Option<String>,
    pub manifest_kind: PluginManifestKind,
    pub source_dir_path: String,
    pub user_config_schema: Vec<PluginUserConfigField>,
    pub user_config_values: std::collections::HashMap<String, String>,
    pub configured_secret_keys: Vec<String>,
    pub components: PluginComponentSummary,
    pub projected_skill_ids: Vec<String>,
    pub projected_command_ids: Vec<String>,
    pub projected_mcp_server_ids: Vec<String>,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPluginFromFolderInput {
    pub folder_path: String,
    pub project_components_to_library: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPluginFromGitInput {
    pub repo_url: String,
    pub sub_path: Option<String>,
    pub branch: Option<String>,
    pub project_components_to_library: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPluginCandidate {
    pub sub_path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub manifest_kind: PluginManifestKind,
    pub components: PluginComponentSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ImportPluginOutcome {
    #[serde(rename_all = "camelCase")]
    Imported {
        plugins: Vec<PluginDto>,
        warnings: Vec<String>,
        backup_sync: BackupSyncResult,
    },
    #[serde(rename_all = "camelCase")]
    SelectPlugins {
        staging_id: String,
        source_label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        marketplace_name: Option<String>,
        candidates: Vec<DiscoveredPluginCandidate>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStagedPluginsInput {
    pub staging_id: String,
    pub sub_paths: Vec<String>,
    pub project_components_to_library: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPluginBatchResult {
    pub plugins: Vec<PluginDto>,
    pub warnings: Vec<String>,
    pub backup_sync: BackupSyncResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePluginConfigInput {
    pub plugin_id: String,
    pub user_config_values: Option<std::collections::HashMap<String, String>>,
    pub secret_values: Option<std::collections::HashMap<String, String>>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginGlobalInput {
    pub plugin_id: String,
    pub apps: Vec<String>,
    pub prefer_native: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginToProjectInput {
    pub plugin_id: String,
    pub project_path: String,
    pub apps: Vec<String>,
    pub claude_mcp_mode: Option<ClaudeProjectMcpMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginResult {
    pub installed_apps: Vec<String>,
    pub failed_apps: Vec<String>,
    pub app_modes: std::collections::HashMap<String, PluginInstallMode>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppInstallStatus {
    pub app_id: String,
    pub installed: bool,
    pub mode: Option<PluginInstallMode>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckPluginStatusInput {
    pub plugin_id: String,
    pub scope: String,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckPluginStatusResult {
    pub plugin_id: String,
    pub statuses: Vec<PluginAppInstallStatus>,
}
