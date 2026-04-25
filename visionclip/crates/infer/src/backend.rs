use anyhow::Result;
use async_trait::async_trait;
use visionclip_common::ipc::Action;

#[derive(Debug, Clone)]
pub struct InferenceInput {
    pub request_id: String,
    pub action: Action,
    pub source_app: Option<String>,
    pub image_bytes: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Clone)]
pub struct InferenceOutput {
    pub text: String,
}

#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn infer(&self, input: InferenceInput) -> Result<InferenceOutput>;
}
