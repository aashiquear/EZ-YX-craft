//! # ezyx-ai — the in-app assistant
//!
//! Provider-agnostic chat with **tool calling**, so the assistant can *operate the app*:
//! create shapes, trace an image, pick a bit, generate toolpaths, explain a G-code error, etc.
//!
//! * [`provider`] – Anthropic Messages API, OpenAI Chat Completions, and any OpenAI-compatible
//!   endpoint (Ollama, LM Studio, vLLM, OpenRouter …).
//! * [`agent`] – the tool-use loop. The app implements [`agent::ToolHost`]; tools marked
//!   `requires_confirmation` (anything that moves the machine or overwrites files) are never
//!   executed without the host asking the user.
//!
//! Credentials: API keys are read from environment variables named in the provider config
//! (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) or entered in Settings (stored in the user config dir;
//! OS keychain storage is on the roadmap). Local models via Ollama need no key.
//! Note: consumer chat subscriptions are generally not API credentials — users need an API key
//! from the provider's developer console, or a local model.

pub mod agent;
pub mod provider;

pub use agent::{Agent, ToolHost, ToolSpec};
pub use provider::{ChatMessage, ContentPart, ProviderConfig, ProviderKind, Role};

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("missing API key: set {0} or enter it in Settings")]
    MissingKey(String),
    #[error("http: {0}")]
    Http(String),
    #[error("provider returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("unexpected response: {0}")]
    Parse(String),
    #[error("agent stopped after {0} tool steps")]
    TooManySteps(usize),
}
