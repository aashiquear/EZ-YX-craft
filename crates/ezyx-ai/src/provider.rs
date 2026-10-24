//! Providers: request building and response parsing are pure functions (unit-tested without
//! network); `send` does the HTTP call.

use crate::{agent::ToolSpec, AiError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
    /// Any OpenAI-compatible server (Ollama: http://localhost:11434/v1).
    OpenAiCompatible,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub model: String,
    pub base_url: String,
    /// Environment variable holding the key (ignored when `api_key` is set).
    pub api_key_env: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl ProviderConfig {
    pub fn anthropic(model: &str) -> Self {
        Self {
            kind: ProviderKind::Anthropic,
            model: model.into(),
            base_url: "https://api.anthropic.com".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            api_key: None,
            max_tokens: 4096,
            temperature: 0.3,
        }
    }
    pub fn openai(model: &str) -> Self {
        Self {
            kind: ProviderKind::OpenAi,
            model: model.into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            api_key: None,
            max_tokens: 4096,
            temperature: 0.3,
        }
    }
    pub fn ollama(model: &str) -> Self {
        Self {
            kind: ProviderKind::OpenAiCompatible,
            model: model.into(),
            base_url: "http://localhost:11434/v1".into(),
            api_key_env: String::new(),
            api_key: None,
            max_tokens: 4096,
            temperature: 0.3,
        }
    }
    fn key(&self) -> Result<Option<String>, AiError> {
        if let Some(k) = &self.api_key {
            if !k.is_empty() {
                return Ok(Some(k.clone()));
            }
        }
        if self.api_key_env.is_empty() {
            return Ok(None);
        }
        match std::env::var(&self.api_key_env) {
            Ok(k) if !k.is_empty() => Ok(Some(k)),
            _ if self.kind == ProviderKind::OpenAiCompatible => Ok(None),
            _ => Err(AiError::MissingKey(self.api_key_env.clone())),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        // Model ids change over time; users pick in Settings.
        Self::anthropic("claude-sonnet-5-5")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContentPart {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        content: String,
        is_error: bool,
    },
    /// PNG/JPEG for vision-capable models (e.g. "make this sketch carvable").
    Image {
        media_type: String,
        base64: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Vec<ContentPart>,
}

impl ChatMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, content: vec![ContentPart::Text(text.into())] }
    }
    pub fn text(&self) -> String {
        self.content.iter().filter_map(|c| if let ContentPart::Text(t) = c { Some(t.as_str()) } else { None }).collect::<Vec<_>>().join("\n")
    }
    pub fn tool_uses(&self) -> Vec<(String, String, Value)> {
        self.content
            .iter()
            .filter_map(|c| if let ContentPart::ToolUse { id, name, input } = c { Some((id.clone(), name.clone(), input.clone())) } else { None })
            .collect()
    }
}

/// Provider reply normalised to our message type.
#[derive(Debug, Clone)]
pub struct Reply {
    pub message: ChatMessage,
    pub wants_tools: bool,
}

// ------------------------------------------------------------------ Anthropic

pub fn anthropic_request(cfg: &ProviderConfig, system: &str, msgs: &[ChatMessage], tools: &[ToolSpec]) -> Value {
    let messages: Vec<Value> = msgs
        .iter()
        .map(|m| {
            let content: Vec<Value> = m
                .content
                .iter()
                .map(|c| match c {
                    ContentPart::Text(t) => json!({"type": "text", "text": t}),
                    ContentPart::ToolUse { id, name, input } => json!({"type": "tool_use", "id": id, "name": name, "input": input}),
                    ContentPart::ToolResult { id, content, is_error } => {
                        json!({"type": "tool_result", "tool_use_id": id, "content": content, "is_error": is_error})
                    }
                    ContentPart::Image { media_type, base64 } => {
                        json!({"type": "image", "source": {"type": "base64", "media_type": media_type, "data": base64}})
                    }
                })
                .collect();
            json!({"role": if m.role == Role::User { "user" } else { "assistant" }, "content": content})
        })
        .collect();
    let mut body = json!({
        "model": cfg.model,
        "max_tokens": cfg.max_tokens,
        "temperature": cfg.temperature,
        "system": system,
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.iter().map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema})).collect());
    }
    body
}

