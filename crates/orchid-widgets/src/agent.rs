//! One-shot chat completion for Ollama and OpenAI-compatible APIs.
//!
//! Universal Search queues a question on [`orchid_core::BackgroundJobQueue`].
//! This module builds the request and reads the assistant text. It does not
//! call tools or keep a conversation.

use std::sync::OnceLock;
use std::time::Duration;

use orchid_storage::AgentConfig;
use serde_json::Value;

/// Largest question accepted, in Unicode scalars.
const MAX_PROMPT_CHARS: usize = 8_000;

/// Largest response body read from the server.
const MAX_BODY_BYTES: usize = 1024 * 1024;

const OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434";
const OPENAI_ENDPOINT: &str = "https://api.openai.com/v1";

/// Why a question was not answered.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// Settings have the agent turned off.
    #[error("disabled")]
    Disabled,
    /// No model name is configured.
    #[error("needs-model")]
    NeedsModel,
    /// The endpoint is not an http(s) URL.
    #[error("bad-endpoint")]
    BadEndpoint,
    /// The question was blank.
    #[error("empty-prompt")]
    EmptyPrompt,
    /// The question exceeded [`MAX_PROMPT_CHARS`].
    #[error("prompt-too-long")]
    PromptTooLong,
    /// The server rejected the call or the body was not a chat reply.
    #[error("{0}")]
    Failed(String),
}

/// Which chat API to call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentBackend {
    /// `POST {endpoint}/api/chat`.
    Ollama,
    /// `POST {endpoint}/chat/completions`.
    OpenAi,
}

/// Parsed backend name. Unknown values stay on Ollama.
#[must_use]
pub fn parse_backend(backend: &str) -> AgentBackend {
    if backend.eq_ignore_ascii_case("openai") {
        AgentBackend::OpenAi
    } else {
        AgentBackend::Ollama
    }
}

/// Default base URL when [`AgentConfig::endpoint`] is empty.
#[must_use]
pub fn default_endpoint(backend: &str) -> &'static str {
    match parse_backend(backend) {
        AgentBackend::Ollama => OLLAMA_ENDPOINT,
        AgentBackend::OpenAi => OPENAI_ENDPOINT,
    }
}

struct RequestPlan {
    url: String,
    body: Value,
    bearer: Option<String>,
    backend: AgentBackend,
}

fn plan(cfg: &AgentConfig, api_key: &str, prompt: &str) -> Result<RequestPlan, AgentError> {
    if !cfg.enabled {
        return Err(AgentError::Disabled);
    }
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(AgentError::EmptyPrompt);
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(AgentError::PromptTooLong);
    }
    let model = cfg.model.trim();
    if model.is_empty() {
        return Err(AgentError::NeedsModel);
    }
    let backend = parse_backend(&cfg.backend);
    let base = {
        let raw = cfg.endpoint.trim();
        let raw = if raw.is_empty() {
            default_endpoint(&cfg.backend)
        } else {
            raw
        };
        raw.trim_end_matches('/')
    };
    let url = reqwest::Url::parse(base).map_err(|_| AgentError::BadEndpoint)?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(AgentError::BadEndpoint);
    }
    if url.host_str().is_none() {
        return Err(AgentError::BadEndpoint);
    }
    let (path, body) = match backend {
        AgentBackend::Ollama => (
            "/api/chat",
            serde_json::json!({
                "model": model,
                "stream": false,
                "messages": [{ "role": "user", "content": prompt }],
            }),
        ),
        AgentBackend::OpenAi => (
            "/chat/completions",
            serde_json::json!({
                "model": model,
                "messages": [{ "role": "user", "content": prompt }],
            }),
        ),
    };
    let url = format!("{base}{path}");
    let bearer = {
        let key = api_key.trim();
        if key.is_empty() {
            None
        } else {
            Some(key.to_string())
        }
    };
    Ok(RequestPlan {
        url,
        body,
        bearer,
        backend,
    })
}

/// Send one user message and return the assistant text.
///
/// Redirects are not followed, so a bearer token stays on the configured host.
pub async fn complete(
    cfg: &AgentConfig,
    api_key: &str,
    prompt: &str,
) -> Result<String, AgentError> {
    let plan = plan(cfg, api_key, prompt)?;
    let client = http_client();
    let mut request = client
        .post(&plan.url)
        .header("accept", "application/json")
        .json(&plan.body);
    if let Some(token) = &plan.bearer {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|e| AgentError::Failed(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(AgentError::Failed(format!("HTTP {}", status.as_u16())));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| AgentError::Failed(e.to_string()))?;
    if bytes.len() > MAX_BODY_BYTES {
        return Err(AgentError::Failed("response is too large".into()));
    }
    parse_reply(plan.backend, &bytes)
}

fn parse_reply(backend: AgentBackend, bytes: &[u8]) -> Result<String, AgentError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|e| AgentError::Failed(format!("reply was not JSON: {e}")))?;
    let text = match backend {
        AgentBackend::Ollama => value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str),
        AgentBackend::OpenAi => value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str),
    };
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Err(AgentError::Failed("reply had no text".into()));
    };
    Ok(text.to_string())
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(format!("Orchid/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled(backend: &str, endpoint: &str, model: &str) -> AgentConfig {
        AgentConfig {
            enabled: true,
            backend: backend.into(),
            endpoint: endpoint.into(),
            model: model.into(),
            api_key: String::new(),
        }
    }

    #[test]
    fn ollama_plan_uses_the_local_default_and_skips_a_blank_key() {
        let cfg = enabled("ollama", "", "llama3.2");
        let plan = plan(&cfg, "  ", "hello").unwrap();
        assert_eq!(plan.url, "http://127.0.0.1:11434/api/chat");
        assert!(plan.bearer.is_none());
        assert_eq!(plan.body["stream"], false);
        assert_eq!(plan.body["messages"][0]["content"], "hello");
    }

    #[test]
    fn openai_plan_trims_the_base_and_sends_the_key() {
        let cfg = enabled("OpenAI", "https://example.test/v1/", "gpt-4o-mini");
        let plan = plan(&cfg, "secret", "hello").unwrap();
        assert_eq!(plan.url, "https://example.test/v1/chat/completions");
        assert_eq!(plan.bearer.as_deref(), Some("secret"));
        assert!(plan.body.get("stream").is_none());
    }

    #[test]
    fn plan_rejects_a_disabled_agent_and_a_non_http_endpoint() {
        let mut cfg = enabled("ollama", "", "m");
        cfg.enabled = false;
        assert!(matches!(plan(&cfg, "", "hi"), Err(AgentError::Disabled)));
        cfg.enabled = true;
        cfg.model.clear();
        assert!(matches!(plan(&cfg, "", "hi"), Err(AgentError::NeedsModel)));
        cfg.model = "m".into();
        cfg.endpoint = "file:///tmp/model".into();
        assert!(matches!(plan(&cfg, "", "hi"), Err(AgentError::BadEndpoint)));
    }

    #[test]
    fn parsers_read_assistant_text() {
        let ollama = br#"{"message":{"role":"assistant","content":" woof "}}"#;
        assert_eq!(parse_reply(AgentBackend::Ollama, ollama).unwrap(), "woof");
        let openai = br#"{"choices":[{"message":{"role":"assistant","content":"meow"}}]}"#;
        assert_eq!(parse_reply(AgentBackend::OpenAi, openai).unwrap(), "meow");
        assert!(parse_reply(AgentBackend::Ollama, b"{}").is_err());
    }
}
