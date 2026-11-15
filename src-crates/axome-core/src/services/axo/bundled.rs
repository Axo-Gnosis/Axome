use std::sync::LazyLock;

use axo::AgentSpec;

fn parse_bundled(name: &str, markdown: &str) -> AgentSpec {
    let (spec, diagnostics) = AgentSpec::parse(name, markdown);
    assert!(
        diagnostics.is_empty(),
        "bundled agent {name} had diagnostics: {diagnostics:?}"
    );
    spec.unwrap_or_else(|| panic!("invalid bundled agent {name}"))
}

static BUNDLED_AGENTS: LazyLock<Vec<AgentSpec>> = LazyLock::new(|| {
    vec![
        parse_bundled("scout", include_str!("bundled/agents/scout.md")),
        parse_bundled("task", include_str!("bundled/agents/task.md")),
    ]
});

pub(super) fn agents() -> &'static [AgentSpec] {
    &BUNDLED_AGENTS
}
