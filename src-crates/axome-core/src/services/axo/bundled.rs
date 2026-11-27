use std::path::Path;
use std::sync::LazyLock;

use axo::{AgentSpec, Diagnostic, KeywordSpec, ModeSpec};

fn parse_bundled<T>(
    kind: &str,
    name: &str,
    markdown: &str,
    parse: impl FnOnce(&str, &str) -> (Option<T>, Vec<Diagnostic>),
) -> T {
    let (spec, diagnostics) = parse(name, markdown);
    assert!(
        diagnostics.is_empty(),
        "bundled {kind} {name} had diagnostics: {diagnostics:?}"
    );
    spec.unwrap_or_else(|| panic!("invalid bundled {kind} {name}"))
}

static BUNDLED_AGENTS: LazyLock<Vec<AgentSpec>> = LazyLock::new(|| {
    let agent = |name, markdown| parse_bundled("agent", name, markdown, AgentSpec::parse);
    vec![
        agent("scout", include_str!("bundled/agents/scout.md")),
        agent("task", include_str!("bundled/agents/task.md")),
    ]
});

static BUNDLED_KEYWORDS: LazyLock<Vec<KeywordSpec>> = LazyLock::new(|| {
    vec![parse_bundled(
        "keyword",
        "orchestrate",
        include_str!("bundled/keywords/orchestrate.md"),
        KeywordSpec::parse,
    )]
});

static BUNDLED_MODES: LazyLock<Vec<ModeSpec>> = LazyLock::new(|| {
    let mode = |name, markdown| parse_bundled("mode", name, markdown, ModeSpec::parse);
    vec![
        mode("plan", include_str!("bundled/modes/plan.md")),
        mode(
            "plan:roadmap",
            include_str!("bundled/modes/plan/roadmap.md"),
        ),
        mode("plan:step", include_str!("bundled/modes/plan/step.md")),
        mode("research", include_str!("bundled/modes/research.md")),
    ]
});

pub(super) fn agents() -> &'static [AgentSpec] {
    &BUNDLED_AGENTS
}

pub(super) fn keywords() -> &'static [KeywordSpec] {
    &BUNDLED_KEYWORDS
}

pub(super) fn modes() -> &'static [ModeSpec] {
    &BUNDLED_MODES
}

pub(super) fn definitions(modes: Vec<ModeSpec>) -> axo::Definitions {
    axo::Definitions {
        agents: agents().to_vec(),
        modes,
        keywords: keywords().to_vec(),
        ..Default::default()
    }
}

/// The catalog for `root`, with the bundled agents, modes and keywords as the base layer.
pub(super) fn catalog(
    axo: &axo::Axo,
    root: &Path,
    access: axo::ProjectAccess,
) -> Result<axo::Catalog, axo::Error> {
    axo.catalog(root, Some(access), &definitions(modes().to_vec()))
}

/// The bundled modes for a session, with `{{plansDir}}` filled in for its folder.
pub(super) fn session_modes(plans_dir: &str) -> Vec<ModeSpec> {
    let plans_dir = plans_dir.trim_end_matches('/');
    modes()
        .iter()
        .cloned()
        .map(|mut mode| {
            mode.body = mode.body.replace("{{plansDir}}", plans_dir);
            mode
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn session_modes_fill_every_placeholder() {
        let modes = super::session_modes("docs/plans/");
        let step = modes.iter().find(|mode| mode.name == "plan:step").unwrap();
        assert!(
            step.body.contains("`docs/plans/<name>/roadmap.md`"),
            "{}",
            step.body
        );
        assert!(modes.iter().all(|mode| !mode.body.contains("{{")));
    }
}
