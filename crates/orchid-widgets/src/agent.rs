//! Chat completion for Ollama and OpenAI-compatible APIs.
//!
//! The transcript lives in `data/agent-chat.json`. A turn may call `read_file`,
//! `list_dir`, and `search`. `propose_write` only records a pending file; bytes
//! hit the disk when the user confirms. Universal Search (`?`) and the Agent
//! widget share this transcript.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use orchid_core::BackgroundJobQueue;
use orchid_search::SearchEngine;
use orchid_storage::{AgentConfig, OrchidConfig};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Largest question accepted, in Unicode scalars.
const MAX_PROMPT_CHARS: usize = 8_000;

/// Largest response body read from the server.
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Stored turns, including tool results.
const MAX_MESSAGES: usize = 40;

/// Model round-trips that may call tools before the turn stops.
const MAX_TOOL_ROUNDS: usize = 4;

/// Bytes of a text file returned to the model.
const MAX_READ_BYTES: usize = 24 * 1024;

/// Directory names returned by one `list_dir` call.
const MAX_LIST_NAMES: usize = 80;

/// Bytes accepted by `propose_write`.
const MAX_WRITE_BYTES: usize = 256 * 1024;

/// Index hits returned by one `search` call.
const MAX_SEARCH_HITS: usize = 8;

const SYSTEM_PROMPT: &str = "You are Orchid's local assistant. You can read one local text file, list one directory, and search the user's file index. To change a file, call propose_write with the full new text. That call does not write anything; the user confirms in the Agent panel. Never claim a file was written. There is no shell. Prefer short answers.";

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

/// One stored turn. The system prompt is not stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    /// `user`, `assistant`, or `tool`.
    pub role: String,
    /// Visible text. Empty when the assistant only called tools.
    pub content: String,
    /// Tool calls attached to an assistant turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<StoredToolCall>,
    /// OpenAI tool-result id. Empty for other roles.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool_call_id: String,
    /// Tool name, used when replaying an Ollama tool result.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool_name: String,
}

/// One function call stored with an assistant turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredToolCall {
    /// Provider id, or a local id when the provider omitted one.
    pub id: String,
    /// `read_file`, `list_dir`, `search`, or `propose_write`.
    pub name: String,
    /// JSON object text.
    pub arguments: String,
}

/// A file the model asked to replace. Nothing is written until confirm.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingWrite {
    /// Absolute path the user will confirm.
    pub path: String,
    /// Full replacement text.
    pub content: String,
}

/// Transcript on disk and in memory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Conversation {
    /// Oldest first.
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    /// Set by `propose_write`. Cleared by confirm or dismiss.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingWrite>,
}

/// What the Agent panel draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentView {
    /// Stored turns.
    pub messages: Vec<ChatMessage>,
    /// Empty when nothing is waiting.
    pub pending_path: String,
    /// Start of the pending text, for the confirm bar.
    pub pending_preview: String,
    /// `working`, `closed`, empty, or a short error.
    pub status: String,
}

/// Outcome of one turn, shown as a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentNotice {
    /// Assistant text, or the last tool line when the model sent none.
    Reply(String),
    /// Fluent key for a known refusal.
    Error(&'static str),
    /// Transport or parse failure.
    Failed(String),
}

struct AgentHost {
    data_dir: std::path::PathBuf,
    search: Option<Arc<SearchEngine>>,
    config: Arc<RwLock<OrchidConfig>>,
    chat: Mutex<Conversation>,
    inbox: Mutex<VecDeque<String>>,
    status: Mutex<String>,
}

static HOST: OnceLock<AgentHost> = OnceLock::new();
static NOTICES: Mutex<Vec<AgentNotice>> = Mutex::new(Vec::new());
static TURN_HOOK: Mutex<Option<fn()>> = Mutex::new(None);

