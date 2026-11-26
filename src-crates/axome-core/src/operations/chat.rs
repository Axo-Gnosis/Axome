use crate::{errors::ApiError, events::EventSink, services::folders::resolve_folder, Host};
use axome_protocol::chat::{
    ChatAnswer, ChatEvent, ChatForkInfo, ChatJobOutput, ChatLinkTarget, ChatLiveSummary,
    ChatModelCatalog, ChatPart, ChatServiceTier, ChatThinking,
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
        service_tier: ChatServiceTier,
    ) -> Result<String, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.create(&folder, model, thinking, service_tier))?
    }

    pub fn chat_prompt(
        &self,
        folder_path: String,
        session_id: String,
        request_id: String,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.predict.observe(&parts);
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
        self.predict.observe(&parts);
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
        self.predict.observe(&parts);
        self.axo
            .block_on(self.axo.follow_up(&folder, &session_id, parts))?
    }

    pub fn chat_withdraw(
        &self,
        folder_path: String,
        session_id: String,
        submission_id: String,
    ) -> Result<Vec<ChatPart>, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.withdraw(&folder, &session_id, submission_id))?
    }

    pub fn chat_send_now(&self, folder_path: String, session_id: String) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo.block_on(self.axo.send_now(&folder, &session_id))?
    }

    pub fn chat_abort(&self, folder_path: String, session_id: String) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo.block_on(self.axo.abort(&folder, &session_id))?
    }

    pub fn chat_close(&self, _folder_path: String, session_id: String) -> Result<(), ApiError> {
        self.axo.block_on(self.axo.close(&session_id))?
    }

    pub fn chat_resume(&self, folder_path: String, session_id: String) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo.block_on(self.axo.resume(&folder, &session_id))?
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

    pub fn chat_set_agent(
        &self,
        folder_path: String,
        session_id: String,
        agent: Option<String>,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.set_agent(&folder, &session_id, agent))?
    }

    pub fn chat_set_mode(
        &self,
        folder_path: String,
        session_id: String,
        mode: Option<String>,
        parts: Vec<ChatPart>,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.set_mode(&folder, &session_id, mode, parts))?
    }

    pub fn chat_set_title(
        &self,
        folder_path: String,
        session_id: String,
        title: Option<String>,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.set_title(&folder, &session_id, title))?
    }

    pub fn chat_keyword_matches(
        &self,
        folder_path: String,
        session_id: String,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<axome_protocol::chat::ChatKeywordMatch>>, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.keyword_matches(&folder, &session_id, texts))?
    }

    pub fn chat_accept_result(
        &self,
        folder_path: String,
        session_id: String,
        call_id: String,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.accept_result(&folder, &session_id, call_id))?
    }

    pub fn chat_revise_result(
        &self,
        folder_path: String,
        session_id: String,
        call_id: String,
        feedback: String,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo.block_on(
            self.axo
                .revise_result(&folder, &session_id, call_id, feedback),
        )?
    }

    pub fn chat_accept_plan(
        &self,
        folder_path: String,
        session_id: String,
        call_id: String,
        save_as: String,
        then: axome_protocol::chat::ChatPlanThen,
    ) -> Result<String, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo.block_on(
            self.axo
                .accept_plan(&folder, &session_id, call_id, save_as, then),
        )?
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

    pub fn chat_background(
        &self,
        folder_path: String,
        session_id: String,
        call_id: String,
    ) -> Result<bool, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.background(&folder, &session_id, call_id))?
    }

    pub fn chat_kill_job(
        &self,
        folder_path: String,
        session_id: String,
        job_id: String,
    ) -> Result<bool, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.kill_job(&folder, &session_id, job_id))?
    }

    pub fn chat_resume_wakes(
        &self,
        folder_path: String,
        session_id: String,
    ) -> Result<(), ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.resume_wakes(&folder, &session_id))?
    }

    pub fn chat_watch_job(
        &self,
        folder_path: String,
        session_id: String,
        attach_id: String,
        job_id: Option<String>,
    ) -> Result<Option<ChatJobOutput>, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.watch_job(&folder, &session_id, attach_id, job_id))?
    }

    pub fn chat_context_details(
        &self,
        folder_path: String,
        session_id: String,
    ) -> Result<axome_protocol::chat::ChatContextDetails, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.context_details(&folder, &session_id))?
    }

    pub fn chat_local_files(
        &self,
        folder_path: String,
        session_id: String,
    ) -> Result<axome_protocol::chat::ChatLocalFiles, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.local_files(&folder, &session_id))?
    }

    pub fn chat_squash_changes(
        &self,
        folder_path: String,
        session_id: String,
        changes: Vec<serde_json::Value>,
    ) -> Result<Vec<axome_protocol::chat::ChatSquashedChange>, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo
            .block_on(self.axo.squash_changes(&folder, &session_id, changes))?
    }

    pub fn chat_turn_diff_file(
        &self,
        folder_path: String,
        session_id: String,
        before_sha: Option<String>,
        after_sha: String,
    ) -> Result<axome_protocol::git::DiffFileContent, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        self.axo.block_on(
            self.axo
                .turn_diff_file(&folder, &session_id, before_sha, after_sha),
        )?
    }

    pub fn chat_live_summaries(
        &self,
        folder_path: String,
    ) -> Result<Vec<ChatLiveSummary>, ApiError> {
        let folder = resolve_folder(&folder_path)?;
        Ok(self.axo.live_summaries(&folder))
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

    pub fn chat_detach_sink(&self, session_id: &str, attach_id: &str, sink: &EventSink<ChatEvent>) {
        self.axo.detach_sink(session_id, attach_id, sink);
    }
}
