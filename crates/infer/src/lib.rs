pub mod backend;
pub mod ollama;
pub mod postprocess;
pub mod prompts;
pub mod provider;

pub use backend::{
    EmbeddingBackend, EmbeddingInput, EmbeddingOutput, InferenceBackend, InferenceInput,
    InferenceOutput,
};
pub use ollama::{
    list_models as list_ollama_models, OllamaBackend, OllamaModelDetails, OllamaModelSummary,
};
pub use provider::{
    AiProvider, AiTask, ChatRequest, ChatResponse, EmbedRequest, EmbedResponse, ProviderCapability,
    ProviderHealth, ProviderMode, ProviderRouteRequest, ProviderRouter, ProviderSelection,
    TranslateRequest, Translation, VisionRequest, VisionResponse,
};