/// Remember where the transcript is stored and which index `search` may use.
///
/// A second call is ignored. The window and the Agent widget share this host.
pub fn install(
    data_dir: PathBuf,
    search: Option<Arc<SearchEngine>>,
    config: Arc<RwLock<OrchidConfig>>,
) {
    let chat = load_chat(&data_dir);
    let _ = HOST.set(AgentHost {
        data_dir,
        search,
        config,
        chat: Mutex::new(chat),
        inbox: Mutex::new(VecDeque::new()),
        status: Mutex::new(String::new()),
    });
}

/// Called after the transcript changes so open Agent widgets republish.
pub fn set_turn_hook(hook: fn()) {
    *TURN_HOOK.lock() = Some(hook);
}

/// Current transcript for a widget snapshot.
#[must_use]
pub fn view() -> AgentView {
    let Some(host) = HOST.get() else {
        return AgentView {
            messages: Vec::new(),
            pending_path: String::new(),
            pending_preview: String::new(),
            status: "closed".into(),
        };
    };
    let chat = host.chat.lock().clone();
    let (pending_path, pending_preview) = match &chat.pending {
        Some(pending) => (pending.path.clone(), clip_chars(&pending.content, 500)),
        None => (String::new(), String::new()),
    };
    AgentView {
        messages: chat.messages,
        pending_path,
        pending_preview,
        status: host.status.lock().clone(),
    }
}

/// Notices produced since the last take. The UI tick drains these.
#[must_use]
pub fn take_notices() -> Vec<AgentNotice> {
    std::mem::take(&mut *NOTICES.lock())
}

/// Queue one user message on the shared transcript.
///
/// Known refusals become notices and are not stored. The job drains every
/// queued message, including ones that arrive while a turn is running.
pub fn submit(jobs: &BackgroundJobQueue, prompt: String) {
    let Some(host) = HOST.get() else {
        NOTICES
            .lock()
            .push(AgentNotice::Failed("agent store is not open".into()));
        return;
    };
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        NOTICES
            .lock()
            .push(AgentNotice::Error("agent-empty-prompt"));
        return;
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        NOTICES
            .lock()
            .push(AgentNotice::Error("agent-prompt-too-long"));
        return;
    }
    let cfg = host.config.read().agent.clone();
    if !cfg.enabled {
        NOTICES.lock().push(AgentNotice::Error("agent-disabled"));
        return;
    }
    if cfg.model.trim().is_empty() {
        NOTICES.lock().push(AgentNotice::Error("agent-needs-model"));
        return;
    }
    host.inbox.lock().push_back(prompt);
    jobs.spawn_coalesced("agent:turn", || async {
        drain_inbox().await;
    });
}

/// Write the pending file, then append a local note to the transcript.
///
/// # Errors
///
/// Returns a short reason when nothing is pending, the parent directory is
/// gone, or the write fails. A failed write keeps the proposal.
pub fn confirm_pending_write() -> Result<String, String> {
    let host = HOST
        .get()
        .ok_or_else(|| "agent store is not open".to_string())?;
    let path = {
        let mut chat = host.chat.lock();
        confirm_write(&mut chat)?
    };
    save_chat(host);
    *host.status.lock() = String::new();
    notify_widgets();
    Ok(path)
}

/// Drop the pending file without writing it.
pub fn dismiss_pending_write() {
    let Some(host) = HOST.get() else {
        return;
    };
    {
        let mut chat = host.chat.lock();
        dismiss_write(&mut chat);
    }
    save_chat(host);
    *host.status.lock() = String::new();
    notify_widgets();
}

/// Forget the transcript and any pending write.
pub fn clear_conversation() {
    let Some(host) = HOST.get() else {
        return;
    };
    *host.chat.lock() = Conversation::default();
    *host.status.lock() = String::new();
    save_chat(host);
    notify_widgets();
}

