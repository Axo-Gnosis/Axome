use std::sync::Arc;

use async_trait::async_trait;
use axo_ai::auth::provider::{DeviceCode, LoginInteraction, LoginPrompt, LoginSelect};
use axome_protocol::axo::{
    AxoLoginDeviceCode, AxoLoginOption, AxoLoginQuestion, AxoLoginQuestionKind, AxoLoginStarted,
    AxoLoginStatus, AxoLoginUrl, AxoLoginView,
};
use parking_lot::Mutex;
use tokio::{sync::oneshot, task::JoinHandle};

use super::AxoService;
use crate::errors::ApiError;

pub(super) struct Login {
    id: String,
    task: JoinHandle<()>,
    state: Arc<Mutex<LoginState>>,
}

impl Login {
    async fn stop(mut self) {
        self.task.abort();
        let _ = (&mut self.task).await;
    }
}

impl Drop for Login {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A login's current state, shared by its task (through `Interaction`) and the RPCs.
#[derive(Default)]
struct LoginState {
    view: AxoLoginView,
    /// Questions asked so far; the latest one's id.
    questions: u32,
    /// Answers `view.question`.
    answer: Option<oneshot::Sender<String>>,
    outcome: Option<Result<(), String>>,
}

impl LoginState {
    /// Delivers `answer` to the open question `question_id`.
    fn answer(&mut self, question_id: u32, answer: String) -> Result<(), ApiError> {
        let gone =
            || ApiError::InvalidArgument("Login is no longer waiting for this answer".into());
        let question = self
            .view
            .question
            .as_ref()
            .filter(|question| question.id == question_id)
            .ok_or_else(gone)?;
        if let AxoLoginQuestionKind::Select { options } = &question.kind {
            if !options.iter().any(|option| option.id == answer) {
                return Err(ApiError::InvalidArgument("Unknown option".into()));
            }
        }
        self.view.question = None;
        self.answer
            .take()
            .ok_or_else(gone)?
            .send(answer)
            .map_err(|_| gone())
    }
}

/// The UI side of a login: each request overwrites the matching part of the view.
struct Interaction(Arc<Mutex<LoginState>>);

impl Interaction {
    async fn ask(
        &self,
        message: &str,
        kind: AxoLoginQuestionKind,
    ) -> Result<String, axo_ai::Error> {
        let (sender, receiver) = oneshot::channel();
        let id = {
            let mut state = self.0.lock();
            state.questions += 1;
            let id = state.questions;
            state.view.question = Some(AxoLoginQuestion {
                id,
                message: message.to_owned(),
                kind,
            });
            state.answer = Some(sender);
            id
        };
        // The provider drops this future when the answer is no longer needed (its browser
        // callback won); the question must disappear with it.
        let _withdraw = Withdraw { state: &self.0, id };
        receiver
            .await
            .map_err(|_| axo_ai::Error::Auth("Login was cancelled".into()))
    }
}

struct Withdraw<'a> {
    state: &'a Mutex<LoginState>,
    id: u32,
}

impl Drop for Withdraw<'_> {
    fn drop(&mut self) {
        let mut state = self.state.lock();
        if state
            .view
            .question
            .as_ref()
            .is_some_and(|question| question.id == self.id)
        {
            state.view.question = None;
            state.answer = None;
        }
    }
}

#[async_trait]
impl LoginInteraction for Interaction {
    async fn open_url(&self, url: &str, instructions: Option<&str>) {
        self.0.lock().view.url = Some(AxoLoginUrl {
            url: url.to_owned(),
            instructions: instructions.map(str::to_owned),
        });
    }

    async fn device_code(&self, code: &DeviceCode) {
        self.0.lock().view.device_code = Some(AxoLoginDeviceCode {
            user_code: code.user_code.clone(),
            verification_url: code.verification_url.clone(),
            expires_in_seconds: code.expires_in_seconds,
        });
    }

    async fn prompt(&self, prompt: &LoginPrompt) -> Result<String, axo_ai::Error> {
        self.ask(
            &prompt.message,
            AxoLoginQuestionKind::Prompt {
                placeholder: prompt.placeholder.clone(),
                secret: prompt.secret,
            },
        )
        .await
    }

    async fn select(&self, select: &LoginSelect) -> Result<String, axo_ai::Error> {
        self.ask(
            &select.message,
            AxoLoginQuestionKind::Select {
                options: select
                    .options
                    .iter()
                    .map(|option| AxoLoginOption {
                        id: option.id.clone(),
                        label: option.label.clone(),
                    })
                    .collect(),
            },
        )
        .await
    }

    async fn progress(&self, message: &str) {
        self.0.lock().view.progress = Some(message.to_owned());
    }
}

impl AxoService {
    pub(super) async fn login_stop(&self) {
        if let Some(login) = self.login.lock().await.take() {
            login.stop().await;
        }
    }

