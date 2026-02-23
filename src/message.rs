use std::path::PathBuf;

use crossterm::event::KeyEvent;
use uuid::Uuid;

use crate::model::{OutputEntry, SessionStats, TerminalId};

#[derive(Debug)]
pub enum Message {
    Key(KeyEvent),
    MouseScroll { delta: i16 },
    Tick,

    // Session lifecycle
    WorktreeCreated { session_id: Uuid, path: PathBuf },
    WorktreeCreateFailed { session_id: Uuid, error: String },
    ClaudeSpawned { session_id: Uuid, claude_session_id: String },
    ClaudeSpawnFailed { session_id: Uuid, error: String },
    ClaudeOutput { session_id: Uuid, entry: OutputEntry },
    ClaudeStatsUpdate { session_id: Uuid, stats: SessionStats },
    ClaudeProcessExited { session_id: Uuid, exit_code: Option<i32> },

    // Git changes
    FullDiffLoaded {
        worktree_path: PathBuf,
        diff: String,
    },

    // Permission
    PermissionRequested {
        claude_session_id: String,
        request_id: String,
        tool_name: String,
        tool_input: String,
        request_path: PathBuf,
    },

    // Generate multi-agent
    GeneratePlanReceived {
        branch_name: String,
        agents: Vec<(String, String)>,
        skip_permissions: bool,
        context: Option<String>,
    },
    GeneratePlanFailed {
        error: String,
    },

    // Embedded terminal
    TerminalOutput { terminal_id: TerminalId, data: Vec<u8> },
    TerminalExited { terminal_id: TerminalId },
    Resize { cols: u16, rows: u16 },
}

#[derive(Debug)]
pub enum Action {
    Quit,
    CreateWorktree {
        session_id: Uuid,
        repo_root: PathBuf,
        branch: String,
        base: String,
    },
    SpawnClaude {
        session_id: Uuid,
        worktree_path: PathBuf,
        prompt: String,
        skip_permissions: bool,
    },
    ResumeClaude {
        session_id: Uuid,
        worktree_path: PathBuf,
        claude_session_id: String,
        message: String,
        skip_permissions: bool,
    },
    KillProcess {
        session_id: Uuid,
    },
    RemoveWorktree {
        worktree_path: PathBuf,
    },
    LoadFullDiff {
        worktree_path: PathBuf,
    },
    SpawnTerminal {
        worktree_path: PathBuf,
        command: Option<Vec<String>>,
    },
    TerminalInput {
        terminal_id: TerminalId,
        data: Vec<u8>,
    },
    DetachTerminal,
    KillTerminal {
        terminal_id: TerminalId,
    },
    DeleteNote {
        note_index: usize,
    },
    GeneratePlan {
        description: String,
        context: Option<String>,
        repo_root: PathBuf,
        skip_permissions: bool,
    },
    WritePermissionResponse {
        request_path: PathBuf,
        request_id: String,
        claude_session_id: String,
        allow: bool,
    },
    SaveState,
}