async fn drain_inbox() {
    let Some(host) = HOST.get() else {
        return;
    };
    loop {
        let prompt = host.inbox.lock().pop_front();
        let Some(prompt) = prompt else {
            break;
        };
        *host.status.lock() = "working".into();
        notify_widgets();
        match run_turn(host, &prompt).await {
            Ok(text) => NOTICES.lock().push(AgentNotice::Reply(text)),
            Err(AgentError::Disabled) => {
                NOTICES.lock().push(AgentNotice::Error("agent-disabled"));
            }
            Err(AgentError::NeedsModel) => {
                NOTICES.lock().push(AgentNotice::Error("agent-needs-model"));
            }
            Err(AgentError::BadEndpoint) => {
                NOTICES
                    .lock()
                    .push(AgentNotice::Error("agent-bad-endpoint"));
            }
            Err(AgentError::EmptyPrompt) => {
                NOTICES
                    .lock()
                    .push(AgentNotice::Error("agent-empty-prompt"));
            }
            Err(AgentError::PromptTooLong) => {
                NOTICES
                    .lock()
                    .push(AgentNotice::Error("agent-prompt-too-long"));
            }
            Err(AgentError::Failed(reason)) => {
                *host.status.lock() = reason.clone();
                NOTICES.lock().push(AgentNotice::Failed(reason));
                notify_widgets();
                continue;
            }
        }
        if host.status.lock().as_str() == "working" {
            *host.status.lock() = String::new();
        }
        notify_widgets();
    }
}

async fn run_turn(host: &AgentHost, prompt: &str) -> Result<String, AgentError> {
    let cfg = host.config.read().agent.clone();
    let api_key = orchid_crypto::resolve_stored_secret(&cfg.api_key)
        .map_err(|e| AgentError::Failed(e.to_string()))?;
    {
        let mut chat = host.chat.lock();
        chat.messages.push(ChatMessage {
            role: "user".into(),
            content: prompt.to_string(),
            tool_calls: Vec::new(),
            tool_call_id: String::new(),
            tool_name: String::new(),
        });
        trim_conversation(&mut chat);
    }
    save_chat(host);

    let mut reply = String::new();
    for _ in 0..MAX_TOOL_ROUNDS {
        let messages = host.chat.lock().messages.clone();
        let plan = plan_messages(&cfg, &api_key, &messages)?;
        let value = post_plan(&plan).await?;
        let turn = parse_turn(plan.backend, &value)?;
        if turn.tool_calls.is_empty() {
            reply = turn.content.clone();
            host.chat.lock().messages.push(assistant_message(turn));
            break;
        }
        let stop_for_confirm = turn
            .tool_calls
            .iter()
            .any(|call| call.name == "propose_write");
        if !turn.content.trim().is_empty() {
            reply.clone_from(&turn.content);
        }
        let calls = turn.tool_calls.clone();
        host.chat.lock().messages.push(assistant_message(turn));
        for call in calls {
            let result = if call.name == "search" {
                let args = arguments_object(&call.arguments);
                let query = args
                    .as_ref()
                    .map(|map| arg_str(map, "query"))
                    .unwrap_or_default();
                if args.is_none() {
                    "The tool arguments were not an object.".into()
                } else {
                    search_index(host.search.as_deref(), &query).await
                }
            } else {
                apply_tool(&mut host.chat.lock(), &call)
            };
            if reply.is_empty() {
                reply = clip_chars(&result, 500);
            }
            host.chat.lock().messages.push(ChatMessage {
                role: "tool".into(),
                content: result,
                tool_calls: Vec::new(),
                tool_call_id: call.id,
                tool_name: call.name,
            });
        }
        if stop_for_confirm {
            break;
        }
    }
    trim_conversation(&mut host.chat.lock());
    save_chat(host);
    if reply.trim().is_empty() {
        reply = "Stopped after the tool rounds.".into();
    }
    Ok(reply)
}

fn assistant_message(turn: ModelTurn) -> ChatMessage {
    ChatMessage {
        role: "assistant".into(),
        content: turn.content,
        tool_calls: turn.tool_calls,
        tool_call_id: String::new(),
        tool_name: String::new(),
    }
}

