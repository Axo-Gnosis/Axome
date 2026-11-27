use crate::axo::AxoEnabledExtension;
use crate::chat::ChatThinking;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where accepted plans are saved when `plansDir` is unset.
pub const DEFAULT_PLANS_DIR: &str = ".axome/plans";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoSettings {
    pub extensions: Vec<AxoEnabledExtension>,
    pub project_extensions: IndexMap<String, Vec<AxoEnabledExtension>>,
    pub default_model: Option<String>,
    /// Axome-owned: persona for new chats; axo ignores the key.
    pub default_agent: Option<String>,
    /// Axome-owned: where accepted plans are saved, relative to the folder or absolute; axo ignores the key.
    pub plans_dir: String,
    pub model_roles: IndexMap<String, String>,
    pub default_thinking_level: ChatThinking,
    pub service_tier: AxoServiceTierSettings,
    pub cache_ttl: AxoCacheTtlSettings,
    pub cache_warming: AxoCacheWarming,
    pub home_agents_dir: bool,
    pub steering_mode: AxoQueueMode,
    pub follow_up_mode: AxoQueueMode,
    pub context_files: IndexMap<String, bool>,
    pub nested_context_files: bool,
    pub extended_context: bool,
    pub bash: AxoBashSettings,
    pub ask: AxoAskSettings,
    pub read: AxoReadSettings,
    pub project: AxoProjectSettings,
    pub compaction: AxoCompactionSettings,
    pub retry: AxoRetrySettings,
    pub subagents: AxoSubagentSettings,
    pub keywords: AxoKeywordSettings,
    pub web_search: AxoWebSearchSettings,
    pub docs: AxoDocsSettings,
    pub browser: AxoBrowserSettings,
}

