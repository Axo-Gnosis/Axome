use crate::{errors::ApiError, events::EventSink, services::folders::resolve_folder, Host};
use axome_protocol::chat::{
    ChatAnswer, ChatEvent, ChatForkInfo, ChatLinkTarget, ChatModelCatalog, ChatPart, ChatThinking,
};

impl Host {
    pub fn chat_models(&self, folder_path: String) -> Result<ChatModelCatalog, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo.block_on(self.axo.catalog(&folder))?
    }

    pub fn chat_create(
        &self,
        folder_path: String,
        model: String,
        thinking: ChatThinking,
    ) -> Result<String, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.create(&folder, model, thinking))?
    }

    pub fn chat_prompt(
        &self,
        folder_path: String,
        session_id: String,
        request_id: String,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.prompt(&folder, &session_id, request_id, parts))?
    }

    pub fn chat_steer(
        &self,
        folder_path: String,
        session_id: String,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.steer(&folder, &session_id, parts))?
    }

    pub fn chat_follow_up(
        &self,
        folder_path: String,
        session_id: String,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.follow_up(&folder, &session_id, parts))?
    }

    pub fn chat_abort(&self, folder_path: String, session_id: String) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo.block_on(self.axo.abort(&folder, &session_id))?
    }

    pub fn chat_close(&self, folder_path: String, session_id: String) -> Result<(), ApiError> {
        resolve_folder(&folder_path)?;
        self.axo.block_on(self.axo.close(&session_id))?
    }

    pub fn chat_close_snapshot(&self, snapshot: &str) {
        for session_id in crate::workbench_snapshot::chat_session_ids(snapshot) {
            match self.axo.block_on(self.axo.close(&session_id)) {
                Ok(Ok(())) => {}
                Ok(Err(error)) | Err(error) => {
                    tracing::warn!(%session_id, %error, "Failed to close workbench chat");
                }
            }
        }
    }

    pub fn chat_answer(
        &self,
        folder_path: String,
        session_id: String,
        call_id: String,
        answers: Vec<ChatAnswer>,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.answer(&folder, &session_id, call_id, answers))?
    }

    pub fn chat_dismiss(
        &self,
        folder_path: String,
        session_id: String,
        call_id: String,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.dismiss(&folder, &session_id, call_id))?
    }

    pub fn chat_run_command(
        &self,
        folder_path: String,
        session_id: String,
        name: String,
        args: serde_json::Value,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.run_command(&folder, &session_id, name, args))?
    }

    pub fn chat_resolve_link(
        &self,
        folder_path: String,
        session_id: String,
        url: String,
    ) -> Result<ChatLinkTarget, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.resolve_link(&folder, &session_id, url))?
    }

    pub fn chat_navigate(
        &self,
        folder_path: String,
        session_id: String,
        entry_id: Option<String>,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.navigate(&folder, &session_id, entry_id))?
    }

    pub fn chat_select_branch(
        &self,
        folder_path: String,
        session_id: String,
        entry_id: String,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.select_branch(&folder, &session_id, entry_id))?
    }

    pub fn chat_fork(
        &self,
        folder_path: String,
        session_id: String,
        entry_id: String,
    ) -> Result<ChatForkInfo, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.fork(&folder, &session_id, entry_id))?
    }

    pub fn chat_attach(
        &self,
        folder_path: String,
        session_id: String,
        attach_id: String,
        events: EventSink<ChatEvent>,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.attach(&folder, &session_id, attach_id, events))?
    }

    pub fn chat_detach(&self, session_id: &str, attach_id: &str) {
        self.axo.detach(session_id, attach_id);
    }
}
