//! Credential resolution for dedicated web search backends.

use axo_ai::{Credential, CredentialStore, FileCredentialStore};
use axome_protocol::axo::{AxoCredentialSource, AxoSearchProviderStatus};
use axome_protocol::axo_settings::AxoWebSearchSettings;

pub(crate) struct SearchProviderSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub store_key: &'static str,
    pub env: &'static [&'static str],
    pub requires_key: bool,
}

pub(crate) const SEARCH_PROVIDER_SPECS: &[SearchProviderSpec] = &[
    SearchProviderSpec {
        id: "parallel",
        label: "Parallel",
        store_key: "web:parallel",
        env: &["PARALLEL_API_KEY", "AXO_PARALLEL_KEY"],
        requires_key: false,
    },
    SearchProviderSpec {
        id: "tavily",
        label: "Tavily",
        store_key: "web:tavily",
        env: &["TAVILY_API_KEY", "AXO_TAVILY_KEY"],
        requires_key: true,
    },
    SearchProviderSpec {
        id: "exa",
        label: "Exa",
        store_key: "web:exa",
        env: &["EXA_API_KEY"],
        requires_key: true,
    },
    SearchProviderSpec {
        id: "brave",
        label: "Brave Search",
        store_key: "web:brave",
        env: &["BRAVE_API_KEY"],
        requires_key: true,
    },
    SearchProviderSpec {
        id: "perplexity",
        label: "Perplexity",
        store_key: "web:perplexity",
        env: &["PERPLEXITY_API_KEY"],
        requires_key: true,
    },
];

pub(crate) fn find_search_spec(id: &str) -> Option<&'static SearchProviderSpec> {
    SEARCH_PROVIDER_SPECS.iter().find(|spec| spec.id == id)
}

pub(crate) async fn resolve_search_key(
    credentials: &FileCredentialStore,
    id: &str,
) -> Option<String> {
    let spec = find_search_spec(id)?;
    read_api_key(credentials, spec.store_key)
        .await
        .or_else(|| first_env(spec.env).map(|(_, value)| value))
}

pub(crate) fn resolve_searxng_url(settings: &AxoWebSearchSettings) -> Option<String> {
    settings
        .searxng_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| first_env(&["SEARXNG_URL"]).map(|(_, value)| value))
}

pub(crate) async fn search_provider_statuses(
    credentials: &FileCredentialStore,
    settings: &AxoWebSearchSettings,
) -> Vec<AxoSearchProviderStatus> {
    let mut out = Vec::with_capacity(SEARCH_PROVIDER_SPECS.len() + 1);
    for spec in SEARCH_PROVIDER_SPECS {
        let source = if read_api_key(credentials, spec.store_key).await.is_some() {
            AxoCredentialSource::ApiKey
        } else if let Some((name, _)) = first_env(spec.env) {
            AxoCredentialSource::Env {
                name: name.to_owned(),
            }
        } else {
            AxoCredentialSource::None
        };
        let configured = !spec.requires_key || !matches!(source, AxoCredentialSource::None);
        out.push(AxoSearchProviderStatus {
            id: spec.id.to_owned(),
            label: spec.label.to_owned(),
            requires_key: spec.requires_key,
            env: spec.env.iter().map(|s| (*s).to_owned()).collect(),
            configured,
            source,
        });
    }
    let source = if settings
        .searxng_url
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        AxoCredentialSource::Config
    } else if let Some((name, _)) = first_env(&["SEARXNG_URL"]) {
        AxoCredentialSource::Env {
            name: name.to_owned(),
        }
    } else {
        AxoCredentialSource::None
    };
    out.push(AxoSearchProviderStatus {
        id: "searxng".into(),
        label: "SearXNG".into(),
        requires_key: false,
        env: vec!["SEARXNG_URL".into()],
        configured: resolve_searxng_url(settings).is_some(),
        source,
    });
    out
}

async fn read_api_key(credentials: &FileCredentialStore, provider: &str) -> Option<String> {
    match credentials.get(provider).await.ok().flatten() {
        Some(Credential::ApiKey { key }) if !key.trim().is_empty() => Some(key),
        _ => None,
    }
}

fn first_env(names: &[&'static str]) -> Option<(&'static str, String)> {
    names.iter().find_map(|&name| {
        let value = std::env::var(name).ok()?;
        let value = value.trim();
        (!value.is_empty()).then(|| (name, value.to_owned()))
    })
}
