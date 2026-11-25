//! Slash commands Axome registers on every root session.

use std::sync::Arc;

use async_trait::async_trait;
use axo::{CommandHandler, RegisteredCommand, Session};

struct Persona;

#[async_trait]
impl CommandHandler for Persona {
    async fn run(&self, session: &Session, args: &str) -> Result<(), axo::Error> {
        let name = args.trim();
        let persona = (!name.is_empty() && !name.eq_ignore_ascii_case("default")).then_some(name);
        session.set_agent(persona).await
    }
}

pub(super) fn host_commands() -> Vec<RegisteredCommand> {
    vec![RegisteredCommand {
        name: "persona".into(),
        description: "Switch the persona; no name selects Default".into(),
        argument_hint: Some("[name]".into()),
        origin: None,
        handler: Arc::new(Persona),
    }]
}