pub fn anthropic_parse(v: &Value) -> Result<Reply, AiError> {
    let blocks = v["content"].as_array().ok_or_else(|| AiError::Parse(v.to_string()))?;
    let mut content = Vec::new();
    for b in blocks {
        match b["type"].as_str() {
            Some("text") => content.push(ContentPart::Text(b["text"].as_str().unwrap_or("").into())),
            Some("tool_use") => content.push(ContentPart::ToolUse {
                id: b["id"].as_str().unwrap_or("").into(),
                name: b["name"].as_str().unwrap_or("").into(),
                input: b["input"].clone(),
            }),
            _ => {}
        }
    }
    let wants_tools = v["stop_reason"].as_str() == Some("tool_use");
    Ok(Reply { message: ChatMessage { role: Role::Assistant, content }, wants_tools })
}

// ------------------------------------------------------------------ OpenAI

pub fn openai_request(cfg: &ProviderConfig, system: &str, msgs: &[ChatMessage], tools: &[ToolSpec]) -> Value {
    let mut messages = vec![json!({"role": "system", "content": system})];
    for m in msgs {
        match m.role {
            Role::User => {
                // Tool results become separate "tool" messages.
                let mut parts = Vec::new();
                for c in &m.content {
                    match c {
                        ContentPart::ToolResult { id, content, .. } => messages.push(json!({"role": "tool", "tool_call_id": id, "content": content})),
                        ContentPart::Text(t) => parts.push(json!({"type": "text", "text": t})),
                        ContentPart::Image { media_type, base64 } => {
                            parts.push(json!({"type": "image_url", "image_url": {"url": format!("data:{media_type};base64,{base64}")}}))
                        }
                        ContentPart::ToolUse { .. } => {}
                    }
                }
                if !parts.is_empty() {
                    messages.push(json!({"role": "user", "content": parts}));
                }
            }
            Role::Assistant => {
                let text = m.text();
                let calls: Vec<Value> = m
                    .tool_uses()
                    .into_iter()
                    .map(|(id, name, input)| json!({"id": id, "type": "function", "function": {"name": name, "arguments": input.to_string()}}))
                    .collect();
                let mut msg = json!({"role": "assistant", "content": text});
                if !calls.is_empty() {
                    msg["tool_calls"] = Value::Array(calls);
                }
                messages.push(msg);
            }
        }
    }
    let mut body = json!({"model": cfg.model, "messages": messages, "temperature": cfg.temperature, "max_tokens": cfg.max_tokens});
    if !tools.is_empty() {
        body["tools"] = Value::Array(
            tools
                .iter()
                .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema}}))
                .collect(),
        );
    }
    body
}

pub fn openai_parse(v: &Value) -> Result<Reply, AiError> {
    let msg = &v["choices"][0]["message"];
    if msg.is_null() {
        return Err(AiError::Parse(v.to_string()));
    }
    let mut content = Vec::new();
    if let Some(t) = msg["content"].as_str() {
        if !t.is_empty() {
            content.push(ContentPart::Text(t.into()));
        }
    }
    if let Some(calls) = msg["tool_calls"].as_array() {
        for c in calls {
            let args = c["function"]["arguments"].as_str().unwrap_or("{}");
            content.push(ContentPart::ToolUse {
                id: c["id"].as_str().unwrap_or("").into(),
                name: c["function"]["name"].as_str().unwrap_or("").into(),
                input: serde_json::from_str(args).unwrap_or(json!({})),
            });
        }
    }
    let wants_tools = content.iter().any(|c| matches!(c, ContentPart::ToolUse { .. }));
    Ok(Reply { message: ChatMessage { role: Role::Assistant, content }, wants_tools })
}

