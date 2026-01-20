//! Remote diffusion server client

use crate::{DiffusionBackend, DiffusionError, ProgressCallback};
use futures_util::{SinkExt, StreamExt};
use pentimento_ipc::DiffusionRequest;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Remote diffusion client that connects to a WebSocket server
pub struct RemoteDiffusion {
    server_url: String,
    cancelled: Arc<AtomicBool>,
    generating: Arc<AtomicBool>,
}

impl RemoteDiffusion {
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            cancelled: Arc::new(AtomicBool::new(false)),
            generating: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl DiffusionBackend for RemoteDiffusion {
    async fn generate(
        &mut self,
        request: DiffusionRequest,
        on_progress: Option<ProgressCallback>,
    ) -> Result<image::RgbaImage, DiffusionError> {
        self.cancelled.store(false, Ordering::SeqCst);
        self.generating.store(true, Ordering::SeqCst);

        let result = self.generate_inner(request, on_progress).await;

        self.generating.store(false, Ordering::SeqCst);
        result
    }

    fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn is_generating(&self) -> bool {
        self.generating.load(Ordering::SeqCst)
    }
}

impl RemoteDiffusion {
    async fn generate_inner(
        &self,
        request: DiffusionRequest,
        on_progress: Option<ProgressCallback>,
    ) -> Result<image::RgbaImage, DiffusionError> {
        // Connect to WebSocket server
        let (ws_stream, _) = connect_async(&self.server_url)
            .await
            .map_err(|e| DiffusionError::Connection(e.to_string()))?;

        let (mut write, mut read) = ws_stream.split();

        // Send the request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| DiffusionError::InvalidResponse(e.to_string()))?;

        write
            .send(Message::Text(request_json.into()))
            .await
            .map_err(|e| DiffusionError::Connection(e.to_string()))?;

        // Read responses
        let mut final_image: Option<image::RgbaImage> = None;

        while let Some(msg) = read.next().await {
            if self.cancelled.load(Ordering::SeqCst) {
                return Err(DiffusionError::Cancelled);
            }

            match msg {
                Ok(Message::Text(text)) => {
                    // Parse progress update
                    if let Ok(progress) = serde_json::from_str::<ProgressUpdate>(&text) {
                        if let Some(ref callback) = on_progress {
                            callback(progress.progress, None);
                        }
                    }
                }
                Ok(Message::Binary(data)) => {
                    // Final image data (PNG encoded)
                    let img = image::load_from_memory(&data)
                        .map_err(|e| DiffusionError::InvalidResponse(e.to_string()))?;
                    final_image = Some(img.to_rgba8());
                    break;
                }
                Ok(Message::Close(_)) => break,
                Err(e) => return Err(DiffusionError::Connection(e.to_string())),
                _ => {}
            }
        }

        final_image.ok_or_else(|| DiffusionError::InvalidResponse("No image received".into()))
    }
}

#[derive(serde::Deserialize)]
struct ProgressUpdate {
    progress: f32,
    #[allow(dead_code)]
    step: u32,
}
