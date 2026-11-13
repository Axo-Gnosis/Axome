use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axo::store::JsonlStore;
use axo::{Axo, Command, ConfigLayer, ConfigSource, PromptRequest, SessionOptions, UserConfig};
use axo_ai::testing::{reply_text, reply_tool_calls};
use axo_ai::{FileCredentialStore, Models, ModelsConfig, ModelsOptions, ScriptedApi, ScriptedTurn};
use axome_protocol::chat::{
    ChatAnswer, ChatCommandKind, ChatEntry, ChatEntryBody, ChatEvent, ChatLinkTarget, ChatPart,
    ChatRunOutcome, ChatThinking,
};
use base64::Engine as _;
use parking_lot::Mutex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::engine::Engine;
use super::AxoService;
use crate::events::EventSink;
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

    fn project(&self, _cwd: &Path) -> Result<ConfigLayer, axo::Error> {
        Ok(ConfigLayer::default())
    }
}

type Events = Arc<Mutex<Vec<ChatEvent>>>;

struct Fixture {
    service: Arc<AxoService>,
    folder: PathBuf,
    dir: PathBuf,
    api: Arc<ScriptedApi>,
}

impl Fixture {
    async fn new(turns: Vec<ScriptedTurn>) -> Self {
        Self::with_settings(turns, json!({})).await
    }

    async fn with_settings(turns: Vec<ScriptedTurn>, settings: Value) -> Self {
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
        let api = Arc::new(ScriptedApi::new(turns));
        let axo = Axo::builder(
            models.clone(),
            Arc::new(JsonlStore::new(dir.join("sessions"))),
        )
        .config(Arc::new(FixtureConfig(settings)))
        .streamer(api.clone())
        .build();
        let db = Arc::new(DatabaseService::new(dir.join("test.db")).unwrap());
        let service = AxoService::with_engine(
            db,
            Engine {
                axo: Arc::new(axo),
                models,
                models_error: None,
            },
        );
        Self {
            service,
            folder,
            dir,
            api,
        }
    }

    async fn create(&self) -> String {
        self.service
            .create(&self.folder, "test/m".into(), ChatThinking::Off)
            .await
            .unwrap()
    }

    async fn attach(&self, id: &str, attach_id: &str) -> Events {
        let events = Arc::new(Mutex::new(Vec::new()));
        let collected = Arc::clone(&events);
        let sink: EventSink<ChatEvent> = Arc::new(move |event| {
            collected.lock().push(event);
            Ok(())
        });
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
        self.wait_settled(id, events, runs).await;
    }

    async fn wait_settled(&self, id: &str, events: &Events, runs: usize) {
        wait_until(|| settled(&events.lock(), runs)).await;
        let session = self.service.sessions.lock()[id].session.clone();
        // axo 97513d6 publishes RunEnd before releasing its busy claim; Axome's
        // idle event is therefore not an admission barrier. Until axo exposes an
        // activity notification, fixture sequencing must also check its snapshot.
        // This does not fix the production race for actions immediately after a reply.
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let live = session.attach(None).await.live;
                if !live.running && live.maintenance.is_none() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("timed out waiting for axo to release its busy claim");
    }
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
            ChatEvent::Live { live } => Some(!live.running && live.questions.is_empty()),
            ChatEvent::Snapshot { state } => {
                Some(!state.live.running && state.live.questions.is_empty())
            }
            _ => None,
        }) == Some(true)
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
async fn ask_round_trip() {
    let fixture = Fixture::new(vec![
        ScriptedTurn::Reply(reply_tool_calls(&[(
            "ask-1",
            "ask",
            json!({
                "questions": [{
                    "id": "choice", "question": "Which file?",
                    "options": [{"label": "first"}, {"label": "second"}]
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
    assert_eq!(pending.questions.len(), 1);
    let question = &pending.questions[0];
    fixture
        .service
        .answer(
            &fixture.folder,
            &id,
            pending.call_id.clone(),
            vec![ChatAnswer {
                id: question.id.clone(),
                selected: vec![question.options[0].label.clone()],
                custom: None,
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
    drop(snapshot);

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
    let fixture = Fixture {
        service: AxoService::new(
            db,
            crate::HostDirs::under(&dir),
            axome_protocol::server_config::Supervision::ProcessGroup,
        ),
        folder: dir.clone(),
        dir,
        api: Arc::new(ScriptedApi::default()),
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
    fixture.wait_settled(id, events, 1).await;
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
    let _orphan = fixture.create().await;
    let id = fixture.create().await;
    fixture.attach(&id, "a").await;
    fixture.service.detach(&id, "a");
    assert_eq!(fixture.service.loaded(), 2);
    fixture.attach(&id, "b").await;
    fixture.service.detach(&id, "b");
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
            model: Some("test/m2".into()),
            ..Default::default()
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
        .chat_create(folder_str.clone(), "test/m".into(), ChatThinking::Off)
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
async fn engine_rebuilds_when_extended_context_changes() {
    let dir = std::env::temp_dir().join(format!("axome-extended-ctx-{}", uuid::Uuid::new_v4()));
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
    );
    service.bind_runtime();

    let first = service.engine().await.unwrap();
    assert_eq!(
        service.engine.lock().await.stamp.map(|(_, ext)| ext),
        Some(true)
    );

    std::fs::write(
        dirs.settings_path(),
        json!({"axo": {"extendedContext": false}}).to_string(),
    )
    .unwrap();

    let second = service.engine().await.unwrap();
    assert_eq!(
        service.engine.lock().await.stamp.map(|(_, ext)| ext),
        Some(false)
    );
    assert!(!Arc::ptr_eq(&first, &second));

    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
