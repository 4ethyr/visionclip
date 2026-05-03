use crate::backend::{EmbeddingOutput, InferenceOutput};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};
use visionclip_common::ipc::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderCapability {
    Chat,
    Vision,
    Ocr,
    Embeddings,
    DocumentTranslation,
    SpeechToText,
    TextToSpeech,
    WebSearch,
    ToolCalling,
    StructuredOutputs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderMode {
    LocalOnly,
    LocalFirst,
    CloudAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTask {
    Chat,
    Vision,
    Ocr,
    Embeddings,
    DocumentTranslation,
    SpeechToText,
    TextToSpeech,
    WebSearch,
}

impl AiTask {
    pub fn required_capability(self) -> ProviderCapability {
        match self {
            Self::Chat => ProviderCapability::Chat,
            Self::Vision => ProviderCapability::Vision,
            Self::Ocr => ProviderCapability::Ocr,
            Self::Embeddings => ProviderCapability::Embeddings,
            Self::DocumentTranslation => ProviderCapability::DocumentTranslation,
            Self::SpeechToText => ProviderCapability::SpeechToText,
            Self::TextToSpeech => ProviderCapability::TextToSpeech,
            Self::WebSearch => ProviderCapability::WebSearch,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderRouteRequest {
    pub task: AiTask,
    pub mode: ProviderMode,
    pub sensitive: bool,
}

impl ProviderRouteRequest {
    pub fn local_first(task: AiTask) -> Self {
        Self {
            task,
            mode: ProviderMode::LocalFirst,
            sensitive: false,
        }
    }

    pub fn local_only(task: AiTask) -> Self {
        Self {
            task,
            mode: ProviderMode::LocalOnly,
            sensitive: false,
        }
    }

    pub fn sensitive(task: AiTask) -> Self {
        Self {
            task,
            mode: ProviderMode::LocalFirst,
            sensitive: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderHealth {
    pub id: String,
    pub display_name: String,
    pub local: bool,
    pub available: bool,
    pub capabilities: Vec<ProviderCapability>,
    pub message: Option<String>,
}

impl ProviderHealth {
    pub fn supports(&self, capability: ProviderCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub request_id: String,
    pub action: Action,
    pub source_app: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    pub text: String,
}

impl From<InferenceOutput> for ChatResponse {
    fn from(output: InferenceOutput) -> Self {
        Self { text: output.text }
    }
}

#[derive(Debug, Clone)]
pub struct VisionRequest {
    pub request_id: String,
    pub action: Action,
    pub source_app: Option<String>,
    pub image_bytes: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionResponse {
    pub text: String,
}

impl From<InferenceOutput> for VisionResponse {
    fn from(output: InferenceOutput) -> Self {
        Self { text: output.text }
    }
}

#[derive(Debug, Clone)]
pub struct EmbedRequest {
    pub request_id: String,
    pub texts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbedResponse {
    pub vectors: Vec<Vec<f32>>,
}

impl From<EmbeddingOutput> for EmbedResponse {
    fn from(output: EmbeddingOutput) -> Self {
        Self {
            vectors: output.vectors,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranslateRequest {
    pub request_id: String,
    pub target_language: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    pub text: String,
}

impl From<InferenceOutput> for Translation {
    fn from(output: InferenceOutput) -> Self {
        Self { text: output.text }
    }
}

#[derive(Debug, Clone)]
pub struct SearchAnswerRequest {
    pub request_id: String,
    pub query: String,
    pub source_label: String,
    pub ai_overview_text: String,
    pub supporting_sources: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchAnswerResponse {
    pub text: String,
}

impl From<InferenceOutput> for SearchAnswerResponse {
    fn from(output: InferenceOutput) -> Self {
        Self { text: output.text }
    }
}

#[derive(Debug, Clone)]
pub struct ReplRequest {
    pub request_id: String,
    pub user_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplResponse {
    pub text: String,
}

impl From<InferenceOutput> for ReplResponse {
    fn from(output: InferenceOutput) -> Self {
        Self { text: output.text }
    }
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse> {
        Err(anyhow!("provider does not support chat"))
    }

    async fn vision(&self, _req: VisionRequest) -> Result<VisionResponse> {
        Err(anyhow!("provider does not support vision"))
    }

    async fn ocr(&self, _req: VisionRequest) -> Result<VisionResponse> {
        Err(anyhow!("provider does not support OCR"))
    }

    async fn embed(&self, _req: EmbedRequest) -> Result<EmbedResponse> {
        Err(anyhow!("provider does not support embeddings"))
    }

    async fn translate(&self, _req: TranslateRequest) -> Result<Translation> {
        Err(anyhow!("provider does not support translation"))
    }

    async fn answer_search(&self, _req: SearchAnswerRequest) -> Result<SearchAnswerResponse> {
        Err(anyhow!("provider does not support grounded search answers"))
    }

    async fn answer_repl(&self, _req: ReplRequest) -> Result<ReplResponse> {
        Err(anyhow!("provider does not support REPL answers"))
    }

    async fn health(&self) -> ProviderHealth;
}

#[derive(Clone)]
pub struct ProviderSelection {
    pub id: String,
    pub provider: Arc<dyn AiProvider>,
    pub health: ProviderHealth,
}

pub struct ProviderRouter {
    providers: HashMap<String, Arc<dyn AiProvider>>,
}

impl ProviderRouter {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn with_provider(mut self, id: impl Into<String>, provider: Arc<dyn AiProvider>) -> Self {
        self.register(id, provider);
        self
    }

    pub fn register(&mut self, id: impl Into<String>, provider: Arc<dyn AiProvider>) {
        self.providers.insert(id.into(), provider);
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub async fn route(&self, request: ProviderRouteRequest) -> Result<ProviderSelection> {
        let capability = request.task.required_capability();
        let mut fallback: Option<ProviderSelection> = None;

        for (id, provider) in &self.providers {
            let health = provider.health().await;
            if !health.available || !health.supports(capability) {
                continue;
            }

            if request.mode == ProviderMode::LocalOnly && !health.local {
                continue;
            }

            if request.sensitive && !health.local {
                continue;
            }

            let selection = ProviderSelection {
                id: id.clone(),
                provider: Arc::clone(provider),
                health,
            };

            if selection.health.local {
                return Ok(selection);
            }

            fallback.get_or_insert(selection);
        }

        if request.mode == ProviderMode::CloudAllowed && !request.sensitive {
            if let Some(selection) = fallback {
                return Ok(selection);
            }
        }

        Err(anyhow!(
            "no provider available for task {:?} with mode {:?}",
            request.task,
            request.mode
        ))
    }
}

impl Default for ProviderRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticProvider {
        health: ProviderHealth,
    }

    #[async_trait]
    impl AiProvider for StaticProvider {
        async fn health(&self) -> ProviderHealth {
            self.health.clone()
        }
    }

    fn provider(
        id: &str,
        local: bool,
        capabilities: Vec<ProviderCapability>,
    ) -> Arc<dyn AiProvider> {
        Arc::new(StaticProvider {
            health: ProviderHealth {
                id: id.into(),
                display_name: id.into(),
                local,
                available: true,
                capabilities,
                message: None,
            },
        })
    }

    #[tokio::test]
    async fn router_prefers_local_provider() {
        let router = ProviderRouter::new()
            .with_provider(
                "cloud",
                provider("cloud", false, vec![ProviderCapability::Chat]),
            )
            .with_provider(
                "local",
                provider("local", true, vec![ProviderCapability::Chat]),
            );

        let selected = router
            .route(ProviderRouteRequest::local_first(AiTask::Chat))
            .await
            .unwrap();

        assert_eq!(selected.id, "local");
        assert!(selected.health.local);
    }

    #[tokio::test]
    async fn router_blocks_cloud_when_local_only() {
        let router = ProviderRouter::new().with_provider(
            "cloud",
            provider("cloud", false, vec![ProviderCapability::Chat]),
        );

        let result = router
            .route(ProviderRouteRequest::local_only(AiTask::Chat))
            .await;
        let error = match result {
            Ok(selection) => panic!("unexpected provider selected: {}", selection.id),
            Err(error) => error,
        };

        assert!(error.to_string().contains("no provider available"));
    }

    #[tokio::test]
    async fn router_blocks_cloud_for_sensitive_context() {
        let router = ProviderRouter::new().with_provider(
            "cloud",
            provider(
                "cloud",
                false,
                vec![ProviderCapability::DocumentTranslation],
            ),
        );

        let result = router
            .route(ProviderRouteRequest::sensitive(AiTask::DocumentTranslation))
            .await;
        let error = match result {
            Ok(selection) => panic!("unexpected provider selected: {}", selection.id),
            Err(error) => error,
        };

        assert!(error.to_string().contains("no provider available"));
    }

    #[tokio::test]
    async fn router_allows_cloud_when_explicitly_enabled() {
        let router = ProviderRouter::new().with_provider(
            "cloud",
            provider("cloud", false, vec![ProviderCapability::WebSearch]),
        );

        let selected = router
            .route(ProviderRouteRequest {
                task: AiTask::WebSearch,
                mode: ProviderMode::CloudAllowed,
                sensitive: false,
            })
            .await
            .unwrap();

        assert_eq!(selected.id, "cloud");
        assert!(!selected.health.local);
    }
}