// ------------------------------------------------------------------ HTTP

pub fn send(cfg: &ProviderConfig, system: &str, msgs: &[ChatMessage], tools: &[ToolSpec]) -> Result<Reply, AiError> {
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(180))).build().into();
    let key = cfg.key()?;
    let (url, body) = match cfg.kind {
        ProviderKind::Anthropic => (format!("{}/v1/messages", cfg.base_url.trim_end_matches('/')), anthropic_request(cfg, system, msgs, tools)),
        _ => (format!("{}/chat/completions", cfg.base_url.trim_end_matches('/')), openai_request(cfg, system, msgs, tools)),
    };
    let mut req = agent.post(&url).header("content-type", "application/json");
    match cfg.kind {
        ProviderKind::Anthropic => {
            req = req.header("x-api-key", key.as_deref().unwrap_or("")).header("anthropic-version", "2023-06-01");
        }
        _ => {
            if let Some(k) = &key {
                req = req.header("authorization", &format!("Bearer {k}"));
            }
        }
    }
    let mut resp = req.send_json(&body).map_err(|e| AiError::Http(e.to_string()))?;
    let status = resp.status().as_u16();
    let text = resp.body_mut().read_to_string().map_err(|e| AiError::Http(e.to_string()))?;
    if status >= 400 {
        return Err(AiError::Status { status, body: text.chars().take(2000).collect() });
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| AiError::Parse(format!("{e}: {text}")))?;
    match cfg.kind {
        ProviderKind::Anthropic => anthropic_parse(&v),
        _ => openai_parse(&v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> ToolSpec {
        ToolSpec {
            name: "add_circle".into(),
            description: "Add a circle".into(),
            input_schema: json!({"type": "object", "properties": {"r": {"type": "number"}}}),
            requires_confirmation: false,
        }
    }

    #[test]
    fn anthropic_roundtrip() {
        let cfg = ProviderConfig::anthropic("m");
        let req = anthropic_request(&cfg, "sys", &[ChatMessage::user("hi")], &[tool()]);
        assert_eq!(req["tools"][0]["name"], "add_circle");
        assert_eq!(req["messages"][0]["content"][0]["text"], "hi");
        let resp = json!({"content": [{"type": "text", "text": "ok"}, {"type": "tool_use", "id": "t1", "name": "add_circle", "input": {"r": 5}}], "stop_reason": "tool_use"});
        let r = anthropic_parse(&resp).unwrap();
        assert!(r.wants_tools);
        assert_eq!(r.message.tool_uses()[0].2["r"], 5);
    }

    #[test]
    fn openai_roundtrip() {
        let cfg = ProviderConfig::openai("m");
        let msgs = vec![
            ChatMessage::user("hi"),
            ChatMessage { role: Role::Assistant, content: vec![ContentPart::ToolUse { id: "c1".into(), name: "add_circle".into(), input: json!({"r": 2}) }] },
            ChatMessage { role: Role::User, content: vec![ContentPart::ToolResult { id: "c1".into(), content: "done".into(), is_error: false }] },
        ];
        let req = openai_request(&cfg, "sys", &msgs, &[tool()]);
        assert_eq!(req["messages"][0]["role"], "system");
        assert_eq!(req["messages"][2]["tool_calls"][0]["function"]["name"], "add_circle");
        assert_eq!(req["messages"][3]["role"], "tool");
        let resp = json!({"choices": [{"message": {"content": null, "tool_calls": [{"id": "c2", "type": "function", "function": {"name": "add_circle", "arguments": "{\"r\":3}"}}]}}]});
        let r = openai_parse(&resp).unwrap();
        assert!(r.wants_tools);
        assert_eq!(r.message.tool_uses()[0].2["r"], 3);
    }
}
