use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::claude::stream::{ContentBlock, StreamEvent};
use crate::message::Message;
use crate::model::{OutputEntry, OutputKind, SessionStats};

/// Spawn a Claude process with `--output-format stream-json` and pipe output
/// back as Messages. Returns the child process handle for lifecycle management.
pub async fn spawn_claude(
    session_id: Uuid,
    worktree_path: &Path,
    prompt: &str,
    skip_permissions: bool,
    resume_session_id: Option<&str>,
    mcp_config_path: Option<&Path>,
    tx: mpsc::UnboundedSender<Message>,
) -> Result<tokio::process::Child> {
    let mut cmd = Command::new("claude");
    cmd.arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .current_dir(worktree_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if skip_permissions {
        cmd.arg("--dangerously-skip-permissions");
    }

    if let Some(sid) = resume_session_id {
        cmd.arg("--resume").arg(sid);
    }

    if let Some(config_path) = mcp_config_path {
        if config_path.exists() {
            cmd.arg("--mcp-config").arg(config_path);
        }
    }

    let mut child = cmd.spawn()?;

    let stdout = child.stdout.take().expect("stdout was piped");
    let tx_clone = tx.clone();
    let sid = session_id;

    // Spawn reader task for stdout
    tokio::spawn(async move {
        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<StreamEvent>(&line) {
                Ok(event) => {
                    // Extract session_id from system init event
                    if event.event_type == "system" && event.session_id.is_some() {
                        let claude_sid = event.session_id.unwrap();
                        let _ = tx_clone.send(Message::ClaudeSpawned {
                            session_id: sid,
                            claude_session_id: claude_sid,
                        });
                    }

                    // Parse assistant messages into output entries
                    if event.event_type == "assistant" {
                        if let Some(msg) = &event.message {
                            for block in &msg.content {
                                let entry = match block {
                                    ContentBlock::Text { text } => OutputEntry {
                                        timestamp: Instant::now(),
                                        kind: OutputKind::AssistantText(text.clone()),
                                    },
                                    ContentBlock::ToolUse { name, input, .. } => {
                                        let summary = summarize_json(input);
                                        OutputEntry {
                                            timestamp: Instant::now(),
                                            kind: OutputKind::ToolUse {
                                                name: name.clone(),
                                                input_summary: summary,
                                            },
                                        }
                                    }
                                    ContentBlock::ToolResult {
                                        content, ..
                                    } => {
                                        let summary = summarize_json(content);
                                        OutputEntry {
                                            timestamp: Instant::now(),
                                            kind: OutputKind::ToolResult {
                                                tool_name: String::new(),
                                                output_summary: summary,
                                                success: true,
                                            },
                                        }
                                    }
                                    ContentBlock::Thinking { .. }
                                    | ContentBlock::Unknown => continue,
                                };
                                let _ = tx_clone.send(Message::ClaudeOutput {
                                    session_id: sid,
                                    entry,
                                });
                            }
                        }
                    }

                    // Parse result events for stats
                    if event.event_type == "result" {
                        let stats = SessionStats {
                            num_turns: event.num_turns.unwrap_or(0),
                            cost_usd: event.cost_usd.unwrap_or(0.0),
                            last_activity: Some(Instant::now()),
                            last_activity_summary: "Completed".to_string(),
                        };
                        let _ = tx_clone.send(Message::ClaudeStatsUpdate {
                            session_id: sid,
                            stats,
                        });
                    }
                }
                Err(e) => {
                    let _ = tx_clone.send(Message::ClaudeOutput {
                        session_id: sid,
                        entry: OutputEntry {
                            timestamp: Instant::now(),
                            kind: OutputKind::Error(format!("Parse error: {}", e)),
                        },
                    });
                }
            }
        }
    });

    Ok(child)
}

fn summarize_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => {
            if s.chars().count() > 80 {
                let truncated: String = s.chars().take(80).collect();
                format!("{}...", truncated)
            } else {
                s.clone()
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(cmd) = map.get("command") {
                return summarize_json(cmd);
            }
            if let Some(path) = map.get("file_path") {
                return summarize_json(path);
            }
            format!("{{{} keys}}", map.len())
        }
        _ => value.to_string(),
    }
}