fn notify_widgets() {
    if let Some(hook) = *TURN_HOOK.lock() {
        hook();
    }
}

fn chat_path(dir: &Path) -> PathBuf {
    dir.join("agent-chat.json")
}

fn load_chat(dir: &Path) -> Conversation {
    let Ok(bytes) = std::fs::read(chat_path(dir)) else {
        return Conversation::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_chat(host: &AgentHost) {
    let chat = host.chat.lock().clone();
    let path = chat_path(&host.data_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&chat) {
        let _ = std::fs::write(path, text);
    }
}

fn trim_conversation(chat: &mut Conversation) {
    while chat.messages.len() > MAX_MESSAGES {
        chat.messages.remove(0);
    }
    while chat
        .messages
        .first()
        .is_some_and(|message| message.role == "tool")
    {
        chat.messages.remove(0);
    }
}

fn plan_messages(
    cfg: &AgentConfig,
    api_key: &str,
    messages: &[ChatMessage],
) -> Result<RequestPlan, AgentError> {
    if !cfg.enabled {
        return Err(AgentError::Disabled);
    }
    let model = cfg.model.trim();
    if model.is_empty() {
        return Err(AgentError::NeedsModel);
    }
    let backend = parse_backend(&cfg.backend);
    let base = endpoint_base(cfg)?;
    let api_messages = api_messages(backend, messages);
    let tools = tool_specs();
    let (path, body) = match backend {
        AgentBackend::Ollama => (
            "/api/chat",
            serde_json::json!({
                "model": model,
                "stream": false,
                "messages": api_messages,
                "tools": tools,
            }),
        ),
        AgentBackend::OpenAi => (
            "/chat/completions",
            serde_json::json!({
                "model": model,
                "messages": api_messages,
                "tools": tools,
            }),
        ),
    };
    Ok(RequestPlan {
        url: format!("{base}{path}"),
        body,
        bearer: bearer_token(api_key),
        backend,
    })
}

fn endpoint_base(cfg: &AgentConfig) -> Result<String, AgentError> {
    let raw = cfg.endpoint.trim();
    let raw = if raw.is_empty() {
        default_endpoint(&cfg.backend)
    } else {
        raw
    };
    let base = raw.trim_end_matches('/');
    let url = reqwest::Url::parse(base).map_err(|_| AgentError::BadEndpoint)?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(AgentError::BadEndpoint);
    }
    if url.host_str().is_none() {
        return Err(AgentError::BadEndpoint);
    }
    Ok(base.to_string())
}

fn bearer_token(api_key: &str) -> Option<String> {
    let key = api_key.trim();
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

fn api_messages(backend: AgentBackend, messages: &[ChatMessage]) -> Vec<Value> {
    let mut out = vec![serde_json::json!({
        "role": "system",
        "content": SYSTEM_PROMPT,
    })];
    for message in messages {
        out.push(api_message(backend, message));
    }
    out
}

fn api_message(backend: AgentBackend, message: &ChatMessage) -> Value {
    if message.role == "tool" {
        return match backend {
            AgentBackend::OpenAi => serde_json::json!({
                "role": "tool",
                "tool_call_id": message.tool_call_id,
                "content": message.content,
            }),
            AgentBackend::Ollama => serde_json::json!({
                "role": "tool",
                "tool_name": message.tool_name,
                "content": message.content,
            }),
        };
    }
    if message.role == "assistant" && !message.tool_calls.is_empty() {
        let calls: Vec<Value> = message
            .tool_calls
            .iter()
            .map(|call| match backend {
                AgentBackend::OpenAi => serde_json::json!({
                    "id": call.id,
                    "type": "function",
                    "function": {
                        "name": call.name,
                        "arguments": call.arguments,
                    },
                }),
                AgentBackend::Ollama => {
                    let arguments = serde_json::from_str::<Value>(&call.arguments)
                        .unwrap_or_else(|_| Value::Object(Map::new()));
                    serde_json::json!({
                        "function": {
                            "name": call.name,
                            "arguments": arguments,
                        },
                    })
                }
            })
            .collect();
        return serde_json::json!({
            "role": "assistant",
            "content": message.content,
            "tool_calls": calls,
        });
    }
    serde_json::json!({
        "role": message.role,
        "content": message.content,
    })
}

fn tool_specs() -> Value {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a local text file. path is absolute. Binary files return their size only. This does not write.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_dir",
                "description": "List one local directory. path is absolute. Directory names end with /.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "search",
                "description": "Search the local file index. Returns a few paths and snippets.",
                "parameters": {
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "propose_write",
                "description": "Propose replacing a file with content. Does not write. The user confirms. path is absolute and the parent directory must already exist.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }
            }
        }
    ])
}