impl Default for AxoSettings {
    fn default() -> Self {
        Self {
            extensions: Vec::new(),
            project_extensions: IndexMap::new(),
            default_model: None,
            default_agent: None,
            plans_dir: DEFAULT_PLANS_DIR.to_owned(),
            model_roles: IndexMap::new(),
            default_thinking_level: ChatThinking::Medium,
            service_tier: AxoServiceTierSettings::default(),
            cache_ttl: AxoCacheTtlSettings::default(),
            cache_warming: AxoCacheWarming::default(),
            home_agents_dir: false,
            steering_mode: AxoQueueMode::All,
            follow_up_mode: AxoQueueMode::OneAtATime,
            context_files: [
                "AGENTS.md",
                "AGENTS.local.md",
                "CLAUDE.md",
                "CLAUDE.local.md",
                ".agents/AGENTS.md",
            ]
            .into_iter()
            .map(|name| (name.to_owned(), true))
            .collect(),
            nested_context_files: true,
            extended_context: false,
            bash: AxoBashSettings::default(),
            ask: AxoAskSettings::default(),
            read: AxoReadSettings::default(),
            project: AxoProjectSettings::default(),
            compaction: AxoCompactionSettings::default(),
            retry: AxoRetrySettings::default(),
            subagents: AxoSubagentSettings::default(),
            keywords: AxoKeywordSettings::default(),
            web_search: AxoWebSearchSettings::default(),
            docs: AxoDocsSettings::default(),
            browser: AxoBrowserSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoServiceTierSettings {
    pub on_model_change: AxoTierOnModelChange,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoCacheTtlSettings {
    /// `None` picks 1h on subscription Anthropic routes, 5m otherwise.
    pub root: Option<AxoCacheTtl>,
    pub child: AxoCacheTtl,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AxoCacheTtl {
    #[default]
    #[serde(rename = "5m")]
    Short,
    #[serde(rename = "1h")]
    Long,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AxoCacheWarming {
    Off,
    Streaming,
    #[default]
    Idle,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AxoTierOnModelChange {
    #[default]
    Reset,
    Keep,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AxoSubagentTier {
    #[default]
    Standard,
    Inherit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AxoQueueMode {
    All,
    OneAtATime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AxoDevShellMode {
    Off,
    Direnv,
    Nix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AxoProjectAccess {
    Ignore,
    Read,
    Trust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AxoSummaryRequest {
    Live,
    Separate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AxoContextFiles {
    Switch(bool),
    Names(IndexMap<String, bool>),
    Nested(AxoNestedTag),
}

/// The wire spelling of axo's `ContextFiles::Nested`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AxoNestedTag {
    #[serde(rename = "nested")]
    Nested,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoBashSettings {
    pub dev_shell: AxoDevShellMode,
    pub dev_shell_ref: String,
    pub dev_shell_watch: Vec<String>,
    pub dev_shell_timeout_ms: u64,
    pub ci: bool,
    pub promote_after_ms: u64,
    pub block_tool_views: bool,
}

impl Default for AxoBashSettings {
    fn default() -> Self {
        Self {
            dev_shell: AxoDevShellMode::Off,
            dev_shell_ref: ".#default".to_owned(),
            dev_shell_watch: Vec::new(),
            dev_shell_timeout_ms: 600_000,
            ci: true,
            promote_after_ms: 60_000,
            block_tool_views: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoAskSettings {
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoReadSettings {
    pub outline_min_bytes: u64,
}

impl Default for AxoReadSettings {
    fn default() -> Self {
        Self {
            outline_min_bytes: 24_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoProjectSettings {
    pub files: AxoProjectFiles,
    pub access: AxoProjectAccess,
    pub paths: IndexMap<String, AxoProjectAccess>,
}

impl Default for AxoProjectSettings {
    fn default() -> Self {
        Self {
            files: AxoProjectFiles::default(),
            access: AxoProjectAccess::Read,
            paths: IndexMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoProjectFiles {
    pub skills: bool,
    pub config: bool,
}

impl Default for AxoProjectFiles {
    fn default() -> Self {
        Self {
            skills: true,
            config: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoCompactionSettings {
    pub enabled: bool,
    pub max_context: Option<u64>,
    pub reserve_tokens: Option<u64>,
    pub keep_recent_tokens: Option<u64>,
    pub summary_request: AxoSummaryRequest,
}

impl Default for AxoCompactionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_context: Some(400_000),
            reserve_tokens: None,
            keep_recent_tokens: None,
            summary_request: AxoSummaryRequest::Live,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoRetrySettings {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for AxoRetrySettings {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 2_000,
            max_delay_ms: 60_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoSubagentSettings {
    pub max_depth: u32,
    pub max_concurrent: usize,
    pub max_turns: u32,
    pub default_model: Option<String>,
    pub models: IndexMap<String, String>,
    pub service_tier: AxoSubagentTier,
    pub context_files: AxoContextFiles,
}

impl Default for AxoSubagentSettings {
    fn default() -> Self {
        Self {
            max_depth: 1,
            max_concurrent: 8,
            max_turns: 50,
            default_model: None,
            models: IndexMap::new(),
            service_tier: AxoSubagentTier::default(),
            context_files: AxoContextFiles::Nested(AxoNestedTag::Nested),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoKeywordSettings {
    pub enabled: bool,
    pub disabled: Vec<String>,
}

impl Default for AxoKeywordSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoWebSearchSettings {
    pub enabled: bool,
    pub providers: Vec<String>,
    pub searxng_url: Option<String>,
    pub grounded_model: Option<String>,
}

impl Default for AxoWebSearchSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            providers: Vec::new(),
            searxng_url: None,
            grounded_model: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoDocsSettings {
    pub enabled: bool,
    pub ttl_hours: u32,
    pub retention_days: u32,
    pub sources: Vec<String>,
}

impl Default for AxoDocsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_hours: 24,
            retention_days: 30,
            sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AxoBrowserSettings {
    pub enabled: bool,
    pub headless: bool,
    pub cdp_url: Option<String>,
    pub executable_path: Option<String>,
    pub idle_close_secs: u64,
}

impl Default for AxoBrowserSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            headless: true,
            cdp_url: None,
            executable_path: None,
            idle_close_secs: 1800,
        }
    }
}
