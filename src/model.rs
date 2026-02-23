use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::{DateTime, Utc};
use ratatui::widgets::ListState;
use uuid::Uuid;

pub type TerminalId = u32;

pub struct App {
    pub sessions: Vec<Session>,
    pub notes: Vec<Note>,
    pub selected_index: usize,
    pub mode: AppMode,
    pub repo_root: PathBuf,
    pub dialog: Option<Dialog>,
    pub should_quit: bool,
    pub input_buffer: String,
    pub notification: Option<Notification>,
    pub sidebar_state: ListState,
    pub sidebar_focused: bool,
    pub new_session_form: NewSessionForm,
    pub new_note_form: NewNoteForm,
    pub sub_agent_form: SubAgentForm,
    pub file_cache: Vec<String>,
    pub autocomplete: Option<Autocomplete>,
    pub generate_form: GenerateForm,
    pub pending_generate_agents: Vec<PendingGenerateAgent>,
    pub pending_permissions: VecDeque<PendingPermission>,
    pub content_height: usize,
    pub worktree_view: WorktreeViewState,
    pub terminals: Vec<EmbeddedTerminalState>,
    pub next_terminal_id: TerminalId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    NewSession,
    WorktreeView { worktree_path: PathBuf },
    SessionChat { session_id: Uuid },
    SessionChatInput { session_id: Uuid },
    NewSubAgent { worktree_path: PathBuf },
    Generate,
    NoteView { note_index: usize },
    NewNote,
    EmbeddedTerminal { worktree_path: PathBuf, terminal_id: TerminalId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeTab {
    Diff,
    Agents,
    Terminals,
}

pub struct WorktreeGroup {
    pub worktree_path: PathBuf,
    pub branch_name: String,
    pub session_ids: Vec<Uuid>,
}

pub struct WorktreeViewState {
    pub active_tab: WorktreeTab,
    pub selected_agent: usize,
    pub selected_terminal: usize,
    pub changes: ChangesState,
}

impl Default for WorktreeViewState {
    fn default() -> Self {
        Self {
            active_tab: WorktreeTab::Diff,
            selected_agent: 0,
            selected_terminal: 0,
            changes: ChangesState::default(),
        }
    }
}

pub struct Session {
    pub id: Uuid,
    pub branch_name: String,
    pub worktree_path: PathBuf,
    pub status: SessionStatus,
    pub claude_session_id: Option<String>,
    pub output_log: Vec<OutputEntry>,
    pub created_at: DateTime<Utc>,
    pub stats: SessionStats,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub rendered_max_scroll: usize,
    pub pending_prompt: Option<String>,
    pub skip_permissions: bool,
    pub agent_label: Option<String>,
    pub parent_session_id: Option<Uuid>,
    pub pending_permission_count: usize,
}

impl Session {
    pub fn needs_attention(&self) -> bool {
        matches!(self.status, SessionStatus::Completed | SessionStatus::Idle)
            && self.claude_session_id.is_some()
            && !self.output_log.is_empty()
    }

}

#[derive(Debug, Clone, Default)]
pub struct ChangesState {
    pub diff_content: String,
    pub diff_scroll: usize,
    pub diff_line_count: usize,
}

impl ChangesState {
    pub fn set_diff_content(&mut self, content: String) {
        self.diff_line_count = content.lines().count();
        self.diff_content = content;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    Creating,
    Idle,
    Working,
    Completed,
    Error(String),
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Creating => write!(f, "Creating"),
            SessionStatus::Idle => write!(f, "Idle"),
            SessionStatus::Working => write!(f, "Working"),
            SessionStatus::Completed => write!(f, "Done"),
            SessionStatus::Error(_) => write!(f, "Error"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputEntry {
    pub timestamp: Instant,
    pub kind: OutputKind,
}

#[derive(Debug, Clone)]
pub enum OutputKind {
    AssistantText(String),
    ToolUse { name: String, input_summary: String },
    ToolResult { tool_name: String, output_summary: String, success: bool },
    UserMessage(String),
    SystemMessage(String),
    Error(String),
}

#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    pub num_turns: u32,
    pub cost_usd: f64,
    pub last_activity: Option<Instant>,
    pub last_activity_summary: String,
}

#[derive(Debug, Clone)]
pub enum Dialog {
    Confirm {
        title: String,
        message: String,
        on_confirm: DialogAction,
    },
    Help,
    Permission {
        request_id: String,
        claude_session_id: String,
        tool_name: String,
        tool_input: String,
        request_path: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub struct NewSessionForm {
    pub branch_name: String,
    pub base_branch: String,
    pub initial_prompt: String,
    pub skip_permissions: bool,
    pub focused_field: FormField,
}

impl Default for NewSessionForm {
    fn default() -> Self {
        Self {
            branch_name: String::new(),
            base_branch: "main".to_string(),
            initial_prompt: String::new(),
            skip_permissions: false,
            focused_field: FormField::BranchName,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormField {
    BranchName,
    BaseBranch,
    Prompt,
    SkipPermissions,
    Submit,
}

impl FormField {
    pub fn next(&self) -> FormField {
        match self {
            FormField::BranchName => FormField::BaseBranch,
            FormField::BaseBranch => FormField::Prompt,
            FormField::Prompt => FormField::SkipPermissions,
            FormField::SkipPermissions => FormField::Submit,
            FormField::Submit => FormField::BranchName,
        }
    }

    pub fn prev(&self) -> FormField {
        match self {
            FormField::BranchName => FormField::Submit,
            FormField::BaseBranch => FormField::BranchName,
            FormField::Prompt => FormField::BaseBranch,
            FormField::SkipPermissions => FormField::Prompt,
            FormField::Submit => FormField::SkipPermissions,
        }
    }

    pub fn is_text_field(&self) -> bool {
        matches!(self, FormField::BranchName | FormField::BaseBranch | FormField::Prompt)
    }
}

#[derive(Debug, Clone)]
pub struct SubAgentForm {
    pub label: String,
    pub initial_prompt: String,
    pub skip_permissions: bool,
    pub focused_field: SubAgentFormField,
}

impl Default for SubAgentForm {
    fn default() -> Self {
        Self {
            label: String::new(),
            initial_prompt: String::new(),
            skip_permissions: false,
            focused_field: SubAgentFormField::Label,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubAgentFormField {
    Label,
    Prompt,
    SkipPermissions,
    Submit,
}

impl SubAgentFormField {
    pub fn next(&self) -> SubAgentFormField {
        match self {
            SubAgentFormField::Label => SubAgentFormField::Prompt,
            SubAgentFormField::Prompt => SubAgentFormField::SkipPermissions,
            SubAgentFormField::SkipPermissions => SubAgentFormField::Submit,
            SubAgentFormField::Submit => SubAgentFormField::Label,
        }
    }

    pub fn prev(&self) -> SubAgentFormField {
        match self {
            SubAgentFormField::Label => SubAgentFormField::Submit,
            SubAgentFormField::Prompt => SubAgentFormField::Label,
            SubAgentFormField::SkipPermissions => SubAgentFormField::Prompt,
            SubAgentFormField::Submit => SubAgentFormField::SkipPermissions,
        }
    }

    pub fn is_text_field(&self) -> bool {
        matches!(self, SubAgentFormField::Label | SubAgentFormField::Prompt)
    }
}

#[derive(Debug, Clone)]
pub enum DialogAction {
    DeleteWorktree(PathBuf),
    DeleteNote { note_index: usize },
    QuitApp,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    pub created_at: Instant,
    pub duration_secs: u64,
}

#[derive(Debug, Clone)]
pub enum NotificationLevel {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct GenerateForm {
    pub description: String,
    pub context: String,
    pub skip_permissions: bool,
    pub focused_field: GenerateFormField,
    pub waiting: bool,
    pub error: Option<String>,
}

impl Default for GenerateForm {
    fn default() -> Self {
        Self {
            description: String::new(),
            context: String::new(),
            skip_permissions: false,
            focused_field: GenerateFormField::Description,
            waiting: false,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateFormField {
    Description,
    Context,
    SkipPermissions,
    Submit,
}

impl GenerateFormField {
    pub fn next(&self) -> GenerateFormField {
        match self {
            GenerateFormField::Description => GenerateFormField::Context,
            GenerateFormField::Context => GenerateFormField::SkipPermissions,
            GenerateFormField::SkipPermissions => GenerateFormField::Submit,
            GenerateFormField::Submit => GenerateFormField::Description,
        }
    }

    pub fn prev(&self) -> GenerateFormField {
        match self {
            GenerateFormField::Description => GenerateFormField::Submit,
            GenerateFormField::Context => GenerateFormField::Description,
            GenerateFormField::SkipPermissions => GenerateFormField::Context,
            GenerateFormField::Submit => GenerateFormField::SkipPermissions,
        }
    }

    pub fn is_text_field(&self) -> bool {
        matches!(self, GenerateFormField::Description | GenerateFormField::Context)
    }
}

#[derive(Debug, Clone)]
pub struct PendingGenerateAgent {
    pub label: String,
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub struct Autocomplete {
    pub query: String,
    pub matches: Vec<String>,
    pub selected: usize,
    pub trigger_pos: usize,
}

#[derive(Debug, Clone)]
pub struct PendingPermission {
    pub request_id: String,
    pub claude_session_id: String,
    pub tool_name: String,
    pub tool_input: String,
    pub request_path: PathBuf,
}

// ── Notes ────────────────────────────────────────────────

pub struct Note {
    pub path: PathBuf,
    pub title: String,
    pub folder: Option<String>,
    pub scroll_offset: usize,
}

pub struct NewNoteForm {
    pub title: String,
    pub folder: String,
    pub focused_field: NoteFormField,
}

impl Default for NewNoteForm {
    fn default() -> Self {
        Self {
            title: String::new(),
            folder: String::new(),
            focused_field: NoteFormField::Title,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteFormField {
    Title,
    Folder,
    Submit,
}

impl NoteFormField {
    pub fn next(&self) -> NoteFormField {
        match self {
            NoteFormField::Title => NoteFormField::Folder,
            NoteFormField::Folder => NoteFormField::Submit,
            NoteFormField::Submit => NoteFormField::Title,
        }
    }

    pub fn prev(&self) -> NoteFormField {
        match self {
            NoteFormField::Title => NoteFormField::Submit,
            NoteFormField::Folder => NoteFormField::Title,
            NoteFormField::Submit => NoteFormField::Folder,
        }
    }

    pub fn is_text_field(&self) -> bool {
        matches!(self, NoteFormField::Title | NoteFormField::Folder)
    }
}

// ── Embedded Terminal ────────────────────────────────────

pub struct EmbeddedTerminalState {
    pub id: TerminalId,
    pub parser: vt100::Parser,
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub worktree_path: PathBuf,
    pub label: String,
}
