use std::time::Duration;

use axome_protocol::axo::{AxoLoginStarted, AxoLoginStatus};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::AxoService;
use crate::errors::ApiError;

pub(super) struct Login {
    id: String,
    task: JoinHandle<()>,
    manual: Option<oneshot::Sender<String>>,
    status: AxoLoginStatus,
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

impl AxoService {
    pub(super) async fn login_stop(&self) {
        if let Some(login) = self.login.lock().await.take() {
            login.stop().await;
        }
    }

    pub(crate) async fn login_start(&self, provider: String) -> Result<AxoLoginStarted, ApiError> {
        if provider != axo_ai::auth::codex::PROVIDER {
            return Err(ApiError::InvalidArgument(format!(
                "OAuth is not supported for {provider}"
            )));
        }
        let mut slot = self.login.lock().await;
        if self.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ApiError::Internal("Axome is shutting down".into()));
        }
        // Wait for cancellation to release the single loopback listener before starting another.
        if let Some(old) = slot.take() {
            old.stop().await;
        }
        let login_id = uuid::Uuid::new_v4().to_string();
        let id = login_id.clone();
        let service = self.this.clone();
        let credentials = self.credentials.clone();
        let (manual, receiver) = oneshot::channel();
        let (urls, mut url_receiver) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            let on_url = move |url: &str| {
                let _ = urls.send(url.to_owned());
            };
            let result = axo_ai::auth::codex::login(
                &axo_ai::api::http::client(),
                credentials.as_ref(),
                &on_url,
                async move { receiver.await.ok() },
            )
            .await;
            if let Some(service) = service.upgrade() {
                service.invalidate().await;
                if let Some(login) = service
                    .login
                    .lock()
                    .await
                    .as_mut()
                    .filter(|login| login.id == id)
                {
                    login.manual = None;
                    login.status = match result {
                        Ok(()) => AxoLoginStatus::Done,
                        Err(error) => {
                            tracing::warn!(%error, "Codex OAuth login failed");
                            AxoLoginStatus::Failed {
                                message: error.to_string(),
                            }
                        }
                    };
                }
            }
        });
        *slot = Some(Login {
            id: login_id.clone(),
            task,
            manual: Some(manual),
            status: AxoLoginStatus::Pending,
        });
        drop(slot);
        match tokio::time::timeout(Duration::from_secs(10), url_receiver.recv()).await {
            Ok(Some(url)) => Ok(AxoLoginStarted { login_id, url }),
            result => {
                let failed_message = self
                    .login
                    .lock()
                    .await
                    .as_ref()
                    .filter(|login| login.id == login_id)
                    .and_then(|login| match &login.status {
                        AxoLoginStatus::Failed { message } => Some(message.clone()),
                        _ => None,
                    });
                self.login_cancel(&login_id).await?;
                let message = match result {
                    Err(_) => "OAuth login did not provide a URL within 10 seconds".into(),
                    Ok(None) => failed_message
                        .unwrap_or_else(|| "OAuth login ended before providing a URL".into()),
                    Ok(Some(_)) => unreachable!(),
                };
                Err(ApiError::Internal(message))
            }
        }
    }

    pub(crate) async fn login_submit(
        &self,
        login_id: &str,
        redirect: String,
    ) -> Result<(), ApiError> {
        let mut slot = self.login.lock().await;
        let login = slot
            .as_mut()
            .filter(|login| login.id == login_id)
            .ok_or_else(|| ApiError::NotFound(format!("OAuth login {login_id}")))?;
        let manual = login.manual.take().ok_or_else(|| {
            ApiError::InvalidArgument("OAuth login is no longer awaiting input".into())
        })?;
        manual.send(redirect).map_err(|_| {
            ApiError::InvalidArgument("OAuth login is no longer awaiting input".into())
        })
    }

    pub(crate) async fn login_status(&self, login_id: &str) -> Result<AxoLoginStatus, ApiError> {
        let mut slot = self.login.lock().await;
        let status = slot
            .as_ref()
            .filter(|login| login.id == login_id)
            .ok_or_else(|| ApiError::NotFound(format!("OAuth login {login_id}")))?
            .status
            .clone();
        if !matches!(status, AxoLoginStatus::Pending) {
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