struct ModelTurn {
    content: String,
    tool_calls: Vec<StoredToolCall>,
}

fn parse_turn(backend: AgentBackend, value: &Value) -> Result<ModelTurn, AgentError> {
    let message = match backend {
        AgentBackend::Ollama => value.get("message"),
        AgentBackend::OpenAi => value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message")),
    };
    let Some(message) = message else {
        return Err(AgentError::Failed("reply had no text".into()));
    };
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let tool_calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| parse_tool_calls(calls))
        .unwrap_or_default();
    if content.is_empty() && tool_calls.is_empty() {
        return Err(AgentError::Failed("reply had no text".into()));
    }
    Ok(ModelTurn {
        content,
        tool_calls,
    })
}

fn parse_tool_calls(calls: &[Value]) -> Vec<StoredToolCall> {
    calls
        .iter()
        .enumerate()
        .filter_map(|(index, call)| {
            let function = call.get("function")?;
            let name = function.get("name").and_then(Value::as_str)?.trim();
            if name.is_empty() {
                return None;
            }
            let arguments = function
                .get("arguments")
                .map(arguments_string)
                .unwrap_or_else(|| "{}".into());
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("call-{index}"));
            Some(StoredToolCall {
                id,
                name: name.to_string(),
                arguments,
            })
        })
        .collect()
}

fn arguments_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

async fn post_plan(plan: &RequestPlan) -> Result<Value, AgentError> {
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
    serde_json::from_slice(&bytes)
        .map_err(|e| AgentError::Failed(format!("reply was not JSON: {e}")))
}

fn apply_tool(chat: &mut Conversation, call: &StoredToolCall) -> String {
    let Some(args) = arguments_object(&call.arguments) else {
        return "The tool arguments were not an object.".into();
    };
    match call.name.as_str() {
        "read_file" => read_file(&arg_str(&args, "path")),
        "list_dir" => list_dir(&arg_str(&args, "path")),
        "search" => "The file index is not open.".into(),
        "propose_write" => propose_write(chat, &arg_str(&args, "path"), &arg_str(&args, "content")),
        other => format!("Unknown tool: {other}"),
    }
}

fn arguments_object(raw: &str) -> Option<Map<String, Value>> {
    let value: Value = serde_json::from_str(raw).ok()?;
    match value {
        Value::Object(map) => Some(map),
        Value::String(text) => match serde_json::from_str::<Value>(&text).ok()? {
            Value::Object(map) => Some(map),
            _ => None,
        },
        _ => None,
    }
}

