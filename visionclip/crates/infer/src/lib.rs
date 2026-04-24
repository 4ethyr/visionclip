pub mod backend;
pub mod ollama;
pub mod postprocess;
pub mod prompts;

pub use backend::{InferenceBackend, InferenceInput, InferenceOutput};
pub use ollama::{
    list_models as list_ollama_models, OllamaBackend, OllamaModelDetails, OllamaModelSummary,
};
