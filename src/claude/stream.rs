#![allow(dead_code)]

use serde::Deserialize;

/// Raw events from Claude's `--output-format stream-json` NDJSON output.
#[derive(Debug, Deserialize)]
pub struct StreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub message: Option<StreamMessage>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub num_turns: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct StreamMessage {
    #[serde(default)]
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
    },
    #[serde(rename = "thinking")]
    Thinking {
        #[serde(default)]
        thinking: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        #[serde(default)]
        id: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        #[serde(default)]
        tool_use_id: String,
        #[serde(default)]
        content: serde_json::Value,
    },
    #[serde(other)]
    Unknown,
}