fn arg_str(map: &Map<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn read_file(raw: &str) -> String {
    let path = match checked_file_path(raw) {
        Ok(path) => path,
        Err(reason) => return reason,
    };
    let meta = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(err) => return format!("Could not read the file: {err}"),
    };
    if meta.is_dir() {
        return "That path is a directory.".into();
    }
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(err) => return format!("Could not read the file: {err}"),
    };
    use std::io::Read;
    let mut limited = std::io::Read::take(file, u64::from(MAX_READ_BYTES as u32) + 1);
    let mut buf = Vec::new();
    let read = limited.read_to_end(&mut buf);
    if let Err(err) = read {
        return format!("Could not read the file: {err}");
    }
    if buf.contains(&0) {
        return format!("Binary file, {} bytes.", meta.len());
    }
    let truncated = buf.len() > MAX_READ_BYTES;
    if truncated {
        buf.truncate(MAX_READ_BYTES);
    }
    let Ok(text) = String::from_utf8(buf) else {
        return format!("The file is not UTF-8 text, {} bytes.", meta.len());
    };
    if truncated {
        format!("{text}\n… truncated")
    } else {
        text
    }
}

fn list_dir(raw: &str) -> String {
    let path = match checked_file_path(raw) {
        Ok(path) => path,
        Err(reason) => return reason,
    };
    let meta = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(err) => return format!("Could not list the directory: {err}"),
    };
    if !meta.is_dir() {
        return "That path is a file.".into();
    }
    let entries = match std::fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(err) => return format!("Could not list the directory: {err}"),
    };
    let mut names = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_dir {
            names.push(format!("{name}/"));
        } else {
            names.push(name);
        }
    }
    names.sort();
    let truncated = names.len() > MAX_LIST_NAMES;
    names.truncate(MAX_LIST_NAMES);
    if names.is_empty() {
        return "The directory is empty.".into();
    }
    let mut text = names.join("\n");
    if truncated {
        text.push_str("\n… truncated");
    }
    text
}

async fn search_index(search: Option<&SearchEngine>, query: &str) -> String {
    let Some(engine) = search else {
        return "The file index is not open.".into();
    };
    let query = query.trim();
    if query.is_empty() {
        return "The search query is empty.".into();
    }
    let query = clip_chars(query, 200);
    match engine.search_hybrid(&query, MAX_SEARCH_HITS).await {
        Ok(results) => format_hits(&results.hits),
        Err(err) => format!("Search failed: {err}"),
    }
}

fn format_hits(hits: &[orchid_search::SearchHit]) -> String {
    if hits.is_empty() {
        return "No files matched.".into();
    }
    let mut lines = Vec::new();
    for hit in hits.iter().take(MAX_SEARCH_HITS) {
        let snippet = hit
            .snippet
            .as_ref()
            .map(|snippet| clip_chars(&snippet.text.replace(['\n', '\r'], " "), 160))
            .unwrap_or_default();
        if snippet.is_empty() {
            lines.push(hit.path.clone());
        } else {
            lines.push(format!("{} — {snippet}", hit.path));
        }
    }
    lines.join("\n")
}

fn propose_write(chat: &mut Conversation, raw_path: &str, content: &str) -> String {
    if chat.pending.is_some() {
        return "A file write is already waiting for confirmation.".into();
    }
    let path = match checked_file_path(raw_path) {
        Ok(path) => path,
        Err(reason) => return reason,
    };
    if content.len() > MAX_WRITE_BYTES {
        return "The proposed file is too large.".into();
    }
    if path.is_dir() {
        return "That path is a directory.".into();
    }
    let Some(parent) = path.parent() else {
        return "The parent directory must already exist.".into();
    };
    if !parent.is_dir() {
        return "The parent directory must already exist.".into();
    }
    let shown = path.display().to_string();
    chat.pending = Some(PendingWrite {
        path: shown.clone(),
        content: content.to_string(),
    });
    format!("Write is waiting for confirmation: {shown}")
}

