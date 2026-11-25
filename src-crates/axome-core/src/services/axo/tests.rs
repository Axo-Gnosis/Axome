use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axo::store::JsonlStore;
use axo::{Axo, Command, ConfigLayer, ConfigSource, PromptRequest, SessionOptions, UserConfig};
use axo_ai::testing::{reply_text, reply_tool_calls};
use axo_ai::{FileCredentialStore, Models, ModelsConfig, ModelsOptions, ScriptedApi, ScriptedTurn};
use axome_protocol::chat::{
    ChatAnswer, ChatAnswerValue, ChatAskBlock, ChatCommandKind, ChatEntry, ChatEntryBody,
    ChatEvent, ChatJobKind, ChatJobStatus, ChatLinkTarget, ChatLive, ChatOutputChunk,
    ChatOutputStream, ChatPart, ChatRunOutcome, ChatServiceTier, ChatThinking,
};
use axome_protocol::memory::{MemoryCaptureQuery, MemoryCaptureTrigger, MemoryFactOutcome};
use base64::Engine as _;
use parking_lot::Mutex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::engine::Engine;
use super::{live, AxoService};
use crate::events::{EventSink, HostEvent};
use crate::services::db::DatabaseService;

struct FixtureConfig(Value);

impl ConfigSource for FixtureConfig {
    fn user(&self) -> Result<UserConfig, axo::Error> {
        Ok(UserConfig {
            layer: ConfigLayer {
                origin: "fixture".into(),
                settings: Some(self.0.clone()),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn project(&self, cwd: &Path) -> Result<ConfigLayer, axo::Error> {
        Ok(ConfigLayer {
            agent_roots: vec![cwd.join(".axome").join("agents")],
            ..Default::default()
        })
    }
}

type Events = Arc<Mutex<Vec<ChatEvent>>>;

struct Fixture {
    service: Arc<AxoService>,
    folder: PathBuf,
    dir: PathBuf,
    api: Arc<ScriptedApi>,
    host_events: Arc<Mutex<Vec<HostEvent>>>,
}

impl Fixture {
    async fn new(turns: Vec<ScriptedTurn>) -> Self {
        Self::with_settings(turns, json!({})).await
    }

    async fn with_settings(turns: Vec<ScriptedTurn>, settings: Value) -> Self {
        let host_events = Arc::new(Mutex::new(Vec::new()));
        let collected = Arc::clone(&host_events);
        let events: EventSink<HostEvent> = Arc::new(move |event| {
            collected.lock().push(event);
            Ok(())
        });
        let dir = std::env::temp_dir().join(format!("axome-axo-{}", uuid::Uuid::new_v4()));
        let folder = dir.join("work");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(folder.join("src")).unwrap();
        std::fs::create_dir_all(folder.join("docs")).unwrap();
        std::fs::write(folder.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
        std::fs::write(folder.join("docs/guide.md"), "# Guide\n").unwrap();
        std::fs::write(folder.join("notes.txt"), "Remember the greeting.\n").unwrap();
        std::fs::write(folder.join("pixel.png"), pixel()).unwrap();
        write_skill(
            &folder,
            "greet",
            "<name> [tone]",
            "Say hello to $1 in a ${2:-friendly} tone.",
        );
        let config = ModelsConfig::parse_jsonc(
            &json!({"providers": {"test": {
                "api": "openai-completions",
                "baseUrl": "http://127.0.0.1:9/v1",
                "models": [
                    {"id": "m", "input": ["text", "image"]},
                    {"id": "m2", "input": ["text", "image"]}
                ]
            }}})
            .to_string(),
        )
        .unwrap();
        let models = Arc::new(
            Models::load(ModelsOptions {
                cache_dir: None,
                config,
                extended_context: false,
                credentials: Arc::new(FileCredentialStore::new(dir.join("auth.json"))),
                http_client: None,
            })
            .await
            .unwrap(),
        );
        super::engine::register_providers(&models).unwrap();
        let api = Arc::new(ScriptedApi::new(turns));
        let axo = Axo::builder(
            models.clone(),
            Arc::new(JsonlStore::new(dir.join("sessions"))),
        )
        .config(Arc::new(FixtureConfig(settings)))
        .streamer(api.clone())
        .build();
        let db = Arc::new(DatabaseService::new(dir.join("test.db")).unwrap());
        let dirs = crate::HostDirs::under(&dir);
        let axo = Arc::new(axo);
        let extensions = super::extensions::ExtensionService::new(axo.clone(), &dirs, None)
            .await
            .unwrap();
        let service = AxoService::with_engine_and_events(
            db,
            dirs,
            Engine {
                axo,
                models,
                extensions,
            },
            events,
        )
        .await;
        Self {
            service,
            folder,
            dir,
            api,
            host_events,
        }
    }

    async fn create(&self) -> String {
        self.service
            .create(
                &self.folder,
                "test/m".into(),
                ChatThinking::Off,
                ChatServiceTier::Standard,
            )
            .await
            .unwrap()
    }

    async fn attach(&self, id: &str, attach_id: &str) -> Events {
        let (events, sink) = collector();
        self.service
            .attach(&self.folder, id, attach_id.into(), sink)
            .await
            .unwrap();
        events
    }

    async fn prompt(&self, id: &str, text: &str, events: &Events, runs: usize) {
        self.service
            .prompt(
                &self.folder,
                id,
                format!("request-{text}"),
                text_parts(text),
            )
            .await
            .unwrap();
        wait_settled(events, runs).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn anthropic_auth_requires_login_without_catalog_or_api_key() {
    use axome_protocol::axo::{AxoCredentialSource, AxoProviderStatus};

    let fixture = Fixture::new(vec![]).await;
    let before = fixture.service.providers(None).await.unwrap();
    let provider = before
        .providers
        .iter()
        .find(|provider| provider.id == "anthropic-auth")
        .expect("subscription login must remain visible without catalog rows");
    assert_eq!(provider.models, 0);
    assert!(provider.requires_key);
    assert!(provider.oauth_only);
    assert!(provider.login);
    assert!(provider.env.is_empty());
    assert!(matches!(
        provider.status,
        AxoProviderStatus::NeedsCredentials
    ));
    assert!(matches!(provider.source, AxoCredentialSource::None));

    let error = fixture
        .service
        .provider_set_key("anthropic-auth", "synthetic-api-key".into(), None)
        .await
        .expect_err("the service must reject API keys for subscription login");
    assert!(
        error.to_string().contains("Log in to anthropic-auth"),
        "{error}"
    );

    let after = fixture.service.providers(None).await.unwrap();
    let provider = after
        .providers
        .iter()
        .find(|provider| provider.id == "anthropic-auth")
        .unwrap();
    assert!(matches!(provider.source, AxoCredentialSource::None));
    assert!(matches!(
        provider.status,
        AxoProviderStatus::NeedsCredentials
    ));
}

fn collector() -> (Events, EventSink<ChatEvent>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let collected = Arc::clone(&events);
    let sink: EventSink<ChatEvent> = Arc::new(move |event| {
        collected.lock().push(event);
        Ok(())
    });
    (events, sink)
}

async fn wait_settled(events: &Events, runs: usize) {
    wait_until(|| settled(&events.lock(), runs)).await;
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.service.shutdown();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn isolated_reopen_test(name: &str) -> bool {
    let test = format!("services::axo::tests::{name}");
    const MARKER: &str = "AXOME_ISOLATED_TEST";
    if std::env::var(MARKER).as_deref() == Ok(test.as_str()) {
        return false;
    }
    // The pinned axo writer releases flock by closing its file. Unrelated PTY tests
    // can fork with that descriptor, keeping the lock alive until their child execs.
    // Isolate reopen assertions from inherited descriptors, not from retirement tasks.
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &test, "--nocapture"])
        .env(MARKER, &test)
        .status()
        .expect("launch isolated axo reopen test");
    assert!(
        status.success(),
        "isolated axo reopen test failed: {status}"
    );
    true
}

async fn wait_until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !predicate() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("timed out waiting for axo events");
}

fn settled(events: &[ChatEvent], runs: usize) -> bool {
    let done = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ChatEvent::Entry {
                    entry: ChatEntry {
                        body: ChatEntryBody::RunEnd {
                            outcome: ChatRunOutcome::Done,
                            ..
                        },
                        ..
                    }
                }
            )
        })
        .count();
    done == runs
        && events.iter().rev().find_map(|event| match event {
            ChatEvent::Live { live } => Some(idle(live)),
            ChatEvent::Snapshot { state } => Some(idle(&state.live)),
            _ => None,
        }) == Some(true)
}

/// Live idle comes from axo's `Activity`, published after queued input and wakes drain.
fn idle(live: &ChatLive) -> bool {
    !live.running && live.maintenance.is_none() && live.questions.is_empty()
}

fn entries(events: &[ChatEvent]) -> Vec<ChatEntry> {
    let mut entries = Vec::new();
    for event in events {
        match event {
            ChatEvent::Snapshot { state } => entries.clone_from(&state.entries),
            ChatEvent::Entry { entry } => entries.push(entry.clone()),
            _ => {}
        }
    }
    entries
}

fn last_user(entries: &[ChatEntry]) -> &ChatEntry {
    entries
        .iter()
        .rev()
        .find(|entry| matches!(entry.body, ChatEntryBody::User { .. }))
        .expect("user entry")
}

#[tokio::test(flavor = "multi_thread")]
async fn two_subscribers_see_same_stream() {
    let fixture = Fixture::new(vec![ScriptedTurn::Reply(reply_text("hello"))]).await;
    let id = fixture.create().await;
    let a = fixture.attach(&id, "a").await;
    let b = fixture.attach(&id, "b").await;
    fixture.prompt(&id, "hi", &a, 1).await;
    wait_until(|| settled(&b.lock(), 1)).await;
    let a = a.lock();
    let b = b.lock();
    assert!(matches!(a.first(), Some(ChatEvent::Snapshot { .. })));
    assert!(matches!(b.first(), Some(ChatEvent::Snapshot { .. })));
    let user = a
        .iter()
        .position(|event| {
            matches!(event,
                ChatEvent::Entry { entry: ChatEntry { body: ChatEntryBody::User { parts, .. }, .. } }
                if parts == &text_parts("hi")
            )
        })
        .expect("user entry");
    let assistant = a
        .iter()
        .position(|event| {
            matches!(
                event,
                ChatEvent::Entry {
                    entry: ChatEntry {
                        body: ChatEntryBody::Assistant { .. },
                        ..
                    }
                }
            )
        })
        .expect("assistant entry");
    let end = a
        .iter()
        .position(|event| {
            matches!(
                event,
                ChatEvent::Entry {
                    entry: ChatEntry {
                        body: ChatEntryBody::RunEnd {
                            outcome: ChatRunOutcome::Done,
                            ..
                        },
                        ..
                    }
                }
            )
        })
        .expect("successful run end");
    assert!(user < assistant && assistant < end);
    assert_eq!(*a, *b);
}

#[tokio::test(flavor = "multi_thread")]
async fn memory_capture_is_cursor_bound_and_never_recursive() {
    if isolated_reopen_test("memory_capture_is_cursor_bound_and_never_recursive") {
        return;
    }
    let fixture = Fixture::with_settings(
        vec![
            ScriptedTurn::Reply(reply_text("Agreed, the lazy icon loader stays.")),
            ScriptedTurn::Reply(reply_text("That is the project decision.")),
            ScriptedTurn::Reply(reply_text("Writer-directory session.")),
        ],
        json!({"modelRoles": {"memory": "test/m2"}}),
    )
    .await;
    fixture.api.lane(
        "m2",
        vec![ScriptedTurn::Reply(reply_text(
            "proj/Icons :: [decision] The lazy icon loader stays.",
        ))],
    );
    let dirs = crate::HostDirs::under(&fixture.dir);
    std::fs::create_dir_all(&dirs.config).unwrap();
    std::fs::write(
        dirs.config.join("settings.jsonc"),
        r#"{"memory":{"enabled":true,"semantic":false}}"#,
    )
    .unwrap();
    let id = fixture.create().await;
    let events = fixture.attach(&id, "memory").await;
    fixture
        .prompt(&id, "Keep the lazy icon loader.", &events, 1)
        .await;
    fixture
        .prompt(
            &id,
            "Yes, remember that decision for this project.",
            &events,
            2,
        )
        .await;
    let (axo, path) = {
        let sessions = fixture.service.sessions.lock();
        let live = &sessions[&id];
        (live.engine.axo.clone(), live.session.path().to_owned())
    };
    fixture.service.close(&id).await.unwrap();
    wait_until(|| !fixture.service.memory.capture_pending()).await;
    let note = dirs.data.join("memory/banks/main/proj/Icons.md");
    assert!(std::fs::read_to_string(&note)
        .unwrap()
        .contains("- [decision] The lazy icon loader stays."));
    let runs = crate::services::memory::captures::list(
        &fixture.service.db,
        &MemoryCaptureQuery {
            session_id: Some(id.clone()),
            bank: None,
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.trigger, MemoryCaptureTrigger::Close);
    assert!(run.error.is_none(), "{run:?}");
    assert!(run.ended_at.is_some());
    assert_eq!(run.facts.len(), 1);
    let fact = &run.facts[0];
    assert_eq!(fact.outcome, MemoryFactOutcome::New);
    assert_eq!(fact.note, "proj/Icons");
    // Forget must remove exactly the captured line, once.
    let reverted = fixture.service.memory.revert_fact(fact.id).await.unwrap();
    assert!(reverted.facts[0].reverted);
    assert!(!std::fs::read_to_string(&note)
        .unwrap()
        .contains("The lazy icon loader stays."));
    assert!(fixture.service.memory.revert_fact(fact.id).await.is_err());
    let (_, entries) = axo.open_session_read(&path).await.unwrap();
    let last_seq = entries.last().unwrap().seq;
    let cursor: u64 = fixture
        .service
        .db
        .read()
        .conn()
        .query_row(
            "SELECT seq FROM memory_cursors WHERE session_id = ?1",
            [&id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cursor, last_seq);
    assert_eq!(fixture.api.requests().len(), 3);
    let writer_dir = dirs.data.join("memory/writer");
    let writer_count = axo.list_sessions(&writer_dir).await.unwrap().len();
    assert_eq!(writer_count, 0);
    fixture.attach(&id, "reopened").await;
    let entry_id = entries
        .iter()
        .rev()
        .find_map(|seq| seq.entry.id())
        .unwrap()
        .to_owned();
    let fork = fixture
        .service
        .fork(&fixture.folder, &id, entry_id)
        .await
        .unwrap();
    fixture.service.close(&fork.session_id).await.unwrap();
    wait_until(|| !fixture.service.memory.capture_pending()).await;
    assert_eq!(fixture.api.requests().len(), 3);
    let fork_cursors: u64 = fixture
        .service
        .db
        .read()
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM memory_cursors WHERE session_id = ?1",
            [&fork.session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fork_cursors, 1);
    // A chat only viewed, never conversed in, must not capture even without a cursor.
    fixture
        .service
        .db
        .write()
        .conn()
        .execute("DELETE FROM memory_cursors WHERE session_id = ?1", [&id])
        .unwrap();
    fixture.service.close(&id).await.unwrap();
    wait_until(|| !fixture.service.memory.capture_pending()).await;
    assert_eq!(fixture.api.requests().len(), 3);
    assert_eq!(
        axo.list_sessions(&writer_dir).await.unwrap().len(),
        writer_count
    );

    let writer_id = fixture
        .service
        .create(
            &writer_dir,
            "test/m".into(),
            ChatThinking::Off,
            ChatServiceTier::Standard,
        )
        .await
        .unwrap();
    let (writer_events, sink) = collector();
    fixture
        .service
        .attach(&writer_dir, &writer_id, "writer".into(), sink)
        .await
        .unwrap();
    fixture
        .service
        .prompt(
            &writer_dir,
            &writer_id,
            "writer-cwd".into(),
            text_parts("This session must not capture itself."),
        )
        .await
        .unwrap();
    wait_settled(&writer_events, 1).await;
    fixture.service.close(&writer_id).await.unwrap();
    assert!(!fixture.service.memory.capture_pending());
    let cursors: u64 = fixture
        .service
        .db
        .read()
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM memory_cursors WHERE session_id = ?1",
            [&writer_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cursors, 0);
    assert_eq!(fixture.api.requests().len(), 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn ask_round_trip() {
    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_tool_calls(&[(
            "ask-1",
            "ask",
            json!({
                "blocks": [{
                    "type": "choice",
                    "id": "choice", "question": "Which file?",
                    "options": [
                        {"value": "first", "label": "first"},
                        {"value": "second", "label": "second"}
                    ]
                }]
            }),
        )])),
        ScriptedTurn::Reply(reply_text("Opening the first file.")),
    ])
    .await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;
    fixture
        .service
        .prompt(
            &fixture.folder,
            &id,
            "ask".into(),
            text_parts("Choose a file"),
        )
        .await
        .unwrap();
    wait_until(|| {
        events.lock().iter().any(|event| {
            matches!(event,
                ChatEvent::Live { live } if live.questions.len() == 1
            )
        })
    })
    .await;
    let pending = events
        .lock()
        .iter()
        .rev()
        .find_map(|event| match event {
            ChatEvent::Live { live } if live.questions.len() == 1 => {
                Some(live.questions[0].clone())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(pending.blocks.len(), 1);
    let ChatAskBlock::Choice {
        id: question_id,
        options,
        ..
    } = &pending.blocks[0]
    else {
        panic!("expected the pending choice block");
    };
    assert_eq!(question_id, "choice");
    assert_eq!(options[0].value, "first");
    fixture
        .service
        .answer(
            &fixture.folder,
            &id,
            pending.call_id.clone(),
            vec![ChatAnswer {
                id: "choice".into(),
                value: Some(ChatAnswerValue::Text("first".into())),
                other: None,
                note: None,
            }],
        )
        .await
        .unwrap();
    wait_until(|| settled(&events.lock(), 1)).await;
    let events = events.lock();
    let live = events
        .iter()
        .rev()
        .find_map(|event| match event {
            ChatEvent::Live { live } => Some(live),
            _ => None,
        })
        .unwrap();
    assert!(live.questions.is_empty());
    assert!(entries(&events).iter().any(|entry| matches!(&entry.body,
        ChatEntryBody::ToolResult { call_id, is_error: false, .. } if call_id == "ask-1"
    )));
}

#[tokio::test(flavor = "multi_thread")]
async fn edit_creates_sibling_and_switches_back() {
    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_text("first reply")),
        ScriptedTurn::Reply(reply_text("second reply")),
        ScriptedTurn::Reply(reply_text("edited reply")),
    ])
    .await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;
    fixture.prompt(&id, "one", &events, 1).await;
    fixture.prompt(&id, "two", &events, 2).await;
    let two = last_user(&entries(&events.lock())).clone();
    let ChatEntryBody::User { parts, anchor, .. } = &two.body else {
        unreachable!()
    };
    assert_eq!(parts, &text_parts("two"));
    fixture
        .service
        .navigate(&fixture.folder, &id, anchor.clone())
        .await
        .unwrap();
    fixture.prompt(&id, "three", &events, 3).await;

    // A fresh subscriber must see the edited branch and all its sibling metadata.
    let edited = fixture.attach(&id, "edited").await;
    {
        let snapshot = edited.lock();
        let Some(ChatEvent::Snapshot { state }) = snapshot.first() else {
            panic!("attach must begin with a snapshot");
        };
        let ChatEntryBody::User {
            parts,
            anchor: edited_anchor,
            siblings,
            ..
        } = &last_user(&state.entries).body
        else {
            unreachable!()
        };
        assert_eq!(parts, &text_parts("three"));
        assert_eq!(edited_anchor, anchor);
        assert_eq!(siblings.len(), 2);
        assert!(siblings.contains(&two.id));
    }

    fixture
        .service
        .select_branch(&fixture.folder, &id, two.id)
        .await
        .unwrap();
    wait_until(|| {
        events.lock().iter().rev().find_map(|event| match event {
            ChatEvent::Snapshot { state } => Some(matches!(&last_user(&state.entries).body,
                ChatEntryBody::User { parts, .. } if parts == &text_parts("two")
            )),
            _ => None,
        }) == Some(true)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_session_unloads_and_reopens() {
    if isolated_reopen_test("idle_session_unloads_and_reopens") {
        return;
    }
    let fixture = Fixture::new(vec![ScriptedTurn::Reply(reply_text("hello"))]).await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;
    fixture.prompt(&id, "hi", &events, 1).await;
    let before = entries(&events.lock());
    fixture.service.detach(&id, "a");
    wait_until(|| fixture.service.loaded() == 0).await;
    assert_eq!(fixture.service.loaded(), 0);
    let reopened = fixture.attach(&id, "reopened").await;
    let reopened = reopened.lock();
    let Some(ChatEvent::Snapshot { state }) = reopened.first() else {
        panic!("reopen must begin with a snapshot");
    };
    assert_eq!(state.session_id, id);
    assert_eq!(state.entries, before);
    assert!(!state.live.running);
    assert_eq!(fixture.service.loaded(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn list_history_reads_title_and_model() {
    let fixture = Fixture::new(vec![ScriptedTurn::Reply(reply_text("hello"))]).await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;
    fixture.prompt(&id, "hi", &events, 1).await;
    fixture
        .service
        .run_command(
            &fixture.folder,
            &id,
            "model".into(),
            json!({"model": "test/m2"}),
        )
        .await
        .unwrap();
    let service = Arc::clone(&fixture.service);
    let folder = fixture.folder.clone();
    let listing = tokio::task::spawn_blocking(move || service.list_history(&folder))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(listing.rows.len(), 1);
    let row = &listing.rows[0];
    assert_eq!(row.id, id);
    assert_eq!(row.title.as_deref(), Some("hi"));
    assert_eq!(
        row.last_message,
        Some(axome_protocol::session::LastMessage {
            role: axome_protocol::session::MessageRole::Agent,
            text: "hello".into(),
        })
    );
    assert_eq!(row.message_count, Some(2));
    assert_eq!(row.model.as_deref(), Some("test/m"));
    assert!(row.created_at.is_some());
    assert!(row.updated_at.is_some());
}

#[test]
fn list_history_rejects_a_stopped_runtime() {
    let dir = std::env::temp_dir().join(format!("axome-axo-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Arc::new(DatabaseService::new(dir.join("test.db")).unwrap());
    let host_events = Arc::new(Mutex::new(Vec::new()));
    let collected = Arc::clone(&host_events);
    let events: EventSink<HostEvent> = Arc::new(move |event| {
        collected.lock().push(event);
        Ok(())
    });
    let fixture = Fixture {
        service: AxoService::new(
            db,
            crate::HostDirs::under(&dir),
            axome_protocol::server_config::Supervision::ProcessGroup,
            None,
            events,
        ),
        folder: dir.clone(),
        dir,
        api: Arc::new(ScriptedApi::default()),
        host_events,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    {
        let _entered = runtime.enter();
        fixture.service.bind_runtime();
    }
    drop(runtime);
    let error = fixture.service.list_history(&fixture.folder).unwrap_err();
    assert!(error.contains("cancelled"), "{error}");
}

const PIXEL: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

fn pixel() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(PIXEL)
        .unwrap()
}

fn text_parts(text: &str) -> Vec<ChatPart> {
    vec![ChatPart::Text { text: text.into() }]
}

fn write_skill(folder: &Path, name: &str, hint: &str, body: &str) {
    let dir = folder.join(".agents/skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Greeting fixture\nargument-hint: '{hint}'\n---\n{body}\n"),
    ).unwrap();
}

fn mixed_parts(folder: &Path) -> Vec<ChatPart> {
    vec![
        ChatPart::Text {
            text: "Look at ".into(),
        },
        ChatPart::File {
            path: "src/lib.rs".into(),
        },
        ChatPart::Text {
            text: " and ".into(),
        },
        ChatPart::File {
            path: "docs/".into(),
        },
        ChatPart::Text {
            text: " please. ".into(),
        },
        ChatPart::Content {
            name: "notes.txt".into(),
            bytes: base64::engine::general_purpose::STANDARD
                .encode(std::fs::read(folder.join("notes.txt")).unwrap()),
        },
        ChatPart::Text { text: " ".into() },
        ChatPart::Content {
            name: "pixel.png".into(),
            bytes: PIXEL.into(),
        },
    ]
}

fn session_path(fixture: &Fixture, id: &str) -> PathBuf {
    fixture
        .service
        .sessions
        .lock()
        .get(id)
        .unwrap()
        .session
        .path()
        .to_owned()
}

async fn submit_parts(fixture: &Fixture, id: &str, events: &Events) {
    fixture
        .service
        .prompt(
            &fixture.folder,
            id,
            "parts".into(),
            mixed_parts(&fixture.folder),
        )
        .await
        .unwrap();
    wait_settled(events, 1).await;
}

fn jsonl(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn normalized_jsonl(path: &Path, root: &Path) -> Vec<Value> {
    let mut rows = jsonl(path);
    let mut identities = HashMap::new();
    for (position, row) in rows.iter().enumerate() {
        for key in ["id", "submissionId", "runId", "sessionId"] {
            if let Some(value) = row.get(key).and_then(Value::as_str) {
                identities
                    .entry(value.to_owned())
                    .or_insert_with(|| format!("{key}-{position}"));
            }
        }
    }
    fn normalize(value: &mut Value, root: &str, identities: &HashMap<String, String>) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    if key == "timestamp" {
                        *value = json!("<timestamp>");
                    } else {
                        normalize(value, root, identities);
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    normalize(value, root, identities);
                }
            }
            Value::String(text) => {
                if let Some(identity) = identities.get(text) {
                    *text = identity.clone();
                } else {
                    *text = text.replace(root, "<fixture>");
                }
            }
            _ => {}
        }
    }
    for row in &mut rows {
        normalize(row, root.to_str().unwrap(), &identities);
    }
    rows
}

fn sha256(path: &Path) -> String {
    format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()))
}

fn replies() -> Vec<ScriptedTurn> {
    vec![
        ScriptedTurn::Reply(reply_text("Files received.")),
        ScriptedTurn::Reply(reply_text("Hello Alice!")),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn structured_prompts_and_commands_match_direct_axo_jsonl() {
    let direct = Fixture::new(replies()).await;
    let wrapped = Fixture::new(replies()).await;
    let mut tools = direct.service.web.host_tools(&direct.folder);
    tools.extend(
        direct
            .service
            .browser
            .host_tools(super::config::browser_config(
                &direct.service.dirs,
                &direct.folder,
            )),
    );
    let session = direct
        .service
        .axo()
        .await
        .unwrap()
        .create_session(SessionOptions {
            cwd: direct.folder.clone(),
            model: Some("test/m".into()),
            thinking: Some(axo_ai::ThinkingLevel::Off),
            ask_enabled: true,
            host: axo::Registrations {
                definitions: super::bundled::definitions(super::bundled::modes().to_vec()),
                tools,
                schemes: direct.service.web.host_schemes(),
                ..Default::default()
            },
            ..Default::default()
        })
        .await
        .unwrap();
    session
        .prompt_with(PromptRequest {
            input: axo::PromptInput {
                parts: vec![
                    axo::PromptPart::Text {
                        text: "Look at ".into(),
                    },
                    axo::PromptPart::File {
                        path: "src/lib.rs".into(),
                    },
                    axo::PromptPart::Text {
                        text: " and ".into(),
                    },
                    axo::PromptPart::File {
                        path: "docs/".into(),
                    },
                    axo::PromptPart::Text {
                        text: " please. ".into(),
                    },
                    axo::PromptPart::Content {
                        name: "notes.txt".into(),
                        bytes: std::fs::read(direct.folder.join("notes.txt")).unwrap(),
                    },
                    axo::PromptPart::Text { text: " ".into() },
                    axo::PromptPart::Content {
                        name: "pixel.png".into(),
                        bytes: pixel(),
                    },
                ],
            },
            request_id: Some("parts".into()),
        })
        .await
        .unwrap();
    let id = wrapped.create().await;
    let events = wrapped.attach(&id, "parity").await;
    submit_parts(&wrapped, &id, &events).await;
    for (name, args) in [
        ("thinking", json!({"level": "high"})),
        ("model", json!({"model": "test/m2"})),
    ] {
        session
            .run_command(Command::decode(name, args.clone()).unwrap())
            .await
            .unwrap();
        wrapped
            .service
            .run_command(&wrapped.folder, &id, name.into(), args)
            .await
            .unwrap();
    }
    session
        .prompt_with(PromptRequest {
            input: axo::PromptInput {
                parts: vec![axo::PromptPart::Text {
                    text: "/greet Alice cheerful".into(),
                }],
            },
            request_id: Some("request-/greet Alice cheerful".into()),
        })
        .await
        .unwrap();
    wrapped
        .prompt(&id, "/greet Alice cheerful", &events, 2)
        .await;
    let wrapped_path = session_path(&wrapped, &id);
    assert_eq!(
        normalized_jsonl(session.path(), &direct.dir),
        normalized_jsonl(&wrapped_path, &wrapped.dir),
    );
    for name in ["notes.txt", "pixel.png"] {
        assert_eq!(
            sha256(&session.path().with_extension("").join("local").join(name)),
            sha256(&wrapped_path.with_extension("").join("local").join(name)),
        );
    }
    session.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_receives_exact_file_references_and_image_blocks() {
    let fixture = Fixture::new(replies()).await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "provider").await;
    submit_parts(&fixture, &id, &events).await;
    let requests = fixture.api.requests();
    assert_eq!(requests.len(), 1);
    let user = requests[0]
        .transcript
        .messages()
        .iter()
        .find_map(|message| match message {
            axo_ai::Message::User(user) => Some(&user.content),
            _ => None,
        })
        .expect("provider user message");
    assert_eq!(
        serde_json::to_value(user).unwrap(),
        json!([
            {"type": "text", "text": "Look at @src/lib.rs and @docs/ please. @local://notes.txt @local://pixel.png"},
            {"type": "text", "text": "local://pixel.png"},
            {"type": "image", "data": PIXEL, "mimeType": "image/png"},
        ])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn saved_parts_survive_retirement_reopen_and_editable_fork() {
    if isolated_reopen_test("saved_parts_survive_retirement_reopen_and_editable_fork") {
        return;
    }
    let fixture = Fixture::new(replies()).await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "saved").await;
    submit_parts(&fixture, &id, &events).await;
    let user = last_user(&entries(&events.lock())).clone();
    let ChatEntryBody::User { parts, .. } = &user.body else {
        unreachable!()
    };
    let mut expected = mixed_parts(&fixture.folder);
    expected[5] = ChatPart::Saved {
        url: "local://notes.txt".into(),
        image: false,
    };
    expected[7] = ChatPart::Saved {
        url: "local://pixel.png".into(),
        image: true,
    };
    assert_eq!(parts, &expected);
    fixture.service.detach(&id, "saved");
    wait_until(|| fixture.service.loaded() == 0).await;
    let reopened = fixture.attach(&id, "reopened-parts").await;
    assert_eq!(last_user(&entries(&reopened.lock())), &user);
    let fork = fixture
        .service
        .fork(&fixture.folder, &id, user.id)
        .await
        .unwrap();
    assert_eq!(fork.editable.as_ref(), Some(&expected));
    for part in expected {
        if let ChatPart::Saved { url, .. } = part {
            let original = fixture
                .service
                .resolve_link(&fixture.folder, &id, url.clone())
                .await
                .unwrap();
            let copied = fixture
                .service
                .resolve_link(&fixture.folder, &fork.session_id, url)
                .await
                .unwrap();
            let (
                ChatLinkTarget::File { path: original, .. },
                ChatLinkTarget::File { path: copied, .. },
            ) = (original, copied)
            else {
                panic!("saved parts must resolve to files")
            };
            assert_ne!(original, copied);
            assert_eq!(sha256(Path::new(&original)), sha256(Path::new(&copied)));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn links_resolve_loaded_and_unloaded_without_recovery_or_writes() {
    let fixture = Fixture::new(replies()).await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "links").await;
    submit_parts(&fixture, &id, &events).await;
    let path = session_path(&fixture, &id);
    let artifacts = path.with_extension("").join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let artifact = artifacts.join("0.bash.fixture.log");
    std::fs::write(&artifact, "Full fixture output\n").unwrap();
    let urls = ["artifact://0", "local://notes.txt", "skill://greet"];
    let before = std::fs::read(&path).unwrap();
    let files = [
        artifact,
        path.with_extension("").join("local/notes.txt"),
        fixture.folder.join(".agents/skills/greet/SKILL.md"),
    ];
    let file_hashes = files.iter().map(|path| sha256(path)).collect::<Vec<_>>();
    let requests_before = fixture.api.requests().len();
    let mut targets = Vec::new();
    for url in urls {
        let target = fixture
            .service
            .resolve_link(&fixture.folder, &id, url.into())
            .await
            .unwrap();
        let ChatLinkTarget::File { path, is_dir } = &target else {
            panic!("fixture URL must resolve to a file: {url}");
        };
        assert!(Path::new(path).is_file());
        assert!(!is_dir);
        targets.push(target);
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(fixture.api.requests().len(), requests_before);
    for (file, hash) in files.iter().zip(&file_hashes) {
        assert_eq!(&sha256(file), hash);
    }
    fixture.service.detach(&id, "links");
    wait_until(|| fixture.service.loaded() == 0).await;
    fixture.service.wait_retiring(&id).await;
    // A dangling run_start would trigger recovery if URL resolution opened this session.
    let rows = jsonl(&path);
    let mut run = rows
        .iter()
        .find(|row| row["type"] == "run_start")
        .unwrap()
        .clone();
    run["id"] = json!("unresolved-run-entry");
    run["parentId"] = rows.last().unwrap()["id"].clone();
    run["runId"] = json!("unresolved-run");
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(file, "{}", run).unwrap();
    drop(file);
    let before = std::fs::read(&path).unwrap();
    let hashes = targets
        .iter()
        .map(|target| match target {
            ChatLinkTarget::File { path, .. } => sha256(Path::new(path)),
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    let requests = fixture.api.requests().len();
    for (index, url) in urls.iter().enumerate() {
        let target = fixture
            .service
            .resolve_link(&fixture.folder, &id, (*url).into())
            .await
            .unwrap();
        assert_eq!(target, targets[index]);
        let ChatLinkTarget::File {
            path: target_path, ..
        } = target
        else {
            unreachable!()
        };
        assert_eq!(sha256(Path::new(&target_path)), hashes[index]);
        assert_eq!(fixture.service.loaded(), 0);
        assert_eq!(fixture.api.requests().len(), requests);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn new_and_reopened_sessions_discover_skill_argument_hints() {
    if isolated_reopen_test("new_and_reopened_sessions_discover_skill_argument_hints") {
        return;
    }
    let fixture = Fixture::new(replies()).await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "catalog").await;
    fixture.prompt(&id, "persist", &events, 1).await;
    write_skill(&fixture.folder, "zz-new", "<name> [tone]", "Greet $1.");
    let loaded = fixture.attach(&id, "loaded-catalog").await;
    let catalog = |events: &Events| {
        let events = events.lock();
        let Some(ChatEvent::Snapshot { state }) = events.first() else {
            panic!("snapshot")
        };
        state.commands.clone()
    };
    assert!(!catalog(&loaded)
        .iter()
        .any(|command| command.name == "zz-new"));
    let fresh = fixture.create().await;
    let fresh_events = fixture.attach(&fresh, "fresh-catalog").await;
    fixture.service.detach(&id, "catalog");
    fixture.service.detach(&id, "loaded-catalog");
    wait_until(|| !fixture.service.sessions.lock().contains_key(&id)).await;
    let reopened = fixture.attach(&id, "reopened-catalog").await;
    for events in [&fresh_events, &reopened] {
        let commands = catalog(events);
        let skill = commands
            .iter()
            .find(|command| command.name == "zz-new")
            .expect("new skill");
        assert!(
            matches!(&skill.kind, ChatCommandKind::Skill { argument_hint }
            if argument_hint.as_deref() == Some("<name> [tone]"))
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_thinking_preserves_axo_error_without_appending() {
    let fixture = Fixture::new(replies()).await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "invalid").await;
    fixture.prompt(&id, "persist", &events, 1).await;
    let path = session_path(&fixture, &id);
    let before = std::fs::read(&path).unwrap();
    let expected = Command::decode("thinking", json!({"level": "bogus"}))
        .unwrap_err()
        .to_string();
    let error = fixture
        .service
        .run_command(
            &fixture.folder,
            &id,
            "thinking".into(),
            json!({"level": "bogus"}),
        )
        .await
        .unwrap_err();
    let crate::errors::ApiError::InvalidArgument(message) = error else {
        panic!("invalid thinking must be InvalidArgument");
    };
    assert_eq!(message, expected);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn maintenance_command_returns_once_started() {
    let settings = json!({"compaction": {"enabled": true, "keepRecentTokens": 0}});
    let fixture = Fixture::with_settings(
        vec![
            ScriptedTurn::Reply(reply_text(&"history ".repeat(1000))),
            ScriptedTurn::HangUntilCancel,
        ],
        settings,
    )
    .await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;
    fixture.prompt(&id, "remember history", &events, 1).await;
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        fixture
            .service
            .run_command(&fixture.folder, &id, "compact".into(), json!({})),
    )
    .await;
    assert!(
        matches!(result, Ok(Ok(()))),
        "compact must return once maintenance starts: {result:?}"
    );
    wait_until(|| {
        events.lock().iter().rev().find_map(|event| match event {
            ChatEvent::Live { live } => Some(live.maintenance.is_some()),
            _ => None,
        }) == Some(true)
    })
    .await;
    fixture.service.abort(&fixture.folder, &id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn unwritten_sessions_are_reclaimed_after_grace() {
    let fixture = Fixture::new(vec![]).await;
    let orphan = fixture.create().await;
    // Each grace runs from that session's own unload check; asserting both together would
    // also require the second create and attach to finish within the orphan's grace.
    assert!(fixture.service.sessions.lock().contains_key(&orphan));
    let id = fixture.create().await;
    fixture.attach(&id, "a").await;
    fixture.service.detach(&id, "a");
    assert!(fixture.service.sessions.lock().contains_key(&id));
    fixture.attach(&id, "b").await;
    fixture.service.detach(&id, "b");
    wait_until(|| fixture.service.loaded() == 0).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stale_stream_detach_keeps_replacement_subscriber() {
    let fixture = Fixture::new(vec![ScriptedTurn::Reply(reply_text("one"))]).await;
    let id = fixture.create().await;
    let (_, first) = collector();
    let (second_events, second) = collector();
    for sink in [&first, &second] {
        fixture
            .service
            .attach(&fixture.folder, &id, "same".into(), sink.clone())
            .await
            .unwrap();
    }
    // The reconnected stream reused the id; the stale stream's teardown must not remove it.
    fixture.service.detach_sink(&id, "same", &first);
    fixture.prompt(&id, "hello", &second_events, 1).await;
    fixture.service.detach_sink(&id, "same", &second);
    wait_until(|| fixture.service.loaded() == 0).await;
}

#[test]
fn settings_mirror_defaults_match_axo() {
    let mirrored = serde_json::to_value(axome_protocol::axo_settings::AxoSettings::default())
        .expect("Axome settings serialize");
    let settings = serde_json::from_value::<axo::Settings>(mirrored)
        .expect("Axome settings deserialize as axo settings");
    assert_eq!(settings, axo::Settings::default());
}

#[tokio::test(flavor = "multi_thread")]
async fn active_job_keeps_detached_session_loaded_until_completion() {
    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_tool_calls(&[(
            "job",
            "bash",
            json!({"command": "while [ ! -f release-job ]; do sleep 0.05; done; echo done", "async": true}),
        )])),
        ScriptedTurn::Reply(reply_text("started")),
        ScriptedTurn::Reply(reply_text("finished")),
    ]).await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;
    fixture.prompt(&id, "start a job", &events, 1).await;
    {
        let sessions = fixture.service.sessions.lock();
        let jobs = sessions[&id].session.jobs();
        assert_eq!(jobs.len(), 1);
        assert!(matches!(jobs[0].status, axo::JobStatus::Running));
    }
    fixture.service.detach(&id, "a");
    assert_eq!(
        fixture.service.loaded(),
        1,
        "detach must retain an active job"
    );
    std::fs::write(fixture.folder.join("release-job"), "").unwrap();
    wait_until(|| fixture.service.loaded() == 0).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn close_shuts_down_and_notifies_other_attachment_then_reopens() {
    if isolated_reopen_test("close_shuts_down_and_notifies_other_attachment_then_reopens") {
        return;
    }
    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_tool_calls(&[(
            "job",
            "bash",
            json!({"command": "sleep 300", "async": true}),
        )])),
        ScriptedTurn::Reply(reply_text("started")),
    ])
    .await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;
    let other = fixture.attach(&id, "b").await;
    fixture.prompt(&id, "start a job", &events, 1).await;
    let session = fixture.service.sessions.lock()[&id].session.clone();
    assert_eq!(session.jobs().len(), 1);
    fixture.service.close(&id).await.unwrap();
    assert_eq!(fixture.service.loaded(), 0);
    assert!(other
        .lock()
        .iter()
        .any(|event| matches!(event, ChatEvent::Closed { .. })));
    assert!(matches!(
        session.jobs()[0].kind,
        axo::JobKind::Command {
            exit: Some(axo::CommandExit::Interrupted),
            ..
        }
    ));
    drop(session);
    let reopened = fixture.attach(&id, "reopened").await;
    assert!(matches!(
        reopened.lock().first(),
        Some(ChatEvent::Snapshot { state }) if state.session_id == id && !state.live.running
    ));
    assert_eq!(fixture.service.loaded(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn list_history_excludes_spawned_child() {
    let fixture = Fixture::new(vec![ScriptedTurn::Reply(reply_text("noted"))]).await;
    fixture
        .api
        .lane("m2", vec![ScriptedTurn::Reply(reply_text("child done"))]);
    let id = fixture.create().await;
    let _events = fixture.attach(&id, "a").await;
    let session = fixture.service.sessions.lock()[&id].session.clone();
    let child = session
        .spawn(axo::SpawnRequest {
            task: "check".into(),
            agent: Some("scout".into()),
            model: Some("test/m2".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let live = child.session.attach(None).await.live;
            if !live.running && live.maintenance.is_none() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let all = fixture
        .service
        .axo()
        .await
        .unwrap()
        .list_sessions(&fixture.folder)
        .await
        .unwrap();
    assert!(all
        .iter()
        .any(|info| info.id == child.session.id() && info.spawn.is_some()));
    let service = fixture.service.clone();
    let folder = fixture.folder.clone();
    let history = tokio::task::spawn_blocking(move || service.list_history(&folder))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        history
            .rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        [id.as_str()]
    );
    assert_eq!(history.rows[0].children.len(), 1);
    let nested = &history.rows[0].children[0];
    assert_eq!(nested.session_id, child.session.id());
    assert_eq!(nested.agent.as_deref(), Some("scout"));
    session.shutdown().await;
}

#[test]
fn chat_close_succeeds_after_folder_is_deleted() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let dir = std::env::temp_dir().join(format!("axome-close-missing-{}", uuid::Uuid::new_v4()));
    let folder = dir.join("work");
    std::fs::create_dir_all(&folder).unwrap();
    let dirs = crate::HostDirs::under(&dir);
    std::fs::create_dir_all(&dirs.config).unwrap();
    std::fs::write(
        dirs.models_path(),
        json!({"providers": {"test": {
            "api": "openai-completions",
            "baseUrl": "http://127.0.0.1:9/v1",
            "models": [{"id": "m", "input": ["text"]}]
        }}})
        .to_string(),
    )
    .unwrap();
    let host = crate::Host::new(
        crate::HostOptions::new(dirs, "test.db"),
        Arc::new(|_| Ok(())),
    )
    .unwrap();
    {
        let _entered = runtime.enter();
        host.axo.bind_runtime();
    }
    let folder_str = folder.to_string_lossy().into_owned();
    let session_id = host
        .chat_create(
            folder_str.clone(),
            "test/m".into(),
            ChatThinking::Off,
            ChatServiceTier::Standard,
        )
        .unwrap();
    assert_eq!(host.axo.loaded(), 1);
    std::fs::remove_dir_all(&folder).unwrap();
    assert!(!folder.exists());
    host.chat_close(folder_str, session_id).unwrap();
    assert_eq!(host.axo.loaded(), 0);
    host.axo.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn engine_reloads_in_place() {
    let dir = std::env::temp_dir().join(format!("axome-reload-{}", uuid::Uuid::new_v4()));
    let dirs = crate::HostDirs::under(&dir);
    std::fs::create_dir_all(&dirs.config).unwrap();
    std::fs::create_dir_all(&dirs.data).unwrap();
    std::fs::write(
        dirs.models_path(),
        json!({"providers": {"test": {
            "api": "openai-completions",
            "baseUrl": "http://127.0.0.1:9/v1",
            "models": [{"id": "m", "input": ["text"]}]
        }}})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dirs.settings_path(),
        json!({"axo": {"extendedContext": true}}).to_string(),
    )
    .unwrap();
    let db = Arc::new(DatabaseService::new(dirs.data.join("test.db")).unwrap());
    let service = AxoService::new(
        db,
        dirs.clone(),
        axome_protocol::server_config::Supervision::ProcessGroup,
        None,
        Arc::new(|_| Ok(())),
    );
    service.bind_runtime();
    let selectors = |engine: &Engine| {
        engine
            .models
            .all()
            .iter()
            .map(|model| format!("{}/{}", model.provider, model.id))
            .collect::<Vec<_>>()
    };

    let first = service.engine().await.unwrap();
    assert!(first.models.find("test/m").is_some());

    std::fs::write(
        dirs.settings_path(),
        json!({"axo": {"extendedContext": false}}).to_string(),
    )
    .unwrap();
    let second = service.engine().await.unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    let before = selectors(&second);

    // An invalid file keeps the last good registry and reports the error.
    std::fs::write(dirs.models_path(), "{").unwrap();
    let third = service.engine().await.unwrap();
    assert!(Arc::ptr_eq(&first, &third));
    assert!(service
        .providers(None)
        .await
        .unwrap()
        .models_file
        .error
        .is_some());
    assert_eq!(selectors(&third), before);

    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn job_output_preserves_stream_order_and_split_utf8() {
    use axo::{OutputChunk, OutputStream};
    let mut pending = None;
    let chunks = live::decode_output(
        vec![
            OutputChunk {
                stream: OutputStream::Stdout,
                bytes: vec![0xa9, b'a', 0xc3],
            },
            OutputChunk {
                stream: OutputStream::Stdout,
                bytes: vec![0xa9],
            },
            OutputChunk {
                stream: OutputStream::Stderr,
                bytes: b"error".to_vec(),
            },
            OutputChunk {
                stream: OutputStream::Stdout,
                bytes: vec![0xe2, 0x82],
            },
        ],
        &mut pending,
        true,
        false,
    );
    assert_eq!(
        chunks,
        vec![
            ChatOutputChunk {
                stream: ChatOutputStream::Stdout,
                text: "aé".into()
            },
            ChatOutputChunk {
                stream: ChatOutputStream::Stderr,
                text: "error".into()
            },
        ]
    );
    let chunks = live::decode_output(
        vec![OutputChunk {
            stream: OutputStream::Stdout,
            bytes: vec![0xac],
        }],
        &mut pending,
        false,
        true,
    );
    assert_eq!(
        chunks,
        vec![ChatOutputChunk {
            stream: ChatOutputStream::Stdout,
            text: "€".into(),
        }]
    );
    assert!(pending.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn background_watch_job_live_and_unloaded_and_kill_job() {
    if isolated_reopen_test("background_watch_job_live_and_unloaded_and_kill_job") {
        return;
    }
    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_tool_calls(&[(
            "call_bg",
            "bash",
            json!({"command": "touch cmd-started; while [ ! -f bg-release ]; do sleep 0.05; done; echo after-release; echo error-output >&2"}),
        )])),
        ScriptedTurn::Reply(reply_text("promoted")),
        ScriptedTurn::Reply(reply_text("job finished")),
        ScriptedTurn::Reply(reply_tool_calls(&[(
            "call_kill",
            "bash",
            json!({"command": "sleep 300", "async": true}),
        )])),
        ScriptedTurn::Reply(reply_text("started second")),
        ScriptedTurn::Reply(reply_text("second killed")),
    ])
    .await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;

    fixture
        .service
        .prompt(&fixture.folder, &id, "req-bg".into(), text_parts("run bg"))
        .await
        .unwrap();
    wait_until(|| fixture.folder.join("cmd-started").exists()).await;

    let promoted = fixture
        .service
        .background(&fixture.folder, &id, "call_bg".into())
        .await
        .unwrap();
    assert!(promoted, "foreground command should promote to background");
    wait_settled(&events, 1).await;

    let live_out = fixture
        .service
        .watch_job(&fixture.folder, &id, "a".into(), Some("bg_1".into()))
        .await
        .unwrap()
        .expect("live command job output");
    assert!(!live_out.done);

    std::fs::write(fixture.folder.join("bg-release"), "").unwrap();
    wait_until(|| {
        events.lock().iter().any(|event| {
            matches!(
                event,
                ChatEvent::JobOutput {
                    job_id,
                    output: axome_protocol::chat::ChatJobOutput { done: true, .. },
                    ..
                } if job_id == "bg_1"
            )
        })
    })
    .await;
    let streamed: Vec<ChatOutputChunk> = events
        .lock()
        .iter()
        .filter_map(|event| match event {
            ChatEvent::JobOutput { job_id, output, .. } if job_id == "bg_1" => {
                Some(output.chunks.clone())
            }
            _ => None,
        })
        .flatten()
        .collect();
    for (stream, text) in [
        (ChatOutputStream::Stdout, "after-release"),
        (ChatOutputStream::Stderr, "error-output"),
    ] {
        assert!(
            live_out
                .chunks
                .iter()
                .chain(&streamed)
                .any(|chunk| chunk.stream == stream && chunk.text.contains(text)),
            "missing {stream:?} output {text:?}"
        );
    }
    assert!(!live_out.capture_failed);
    wait_settled(&events, 2).await;

    fixture.service.detach(&id, "a");
    wait_until(|| fixture.service.loaded() == 0).await;

    let unloaded_out = fixture
        .service
        .watch_job(&fixture.folder, &id, "other".into(), Some("bg_1".into()))
        .await
        .unwrap()
        .expect("unloaded artifact fallback output");
    assert!(unloaded_out.done);
    assert!(!unloaded_out.capture_failed);
    for (stream, text) in [
        (ChatOutputStream::Stdout, "after-release"),
        (ChatOutputStream::Stderr, "error-output"),
    ] {
        assert!(unloaded_out
            .chunks
            .iter()
            .any(|chunk| chunk.stream == stream && chunk.text.contains(text)));
    }
    assert_eq!(
        fixture.service.loaded(),
        0,
        "unloaded watch_job must not load the session"
    );

    let events_b = fixture.attach(&id, "b").await;
    fixture.prompt(&id, "start killable", &events_b, 1).await;
    let running_job_id = events_b
        .lock()
        .iter()
        .rev()
        .find_map(|event| match event {
            ChatEvent::Live { live } => live.jobs.iter().find_map(|job| {
                matches!(job.kind, ChatJobKind::Command { .. })
                    .then_some(job)
                    .filter(|j| j.status == ChatJobStatus::Running)
                    .map(|j| j.id.clone())
            }),
            ChatEvent::Snapshot { state } => state.live.jobs.iter().find_map(|job| {
                matches!(job.kind, ChatJobKind::Command { .. })
                    .then_some(job)
                    .filter(|j| j.status == ChatJobStatus::Running)
                    .map(|j| j.id.clone())
            }),
            _ => None,
        })
        .expect("running command job");
    let killed = fixture
        .service
        .kill_job(&fixture.folder, &id, running_job_id.clone())
        .await
        .unwrap();
    assert!(killed);
    wait_until(|| {
        events_b.lock().iter().rev().any(|event| {
            matches!(
                event,
                ChatEvent::Live { live }
                    if live.jobs.iter().any(|j| {
                        j.id == running_job_id
                            && matches!(j.status, ChatJobStatus::Failed | ChatJobStatus::Finished | ChatJobStatus::Interrupted)
                    })
            )
        })
    })
    .await;
    wait_settled(&events_b, 2).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn summaries_follow_activity() {
    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_tool_calls(&[(
            "call_1",
            "bash",
            json!({"command": "while [ ! -f sum-release ]; do sleep 0.05; done; echo ok"}),
        )])),
        ScriptedTurn::Reply(reply_text("done")),
    ])
    .await;
    let host_events = &fixture.host_events;

    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;
    {
        let guard = events.lock();
        let Some(ChatEvent::Snapshot { state }) = guard.first() else {
            panic!("expected Snapshot event");
        };
        assert!(!state.read_only);
    }

    fixture
        .service
        .prompt(&fixture.folder, &id, "req-sum".into(), text_parts("work"))
        .await
        .unwrap();
    wait_until(|| {
        host_events.lock().iter().any(|event| {
            matches!(
                event,
                HostEvent::ChatSummariesChanged(changed)
                    if changed.summaries.iter().any(|s| s.session_id == id && s.running)
            )
        })
    })
    .await;
    assert!(fixture.service.active_folder(&id).is_some());

    std::fs::write(fixture.folder.join("sum-release"), "").unwrap();
    wait_settled(&events, 1).await;
    wait_until(|| {
        host_events
            .lock()
            .iter()
            .rev()
            .find_map(|event| match event {
                HostEvent::ChatSummariesChanged(changed) => Some(changed),
                _ => None,
            })
            .is_some_and(|changed| {
                changed
                    .summaries
                    .iter()
                    .any(|s| s.session_id == id && !s.running)
            })
    })
    .await;
    assert_eq!(fixture.service.active_folder(&id), None);

    fixture.service.detach(&id, "a");
    wait_until(|| fixture.service.loaded() == 0).await;
    wait_until(|| {
        host_events
            .lock()
            .iter()
            .rev()
            .find_map(|event| match event {
                HostEvent::ChatSummariesChanged(changed) => Some(changed),
                _ => None,
            })
            .is_some_and(|changed| changed.summaries.is_empty())
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn bundled_definitions_load_and_project_agents_override() {
    let fixture = Fixture::new(vec![]).await;
    let agents_dir = fixture.folder.join(".axome").join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::write(
        agents_dir.join("scout.md"),
        "---\nname: scout\ndescription: Custom project scout\nrunAs: subagent\nmodel: \"@work\"\n---\nOverride body.\n",
    )
    .unwrap();

    let engine = fixture.service.engine().await.unwrap();
    let catalog = super::bundled::catalog(
        &engine.axo,
        &fixture.folder,
        super::project_access(&fixture.folder),
    )
    .unwrap();
    let catalog_scout = catalog
        .agents
        .iter()
        .find_map(|def| match def {
            axo::context::agents::AgentDef::Agent(spec) if spec.name == "scout" => Some(spec),
            _ => None,
        })
        .expect("catalog scout");
    assert_eq!(catalog_scout.description, "Custom project scout");
    assert_eq!(catalog_scout.model.as_deref(), Some("@work"));

    let catalog_task = catalog
        .agents
        .iter()
        .find_map(|def| match def {
            axo::context::agents::AgentDef::Agent(spec) if spec.name == "task" => Some(spec),
            _ => None,
        })
        .expect("catalog task");
    assert_eq!(catalog_task.run_as, axo::RunAs::Subagent);
    assert_eq!(catalog_task.model.as_deref(), Some("@work"));

    // Programmatic definitions that fail validation are dropped with a diagnostic.
    let keywords: Vec<_> = catalog.keywords.iter().map(|k| k.name.as_str()).collect();
    assert_eq!(keywords, ["orchestrate"]);
    let modes: Vec<_> = catalog.modes.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(modes, ["plan", "plan:roadmap", "plan:step", "research"]);
    for mode in &catalog.modes {
        assert!(mode.disable_model_invocation, "{}", mode.name);
        assert_eq!(mode.model.as_deref(), Some("@reason"), "{}", mode.name);
        assert_eq!(mode.bash, axo::BashAccess::ReadOnly, "{}", mode.name);
        let plan = mode.extra.get("axome").and_then(|axome| axome.get("plan"));
        let save_as = mode.result.as_ref().map(|result| {
            (
                result.review,
                &result.schema["properties"]["saveAs"]["pattern"],
            )
        });
        if mode.name == "research" {
            assert_eq!(plan, None);
            assert_eq!(save_as, None);
            assert_eq!(
                mode.writes.as_deref(),
                Some(&["local://".to_owned(), ".axome/research/".to_owned()][..])
            );
        } else {
            assert_eq!(plan, Some(&json!(true)), "{}", mode.name);
            assert_eq!(mode.writes.as_deref(), Some(&["local://".to_owned()][..]));
            let pattern = match mode.name.as_str() {
                "plan" => r"^[a-z0-9][a-z0-9-]*\.md$",
                "plan:roadmap" => r"^[a-z0-9][a-z0-9-]*/roadmap\.md$",
                _ => r"^[a-z0-9][a-z0-9-]*/[0-9]{2}-[a-z0-9][a-z0-9-]*\.md$",
            };
            assert_eq!(save_as, Some((true, &json!(pattern))), "{}", mode.name);
        }
    }
}

#[test]
fn keyword_matches_count_utf16_units_after_multibyte_text() {
    // "é" is 2 bytes / 1 unit, U+1F33F 4 bytes / 2 units: bytes 7..18 are units 4..15, and
    // the second match, after one more "é", bytes 22..33 are units 18..29.
    let text = "é\u{1F33F} orchestrate é orchestrate";
    let found = |range| axo::KeywordMatch {
        name: "orchestrate".into(),
        range,
    };
    let matches = super::utf16_matches(text, vec![found(7..18), found(22..33)]);
    let expected = |start, end| axome_protocol::chat::ChatKeywordMatch {
        name: "orchestrate".into(),
        start,
        end,
    };
    assert_eq!(matches, [expected(4, 15), expected(18, 29)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn child_readonly_resume_pinning_and_close_lifecycle() {
    if isolated_reopen_test("child_readonly_resume_pinning_and_close_lifecycle") {
        return;
    }
    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_text("root reply")),
        ScriptedTurn::Reply(reply_text("wake reply")),
    ])
    .await;
    fixture
        .api
        .lane("m2", vec![ScriptedTurn::Reply(reply_text("child done"))]);

    let root_id = fixture.create().await;
    let root_events = fixture.attach(&root_id, "a").await;
    fixture.prompt(&root_id, "hello", &root_events, 1).await;

    let root_session = fixture.service.sessions.lock()[&root_id].session.clone();
    let child = root_session
        .spawn(axo::SpawnRequest {
            task: "check".into(),
            model: Some("test/m2".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let child_id = child.session.id().to_owned();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let live = child.session.attach(None).await.live;
            if !live.running && live.maintenance.is_none() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(child);
    drop(root_session);
    wait_settled(&root_events, 2).await;
    fixture.service.detach(&root_id, "a");
    wait_until(|| fixture.service.loaded() == 0).await;
    fixture.service.wait_retiring(&root_id).await;
    let child_events = fixture.attach(&child_id, "b").await;
    {
        let events = child_events.lock();
        let snapshot = events
            .iter()
            .find_map(|event| match event {
                ChatEvent::Snapshot { state } => Some(state),
                _ => None,
            })
            .expect("read-only child snapshot");
        assert_eq!(snapshot.session_id, child_id);
        assert!(snapshot.read_only);
        assert!(snapshot.is_child);
    }
    assert_eq!(fixture.service.loaded(), 0);
    assert!(fixture
        .service
        .prompt(
            &fixture.folder,
            &child_id,
            "req-ro".into(),
            text_parts("nope"),
        )
        .await
        .is_err());

    fixture
        .service
        .resume(&fixture.folder, &child_id)
        .await
        .unwrap();
    assert_eq!(fixture.service.loaded(), 2);
    {
        let events = child_events.lock();
        let live_snapshot = events
            .iter()
            .rev()
            .find_map(|event| match event {
                ChatEvent::Snapshot { state } => Some(state),
                _ => None,
            })
            .expect("live child snapshot after resume");
        assert!(!live_snapshot.read_only);
        assert!(live_snapshot.is_child);
    }
    let summaries = fixture.service.live_summaries(&fixture.folder);
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].session_id, root_id);

    let _root_events2 = fixture.attach(&root_id, "a2").await;
    fixture.service.detach(&root_id, "a2");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(fixture.service.loaded(), 2);

    let root_events3 = fixture.attach(&root_id, "a3").await;
    fixture.service.close(&child_id).await.unwrap();
    assert!(child_events
        .lock()
        .iter()
        .any(|event| matches!(event, ChatEvent::Closed { .. })));
    assert_eq!(fixture.service.loaded(), 1);
    assert!(!root_events3
        .lock()
        .iter()
        .any(|event| matches!(event, ChatEvent::Closed { .. })));

    let child_events2 = fixture.attach(&child_id, "b2").await;
    {
        let events = child_events2.lock();
        let snapshot = events
            .iter()
            .find_map(|event| match event {
                ChatEvent::Snapshot { state } => Some(state),
                _ => None,
            })
            .expect("reattached child snapshot");
        assert!(!snapshot.read_only);
        assert!(snapshot.is_child);
    }
    assert_eq!(fixture.service.loaded(), 2);

    fixture.service.close(&root_id).await.unwrap();
    assert!(root_events3
        .lock()
        .iter()
        .any(|event| matches!(event, ChatEvent::Closed { .. })));
    assert!(child_events2
        .lock()
        .iter()
        .any(|event| matches!(event, ChatEvent::Closed { .. })));
    assert_eq!(fixture.service.loaded(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn turn_snapshots_and_squash_work_across_live_and_closed_sessions() {
    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_tool_calls(&[("r1", "read", json!({"path": "notes.txt"}))])),
        ScriptedTurn::Reply(reply_tool_calls(&[(
            "e1",
            "edit",
            json!({
                "path": "notes.txt",
                "edits": [{"oldText": "greeting.", "newText": "updated greeting."}]
            }),
        )])),
        ScriptedTurn::Reply(reply_tool_calls(&[
            (
                "e2",
                "edit",
                json!({
                    "path": "notes.txt",
                    "edits": [{"oldText": "updated greeting.", "newText": "final greeting.\nextra line"}]
                }),
            ),
            (
                "w1",
                "write",
                json!({
                    "path": "local://scratch.md",
                    "content": "# Scratch\nLine 2\n"
                }),
            ),
        ])),
        ScriptedTurn::Reply(reply_text("done")),
    ])
    .await;

    let session_id = fixture.create().await;
    let events = fixture.attach(&session_id, "a1").await;
    fixture
        .prompt(&session_id, "update files", &events, 1)
        .await;

    let changes: Vec<Value> = {
        let log = events.lock();
        log.iter()
            .filter_map(|event| match event {
                ChatEvent::Entry {
                    entry:
                        ChatEntry {
                            body:
                                ChatEntryBody::ToolResult {
                                    details: Some(details),
                                    is_error: false,
                                    ..
                                },
                            ..
                        },
                } => details.get("files").and_then(Value::as_array),
                _ => None,
            })
            .flatten()
            .filter_map(|record| {
                record
                    .get("change")
                    .filter(|change| !change.is_null())
                    .cloned()
            })
            .collect()
    };
    assert_eq!(changes.len(), 3);

    // Live session squash & snapshot diff read
    let squashed_live = fixture
        .service
        .squash_changes(&fixture.folder, &session_id, changes.clone())
        .await
        .unwrap();
    assert_eq!(squashed_live.len(), 2);
    let notes = &squashed_live[0];
    assert_eq!(notes.mutations, 2);
    assert!(!notes.created);
    assert!(!notes.discontinuous);
    assert_eq!(
        (notes.additions, notes.deletions),
        (Some(2), Some(1)),
        "re-diffed multi-mutation diff"
    );
    let notes_content = fixture
        .service
        .turn_diff_file(
            &fixture.folder,
            &session_id,
            notes.before_sha.clone(),
            notes.after_sha.clone(),
        )
        .await
        .unwrap();
    assert!(!notes_content.binary);
    assert_eq!(
        notes_content.old_content.as_deref(),
        Some("Remember the greeting.\n")
    );
    assert_eq!(
        notes_content.new_content.as_deref(),
        Some("Remember the final greeting.\nextra line\n")
    );

    let local = &squashed_live[1];
    assert_eq!(local.url.as_deref(), Some("local://scratch.md"));
    assert!(local.created);
    assert_eq!(local.before_sha, None);
    let local_content = fixture
        .service
        .turn_diff_file(&fixture.folder, &session_id, None, local.after_sha.clone())
        .await
        .unwrap();
    assert_eq!(local_content.old_content, None);
    assert_eq!(
        local_content.new_content.as_deref(),
        Some("# Scratch\nLine 2\n")
    );

    // Closed session squash & snapshot diff read (lock-free Axo path)
    fixture.service.close(&session_id).await.unwrap();
    let squashed_closed = fixture
        .service
        .squash_changes(&fixture.folder, &session_id, changes)
        .await
        .unwrap();
    assert_eq!(squashed_closed, squashed_live);
    let notes_closed = fixture
        .service
        .turn_diff_file(
            &fixture.folder,
            &session_id,
            notes.before_sha.clone(),
            notes.after_sha.clone(),
        )
        .await
        .unwrap();
    assert_eq!(notes_closed.old_content, notes_content.old_content);
    assert_eq!(notes_closed.new_content, notes_content.new_content);
}

#[tokio::test(flavor = "multi_thread")]
async fn queue_orders_steers_first_withdraws_and_send_now_releases_held_input() {
    let fixture = Fixture::new(vec![
        ScriptedTurn::HangUntilCancel,
        ScriptedTurn::Reply(reply_text("delivered after send_now")),
    ])
    .await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;

    fixture
        .service
        .prompt(
            &fixture.folder,
            &id,
            "p1".into(),
            vec![ChatPart::Text {
                text: "start".into(),
            }],
        )
        .await
        .unwrap();
    wait_until(|| {
        events
            .lock()
            .iter()
            .any(|event| matches!(event, ChatEvent::Delta { .. }))
    })
    .await;

    // Queue a follow-up first, then a steer: live.queue keeps steers before follow-ups.
    fixture
        .service
        .follow_up(
            &fixture.folder,
            &id,
            vec![ChatPart::Text {
                text: "later".into(),
            }],
        )
        .await
        .unwrap();
    fixture
        .service
        .steer(
            &fixture.folder,
            &id,
            vec![ChatPart::Text {
                text: "sooner".into(),
            }],
        )
        .await
        .unwrap();

    let latest_queue = || {
        events
            .lock()
            .iter()
            .rev()
            .find_map(|event| match event {
                ChatEvent::Live { live } => Some(live.queue.clone()),
                _ => None,
            })
            .unwrap_or_default()
    };
    wait_until(|| latest_queue().len() == 2).await;
    let queue = latest_queue();
    assert_eq!(queue[0].kind, axome_protocol::chat::ChatQueueKind::Steer);
    assert_eq!(
        queue[0].parts,
        vec![ChatPart::Text {
            text: "sooner".into()
        }]
    );
    assert_eq!(queue[1].kind, axome_protocol::chat::ChatQueueKind::FollowUp);
    assert_eq!(
        queue[1].parts,
        vec![ChatPart::Text {
            text: "later".into()
        }]
    );

    // Withdraw the follow-up; re-withdrawing fails with NotFound.
    let withdrawn = fixture
        .service
        .withdraw(&fixture.folder, &id, queue[1].submission_id.clone())
        .await
        .unwrap();
    assert_eq!(
        withdrawn,
        vec![ChatPart::Text {
            text: "later".into()
        }]
    );
    assert!(matches!(
        fixture
            .service
            .withdraw(&fixture.folder, &id, queue[1].submission_id.clone())
            .await,
        Err(crate::errors::ApiError::NotFound(_))
    ));
    wait_until(|| latest_queue().len() == 1).await;

    // Interrupt and send now aborts the active run and delivers the held steer immediately.
    fixture
        .service
        .send_now(&fixture.folder, &id)
        .await
        .unwrap();
    wait_settled(&events, 1).await;
    assert!(latest_queue().is_empty());
    let texts: Vec<_> = events
        .lock()
        .iter()
        .filter_map(|event| match event {
            ChatEvent::Entry {
                entry:
                    ChatEntry {
                        body: ChatEntryBody::Assistant { blocks, .. },
                        ..
                    },
            } => blocks.iter().find_map(|b| match b {
                axome_protocol::chat::ChatBlock::Text { text } => Some(text.clone()),
                _ => None,
            }),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|text| text == "delivered after send_now"));
}

#[test]
fn diff_content_marks_binary_sides() {
    let text = super::diff_content(Some(b"a".to_vec()), b"b".to_vec());
    assert_eq!(text.old_content.as_deref(), Some("a"));
    assert_eq!(text.new_content.as_deref(), Some("b"));
    assert!(!text.binary);

    let created = super::diff_content(None, b"b".to_vec());
    assert_eq!(created.old_content, None);
    assert_eq!(created.new_content.as_deref(), Some("b"));
    assert!(!created.binary);

    for content in [
        super::diff_content(Some(vec![0]), b"b".to_vec()),
        super::diff_content(Some(b"a".to_vec()), vec![0xff]),
    ] {
        assert!(content.binary);
        assert_eq!(content.old_content, None);
        assert_eq!(content.new_content, None);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn search_docs_indexes_llms_txt_and_doc_scheme_resolves_live_and_closed() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..4 {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 2048];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/");
                let (status, ctype, body) = match path {
                    "/llms.txt" => (
                        "200 OK",
                        "text/plain",
                        "# Docs\n- [Client](/guide.md): HTTP client guide\n",
                    ),
                    "/guide.md" => (
                        "200 OK",
                        "text/markdown",
                        "# Client {#client}\n\nUse `ClientBuilder::timeout` to set timeouts.\n",
                    ),
                    _ => ("404 Not Found", "text/plain", "missing"),
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });

    let llms_url = format!("http://{addr}/llms.txt");
    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_tool_calls(&[(
            "docs_1",
            "search_docs",
            json!({"query": "ClientBuilder timeout", "source": llms_url}),
        )])),
        ScriptedTurn::Reply(reply_text("found timeout docs")),
    ])
    .await;
    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;
    fixture
        .prompt(&id, "look up ClientBuilder timeout", &events, 1)
        .await;

    let doc_url = entries(&events.lock())
        .into_iter()
        .find_map(|entry| match &entry.body {
            axome_protocol::chat::ChatEntryBody::ToolResult {
                tool_name, details, ..
            } if tool_name == "search_docs" => details
                .as_ref()
                .and_then(|d| d["hits"][0]["docUrl"].as_str())
                .map(str::to_owned),
            _ => None,
        })
        .expect("search_docs tool result with docUrl");
    assert!(doc_url.starts_with("doc://"), "{doc_url}");

    // Live resolve_link works for doc://
    let live_target = fixture
        .service
        .resolve_link(&fixture.folder, &id, doc_url.clone())
        .await
        .unwrap();
    assert!(matches!(
        live_target,
        ChatLinkTarget::File { is_dir: false, .. }
    ));

    // Closed session resolve_link also resolves doc://
    fixture.service.detach(&id, "a");
    wait_until(|| fixture.service.loaded() == 0).await;
    let closed_target = fixture
        .service
        .resolve_link(&fixture.folder, &id, doc_url)
        .await
        .unwrap();
    assert!(matches!(
        closed_target,
        ChatLinkTarget::File { is_dir: false, .. }
    ));

    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn web_search_tool_uses_configured_searxng_backend() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 2048];
            let _ = sock.read(&mut buf).await;
            let body = json!({
                "results": [{
                    "title": "Axome Docs",
                    "url": "https://example.com/axome",
                    "content": "SearXNG mock snippet"
                }]
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        }
    });

    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_tool_calls(&[(
            "ws_1",
            "web_search",
            json!({"query": "axome docs"}),
        )])),
        ScriptedTurn::Reply(reply_text("done")),
    ])
    .await;
    std::fs::create_dir_all(&fixture.service.dirs.config).unwrap();
    std::fs::write(
        fixture.service.dirs.settings_path(),
        json!({
            "axo": {
                "webSearch": {
                    "enabled": true,
                    "providers": ["searxng"],
                    "searxngUrl": format!("http://{addr}")
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    let id = fixture.create().await;
    let events = fixture.attach(&id, "a").await;
    fixture.prompt(&id, "search axome docs", &events, 1).await;

    let provider = entries(&events.lock())
        .into_iter()
        .find_map(|entry| match &entry.body {
            axome_protocol::chat::ChatEntryBody::ToolResult {
                tool_name,
                is_error,
                details,
                ..
            } if tool_name == "web_search" => {
                assert!(!is_error);
                details
                    .as_ref()
                    .and_then(|d| d["provider"].as_str())
                    .map(str::to_owned)
            }
            _ => None,
        })
        .expect("web_search tool result");
    assert_eq!(provider, "searxng");

    server.abort();
}