    pub(crate) async fn login_start(&self, provider: String) -> Result<AxoLoginStarted, ApiError> {
        let engine = self.engine().await?;
        if !engine
            .models
            .providers()
            .iter()
            .any(|info| info.id == provider && info.login)
        {
            return Err(ApiError::InvalidArgument(format!(
                "{provider} does not support login"
            )));
        }
        let mut slot = self.login.lock().await;
        self.ensure_open()?;
        // Wait for cancellation to release the single loopback listener before starting another.
        if let Some(old) = slot.take() {
            old.stop().await;
        }
        let login_id = uuid::Uuid::new_v4().to_string();
        let state = Arc::new(Mutex::new(LoginState::default()));
        let service = self.this.clone();
        let task = tokio::spawn({
            let state = state.clone();
            async move {
                let result = engine
                    .models
                    .login(&provider, &Interaction(state.clone()))
                    .await;
                match &result {
                    Ok(()) => {
                        if let Some(service) = service.upgrade() {
                            if let Err(error) = service.reload_models().await {
                                tracing::warn!(%error, "Reloading models after login failed");
                            }
                        }
                        if provider == axo_ai::auth::codex::PROVIDER {
                            // ponytail: axo refreshes the Codex account list in the background with no signal; refresh here so lists update. Remove when Models::subscribe lands (docs/plan/handoff/axo-models-batch.md, S).
                            // Detached: `Login.task` is aborted once the outcome is read.
                            let engine = engine.clone();
                            let service = service.clone();
                            tokio::spawn(async move {
                                if let Err(error) = engine.models.refresh().await {
                                    tracing::warn!(%error, "Model refresh after Codex login failed");
                                }
                                if let Some(service) = service.upgrade() {
                                    service.emit_models_changed();
                                }
                            });
                        }
                    }
                    Err(error) => tracing::warn!(%error, %provider, "Provider login failed"),
                }
                state.lock().outcome = Some(result.map_err(|error| error.to_string()));
            }
        });
        *slot = Some(Login {
            id: login_id.clone(),
            task,
            state,
        });
        Ok(AxoLoginStarted { login_id })
    }

    pub(crate) async fn login_submit(
        &self,
        login_id: &str,
        question_id: u32,
        answer: String,
    ) -> Result<(), ApiError> {
        let slot = self.login.lock().await;
        let login = slot
            .as_ref()
            .filter(|login| login.id == login_id)
            .ok_or_else(|| ApiError::NotFound(format!("Login {login_id}")))?;
        let result = login.state.lock().answer(question_id, answer);
        result
    }

    pub(crate) async fn login_status(&self, login_id: &str) -> Result<AxoLoginStatus, ApiError> {
        let mut slot = self.login.lock().await;
        let login = slot
            .as_ref()
            .filter(|login| login.id == login_id)
            .ok_or_else(|| ApiError::NotFound(format!("Login {login_id}")))?;
        let status = {
            let state = login.state.lock();
            match &state.outcome {
                Some(Ok(())) => AxoLoginStatus::Done,
                Some(Err(message)) => AxoLoginStatus::Failed {
                    message: message.clone(),
                },
                None => AxoLoginStatus::Pending {
                    view: state.view.clone(),
                },
            }
        };
        if !matches!(status, AxoLoginStatus::Pending { .. }) {
            *slot = None;
        }
        Ok(status)
    }

    pub(crate) async fn login_cancel(&self, login_id: &str) -> Result<(), ApiError> {
        let mut slot = self.login.lock().await;
        if slot.as_ref().is_some_and(|login| login.id == login_id) {
            slot.take().unwrap().stop().await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt() -> LoginPrompt {
        LoginPrompt {
            message: "Paste the code".into(),
            placeholder: None,
            secret: false,
        }
    }

    fn question_id(state: &Mutex<LoginState>) -> Option<u32> {
        state
            .lock()
            .view
            .question
            .as_ref()
            .map(|question| question.id)
    }

    #[tokio::test]
    async fn login_questions_follow_the_provider() {
        let state = Arc::new(Mutex::new(LoginState::default()));
        let interaction = Arc::new(Interaction(state.clone()));

        // An answer reaches the open question.
        let asking = tokio::spawn({
            let interaction = interaction.clone();
            async move { interaction.prompt(&prompt()).await }
        });
        while question_id(&state).is_none() {
            tokio::task::yield_now().await;
        }
        assert_eq!(question_id(&state), Some(1));
        state.lock().answer(1, "code".into()).unwrap();
        assert_eq!(asking.await.unwrap().unwrap(), "code");
        assert_eq!(question_id(&state), None);

        // A prompt the provider drops disappears.
        let asking = tokio::spawn({
            let interaction = interaction.clone();
            async move { interaction.prompt(&prompt()).await }
        });
        while question_id(&state).is_none() {
            tokio::task::yield_now().await;
        }
        asking.abort();
        let _ = asking.await;
        assert_eq!(question_id(&state), None);
        assert!(state.lock().answer.is_none());

        // A stale answer is refused and the open question stays.
        let asking = tokio::spawn({
            let interaction = interaction.clone();
            async move { interaction.prompt(&prompt()).await }
        });
        while question_id(&state).is_none() {
            tokio::task::yield_now().await;
        }
        assert_eq!(question_id(&state), Some(3));
        assert!(state.lock().answer(1, "stale".into()).is_err());
        assert_eq!(question_id(&state), Some(3));
        asking.abort();
    }
}