fn confirm_write(chat: &mut Conversation) -> Result<String, String> {
    let Some(pending) = chat.pending.clone() else {
        return Err("There is no file waiting to be written.".into());
    };
    let path = checked_file_path(&pending.path)?;
    if pending.content.len() > MAX_WRITE_BYTES {
        return Err("The proposed file is too large.".into());
    }
    if path.is_dir() {
        return Err("That path is a directory.".into());
    }
    let Some(parent) = path.parent() else {
        return Err("The parent directory must already exist.".into());
    };
    if !parent.is_dir() {
        return Err("The parent directory must already exist.".into());
    }
    if let Err(err) = std::fs::write(&path, &pending.content) {
        return Err(format!("Could not write the file: {err}"));
    }
    chat.pending = None;
    let shown = path.display().to_string();
    chat.messages.push(ChatMessage {
        role: "assistant".into(),
        content: format!("Wrote {shown}"),
        tool_calls: Vec::new(),
        tool_call_id: String::new(),
        tool_name: String::new(),
    });
    trim_conversation(chat);
    Ok(shown)
}

fn dismiss_write(chat: &mut Conversation) {
    if chat.pending.take().is_none() {
        return;
    }
    chat.messages.push(ChatMessage {
        role: "assistant".into(),
        content: "The pending write was dismissed.".into(),
        tool_calls: Vec::new(),
        tool_call_id: String::new(),
        tool_name: String::new(),
    });
    trim_conversation(chat);
}

fn checked_file_path(raw: &str) -> Result<PathBuf, String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.contains('\0') {
        return Err("The path is empty.".into());
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err("The path must be absolute.".into());
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("The path cannot contain '..'.".into());
    }
    Ok(path)
}

