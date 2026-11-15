use crate::stream::{StreamReader, StreamWriter};
use axome_core::{errors::ApiError, events::EventSink, Host};
use axome_protocol::{
    chat::ChatEvent,
    rpc::{ChatDown, WireError},
};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

pub(crate) async fn run(
    host: Arc<Host>,
    attach_id: String,
    folder_path: String,
    session_id: String,
    mut send: StreamWriter,
    mut recv: StreamReader,
    mut shutdown: watch::Receiver<bool>,
) {
    let (sender, mut events) = mpsc::channel::<ChatEvent>(1024);
    let sink: EventSink<ChatEvent> = Arc::new(move |event| {
        sender.try_send(event).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => "chat stream lagging".to_owned(),
            mpsc::error::TrySendError::Closed(_) => "chat stream closed".to_owned(),
        })
    });
    let attach_host = Arc::clone(&host);
    let id = session_id.clone();
    let attachment = attach_id.clone();
    // Weak, so the channel still closes when the host drops this subscriber.
    let stream_sink = Arc::downgrade(&sink);
    let attached = tokio::task::spawn_blocking(move || {
        attach_host.chat_attach(folder_path, id, attachment, sink)
    })
    .await
    .map_err(ApiError::from)
    .and_then(|result| result);
    if let Err(error) = attached {
        let _ = send
            .write_json(&ChatDown::Closed {
                error: WireError::from(error),
            })
            .await;
        let _ = send.finish().await;
        return;
    }

    // Keep the read future alive across events so a partial frame is never cancelled.
    let input = async { while recv.read_tagged::<serde::de::IgnoredAny>().await.is_ok() {} };
    tokio::pin!(input);
    loop {
        let event = tokio::select! {
            event = events.recv() => event,
            _ = &mut input => break,
            _ = shutdown.changed() => break,
        };
        let Some(event) = event else { break };
        let closed = matches!(&event, ChatEvent::Closed { .. });
        if send.write_json(&ChatDown::Event { event }).await.is_err() || closed {
            break;
        }
    }
    let _ = send.finish().await;
    // A dead sink has no registration left to remove.
    let Some(stream_sink) = stream_sink.upgrade() else {
        return;
    };
    if let Err(error) = tokio::task::spawn_blocking(move || {
        host.chat_detach_sink(&session_id, &attach_id, &stream_sink)
    })
    .await
    {
        tracing::warn!(%error, "chat stream detach task failed");
    }
}
