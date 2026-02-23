use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{
    App, OutputEntry, OutputKind, Session, SessionStats, SessionStatus,
};

#[derive(Serialize, Deserialize)]
struct PersistedState {
    sessions: Vec<PersistedSession>,
}

#[derive(Serialize, Deserialize)]
struct PersistedSession {
    id: Uuid,
    branch_name: String,
    worktree_path: PathBuf,
    status: PersistedStatus,
    claude_session_id: Option<String>,
    output_log: Vec<PersistedOutput>,
    num_turns: u32,
    cost_usd: f64,
    last_activity_summary: String,
    skip_permissions: bool,
    #[serde(default = "default_created_at")]
    created_at: String,
    #[serde(default)]
    agent_label: Option<String>,
    #[serde(default)]
    parent_session_id: Option<Uuid>,
}

fn default_created_at() -> String {
    Utc::now().to_rfc3339()
}

#[derive(Serialize, Deserialize)]
enum PersistedStatus {
    Idle,
    Completed,
    Error(String),
}

#[derive(Serialize, Deserialize)]
enum PersistedOutput {
    AssistantText(String),
    ToolUse { name: String, input_summary: String },
    ToolResult { tool_name: String, output_summary: String, success: bool },
    UserMessage(String),
    SystemMessage(String),
    Error(String),
}

fn state_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".knute").join("state.json")
}

pub fn save_state(app: &App) {
    let persisted = PersistedState {
        sessions: app
            .sessions
            .iter()
            .filter(|s| !matches!(s.status, SessionStatus::Creating))
            .map(|s| PersistedSession {
                id: s.id,
                branch_name: s.branch_name.clone(),
                worktree_path: s.worktree_path.clone(),
                status: match &s.status {
                    SessionStatus::Completed => PersistedStatus::Completed,
                    SessionStatus::Error(e) => PersistedStatus::Error(e.clone()),
                    _ => PersistedStatus::Idle,
                },
                claude_session_id: s.claude_session_id.clone(),
                output_log: s
                    .output_log
                    .iter()
                    .map(|e| match &e.kind {
                        OutputKind::AssistantText(t) => PersistedOutput::AssistantText(t.clone()),
                        OutputKind::ToolUse { name, input_summary } => {
                            PersistedOutput::ToolUse {
                                name: name.clone(),
                                input_summary: input_summary.clone(),
                            }
                        }
                        OutputKind::ToolResult {
                            tool_name,
                            output_summary,
                            success,
                        } => PersistedOutput::ToolResult {
                            tool_name: tool_name.clone(),
                            output_summary: output_summary.clone(),
                            success: *success,
                        },
                        OutputKind::UserMessage(t) => PersistedOutput::UserMessage(t.clone()),
                        OutputKind::SystemMessage(t) => PersistedOutput::SystemMessage(t.clone()),
                        OutputKind::Error(t) => PersistedOutput::Error(t.clone()),
                    })
                    .collect(),
                num_turns: s.stats.num_turns,
                cost_usd: s.stats.cost_usd,
                last_activity_summary: s.stats.last_activity_summary.clone(),
                skip_permissions: s.skip_permissions,
                created_at: s.created_at.to_rfc3339(),
                agent_label: s.agent_label.clone(),
                parent_session_id: s.parent_session_id,
            })
            .collect(),
    };

    let path = state_path(&app.repo_root);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&persisted) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn load_state(repo_root: &Path) -> Vec<Session> {
    let path = state_path(repo_root);
    let Ok(data) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(persisted) = serde_json::from_str::<PersistedState>(&data) else {
        return Vec::new();
    };

    let now = Instant::now();

    persisted
        .sessions
        .into_iter()
        .filter(|s| s.worktree_path.exists())
        .map(|s| {
            let created_at = DateTime::parse_from_rfc3339(&s.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            Session {
                id: s.id,
                branch_name: s.branch_name,
                worktree_path: s.worktree_path,
                status: match s.status {
                    PersistedStatus::Idle => SessionStatus::Idle,
                    PersistedStatus::Completed => SessionStatus::Completed,
                    PersistedStatus::Error(e) => SessionStatus::Error(e),
                },
                claude_session_id: s.claude_session_id,
                output_log: s
                    .output_log
                    .into_iter()
                    .map(|e| OutputEntry {
                        timestamp: now,
                        kind: match e {
                            PersistedOutput::AssistantText(t) => OutputKind::AssistantText(t),
                            PersistedOutput::ToolUse { name, input_summary } => {
                                OutputKind::ToolUse { name, input_summary }
                            }
                            PersistedOutput::ToolResult {
                                tool_name,
                                output_summary,
                                success,
                            } => OutputKind::ToolResult {
                                tool_name,
                                output_summary,
                                success,
                            },
                            PersistedOutput::UserMessage(t) => OutputKind::UserMessage(t),
                            PersistedOutput::SystemMessage(t) => OutputKind::SystemMessage(t),
                            PersistedOutput::Error(t) => OutputKind::Error(t),
                        },
                    })
                    .collect(),
                created_at,
                stats: SessionStats {
                    num_turns: s.num_turns,
                    cost_usd: s.cost_usd,
                    last_activity: None,
                    last_activity_summary: s.last_activity_summary,
                },
                scroll_offset: 0,
                auto_scroll: true,
                rendered_max_scroll: 0,
                pending_prompt: None,
                skip_permissions: s.skip_permissions,
                agent_label: s.agent_label,
                parent_session_id: s.parent_session_id,
                pending_permission_count: 0,
            }
        })
        .collect()
}