fn clip_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
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

    #[test]
    fn openai_tool_call_keeps_the_id_and_argument_string() {
        let value = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"C:\\\\tmp\\\\a.txt\"}"
                        }
                    }]
                }
            }]
        });
        let turn = parse_turn(AgentBackend::OpenAi, &value).unwrap();
        assert!(turn.content.is_empty());
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].id, "call_1");
        assert_eq!(turn.tool_calls[0].name, "read_file");
        let replay = api_message(AgentBackend::OpenAi, &assistant_message(turn));
        assert_eq!(replay["tool_calls"][0]["id"], "call_1");
        assert_eq!(
            replay["tool_calls"][0]["function"]["arguments"],
            "{\"path\":\"C:\\\\tmp\\\\a.txt\"}"
        );
    }

    #[test]
    fn ollama_tool_call_accepts_an_argument_object() {
        let value = serde_json::json!({
            "message": {
                "role": "assistant",
                "content": "looking",
                "tool_calls": [{
                    "function": {
                        "name": "list_dir",
                        "arguments": { "path": "C:\\tmp" }
                    }
                }]
            }
        });
        let turn = parse_turn(AgentBackend::Ollama, &value).unwrap();
        assert_eq!(turn.content, "looking");
        assert_eq!(turn.tool_calls[0].id, "call-0");
        assert_eq!(turn.tool_calls[0].name, "list_dir");
        let args = arguments_object(&turn.tool_calls[0].arguments).unwrap();
        assert_eq!(arg_str(&args, "path"), "C:\\tmp");
        let replay = api_message(
            AgentBackend::Ollama,
            &ChatMessage {
                role: "tool".into(),
                content: "a.txt".into(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                tool_name: "list_dir".into(),
            },
        );
        assert_eq!(replay["role"], "tool");
        assert_eq!(replay["tool_name"], "list_dir");
    }

    #[tokio::test]
    async fn propose_write_leaves_the_file_until_confirm() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        let mut chat = Conversation::default();
        let call = StoredToolCall {
            id: "1".into(),
            name: "propose_write".into(),
            arguments: serde_json::json!({
                "path": path.display().to_string(),
                "content": "hello"
            })
            .to_string(),
        };
        let result = apply_tool(&mut chat, &call);
        assert!(!path.exists(), "{result}");
        assert!(result.contains("waiting"));
        assert_eq!(chat.pending.as_ref().unwrap().content, "hello");
        let again = apply_tool(&mut chat, &call);
        assert!(again.contains("already waiting"));
        assert_eq!(
            confirm_write(&mut chat).unwrap(),
            path.display().to_string()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
        assert!(chat.pending.is_none());
        assert!(chat
            .messages
            .iter()
            .any(|message| message.content.starts_with("Wrote ")));
        assert!(confirm_write(&mut chat).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn dismiss_drops_the_proposal_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        let mut chat = Conversation::default();
        let call = StoredToolCall {
            id: "1".into(),
            name: "propose_write".into(),
            arguments: serde_json::json!({
                "path": path.display().to_string(),
                "content": "nope"
            })
            .to_string(),
        };
        apply_tool(&mut chat, &call);
        dismiss_write(&mut chat);
        assert!(!path.exists());
        assert!(chat.pending.is_none());
        assert!(confirm_write(&mut chat).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn propose_write_rejects_a_relative_path_and_a_missing_parent() {
        let mut chat = Conversation::default();
        let relative = StoredToolCall {
            id: "1".into(),
            name: "propose_write".into(),
            arguments: serde_json::json!({"path": "note.txt", "content": "x"}).to_string(),
        };
        let result = apply_tool(&mut chat, &relative);
        assert!(result.contains("absolute"));
        assert!(chat.pending.is_none());
        let missing = StoredToolCall {
            id: "2".into(),
            name: "propose_write".into(),
            arguments: serde_json::json!({
                "path": std::env::temp_dir().join("orchid-agent-missing").join("note.txt").display().to_string(),
                "content": "x"
            })
            .to_string(),
        };
        let result = apply_tool(&mut chat, &missing);
        assert!(result.contains("parent"));
        assert!(chat.pending.is_none());
    }

    #[test]
    fn read_file_returns_text_and_reports_binary_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let text_path = dir.path().join("a.txt");
        std::fs::write(&text_path, "orchid").unwrap();
        let binary_path = dir.path().join("a.bin");
        std::fs::write(&binary_path, [0, 1, 2, 0]).unwrap();
        let mut chat = Conversation::default();
        let read = apply_tool(
            &mut chat,
            &StoredToolCall {
                id: "1".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({"path": text_path.display().to_string()}).to_string(),
            },
        );
        assert_eq!(read, "orchid");
        let binary = apply_tool(
            &mut chat,
            &StoredToolCall {
                id: "2".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({"path": binary_path.display().to_string()})
                    .to_string(),
            },
        );
        assert!(binary.starts_with("Binary file"));
        assert_eq!(std::fs::read(&binary_path).unwrap(), [0, 1, 2, 0]);
        let listed = list_dir(&dir.path().display().to_string());
        assert!(listed.contains("a.txt"));
        assert!(!listed.contains("a.txt/"));
    }

    #[test]
    fn list_dir_suffixes_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("file.txt"), "x").unwrap();
        let listed = list_dir(&dir.path().display().to_string());
        assert!(listed.contains("sub/"));
        assert!(listed.contains("file.txt"));
    }

    #[tokio::test]
    async fn search_without_an_engine_says_the_index_is_closed() {
        let text = search_index(None, "orchid").await;
        assert!(text.contains("not open"));
    }

    #[test]
    fn trim_keeps_the_newest_messages_and_drops_a_leading_tool() {
        let mut chat = Conversation::default();
        for index in 0..(MAX_MESSAGES + 1) {
            chat.messages.push(ChatMessage {
                role: "user".into(),
                content: index.to_string(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                tool_name: String::new(),
            });
        }
        trim_conversation(&mut chat);
        assert_eq!(chat.messages.len(), MAX_MESSAGES);
        assert_eq!(chat.messages[0].content, "1");
        chat.messages.insert(
            0,
            ChatMessage {
                role: "tool".into(),
                content: "stale".into(),
                tool_calls: Vec::new(),
                tool_call_id: "x".into(),
                tool_name: "read_file".into(),
            },
        );
        trim_conversation(&mut chat);
        assert_ne!(chat.messages[0].role, "tool");
    }
}
