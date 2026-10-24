//! Tool-use loop.

use crate::provider::{self, ChatMessage, ContentPart, ProviderConfig, Reply, Role};
use crate::AiError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the input object.
    pub input_schema: Value,
    /// The host must get explicit user approval before running this tool.
    pub requires_confirmation: bool,
}

/// Implemented by the application (see `ezyx-project::agent_host`).
pub trait ToolHost {
    fn tools(&self) -> Vec<ToolSpec>;
    /// Execute a tool. Return `Err(text)` for a tool error (sent back to the model).
    fn call(&mut self, name: &str, input: &Value) -> Result<String, String>;
    /// Ask the user to approve a confirmation-required tool. Default: deny.
    fn confirm(&mut self, _name: &str, _input: &Value) -> bool {
        false
    }
    /// Live progress line for the chat panel.
    fn on_event(&mut self, _event: &AgentEvent) {}
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    AssistantText(String),
    ToolCall { name: String, input: Value },
    ToolResult { name: String, ok: bool, output: String },
}

pub const DEFAULT_SYSTEM_PROMPT: &str = "You are the built-in assistant of EZ-YX-craft, an open-source CNC design and carving \
application for wood and metal (2D/2.5D/3D design, V-carving, reliefs, image-to-carve, rotary, \
G-code and machine control). Help beginners succeed safely: prefer conservative feeds and speeds, \
explain choices briefly, and use the provided tools to actually perform actions in the user's \
project instead of describing clicks. Units are millimetres unless the user says otherwise. \
Never claim a tool succeeded unless its result says so. Anything that moves the machine requires \
user confirmation; the app enforces this.";

pub struct Agent {
    pub provider: ProviderConfig,
    pub system: String,
    pub history: Vec<ChatMessage>,
    pub max_steps: usize,
}

impl Agent {
    pub fn new(provider: ProviderConfig) -> Self {
        Self { provider, system: DEFAULT_SYSTEM_PROMPT.into(), history: vec![], max_steps: 12 }
    }

    /// One user turn: loops through tool calls until the model answers in text.
    pub fn run_turn(&mut self, user: ChatMessage, host: &mut dyn ToolHost) -> Result<String, AiError> {
        self.run_turn_with(user, host, |cfg, sys, msgs, tools| provider::send(cfg, sys, msgs, tools))
    }

    /// Same as [`run_turn`] with an injectable transport (tests, offline replay).
    pub fn run_turn_with(
        &mut self,
        user: ChatMessage,
        host: &mut dyn ToolHost,
        mut send: impl FnMut(&ProviderConfig, &str, &[ChatMessage], &[ToolSpec]) -> Result<Reply, AiError>,
    ) -> Result<String, AiError> {
        self.history.push(user);
        let tools = host.tools();
        for _ in 0..self.max_steps {
            let reply = send(&self.provider, &self.system, &self.history, &tools)?;
            let text = reply.message.text();
            if !text.is_empty() {
                host.on_event(&AgentEvent::AssistantText(text.clone()));
            }
            let uses = reply.message.tool_uses();
            self.history.push(reply.message);
            if uses.is_empty() || !reply.wants_tools {
                return Ok(text);
            }
            let mut results = Vec::new();
            for (id, name, input) in uses {
                host.on_event(&AgentEvent::ToolCall { name: name.clone(), input: input.clone() });
                let spec = tools.iter().find(|t| t.name == name);
                let outcome = match spec {
                    None => Err(format!("unknown tool '{name}'")),
                    Some(s) if s.requires_confirmation && !host.confirm(&name, &input) => Err("The user declined this action.".to_string()),
                    Some(_) => host.call(&name, &input),
                };
                let (ok, content) = match outcome {
                    Ok(s) => (true, s),
                    Err(e) => (false, e),
                };
                host.on_event(&AgentEvent::ToolResult { name: name.clone(), ok, output: content.clone() });
                results.push(ContentPart::ToolResult { id, content, is_error: !ok });
            }
            self.history.push(ChatMessage { role: Role::User, content: results });
        }
        Err(AiError::TooManySteps(self.max_steps))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Host {
        circles: Vec<f64>,
    }
    impl ToolHost for Host {
        fn tools(&self) -> Vec<ToolSpec> {
            vec![
                ToolSpec { name: "add_circle".into(), description: "".into(), input_schema: json!({"type": "object"}), requires_confirmation: false },
                ToolSpec { name: "run_job".into(), description: "".into(), input_schema: json!({"type": "object"}), requires_confirmation: true },
            ]
        }
        fn call(&mut self, name: &str, input: &Value) -> Result<String, String> {
            match name {
                "add_circle" => {
                    self.circles.push(input["r"].as_f64().unwrap_or(1.0));
                    Ok("added".into())
                }
                _ => Err("nope".into()),
            }
        }
    }

    #[test]
    fn loop_executes_tools_and_respects_confirmation() {
        let mut agent = Agent::new(ProviderConfig::ollama("test"));
        let mut host = Host { circles: vec![] };
        let mut step = 0;
        let out = agent
            .run_turn_with(ChatMessage::user("make a 5mm circle then run"), &mut host, |_, _, msgs, _| {
                step += 1;
                Ok(match step {
                    1 => Reply {
                        message: ChatMessage {
                            role: Role::Assistant,
                            content: vec![
                                ContentPart::ToolUse { id: "a".into(), name: "add_circle".into(), input: json!({"r": 5.0}) },
                                ContentPart::ToolUse { id: "b".into(), name: "run_job".into(), input: json!({}) },
                            ],
                        },
                        wants_tools: true,
                    },
                    _ => {
                        // The model must have received a declined result for run_job.
                        let last = msgs.last().unwrap();
                        assert!(matches!(&last.content[1], ContentPart::ToolResult { is_error: true, .. }));
                        Reply {
                            message: ChatMessage { role: Role::Assistant, content: vec![ContentPart::Text("Circle added; job not started.".into())] },
                            wants_tools: false,
                        }
                    }
                })
            })
            .unwrap();
        assert_eq!(host.circles, vec![5.0]);
        assert!(out.contains("Circle added"));
    }
}
