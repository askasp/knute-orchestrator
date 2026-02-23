use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use uuid::Uuid;

use crate::message::{Action, Message};
use crate::model::*;

const MAX_COMPLETIONS: usize = 10;

impl App {
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            sessions: Vec::new(),
            notes: Vec::new(),
            selected_index: 0,
            mode: AppMode::NewSession,
            repo_root,
            dialog: None,
            should_quit: false,
            input_buffer: String::new(),
            notification: None,
            sidebar_state: ListState::default(),
            sidebar_focused: true,
            new_session_form: NewSessionForm::default(),
            new_note_form: NewNoteForm::default(),
            sub_agent_form: SubAgentForm::default(),
            file_cache: Vec::new(),
            autocomplete: None,
            generate_form: GenerateForm::default(),
            pending_generate_agents: Vec::new(),
            pending_permissions: VecDeque::new(),
            content_height: 0,
            worktree_view: WorktreeViewState::default(),
            terminals: Vec::new(),
            next_terminal_id: 0,
        }
    }

    // ── Worktree grouping ────────────────────────────────────

    pub fn worktree_groups(&self) -> Vec<WorktreeGroup> {
        let mut groups: Vec<WorktreeGroup> = Vec::new();
        for session in &self.sessions {
            if let Some(group) = groups.iter_mut().find(|g| g.worktree_path == session.worktree_path) {
                group.session_ids.push(session.id);
            } else {
                groups.push(WorktreeGroup {
                    worktree_path: session.worktree_path.clone(),
                    branch_name: session.branch_name.clone(),
                    session_ids: vec![session.id],
                });
            }
        }
        groups
    }

    fn root_session_for_worktree(&self, path: &PathBuf) -> Option<&Session> {
        self.sessions.iter().find(|s| s.worktree_path == *path && s.parent_session_id.is_none())
    }

    fn sessions_for_worktree(&self, path: &PathBuf) -> Vec<&Session> {
        self.sessions.iter().filter(|s| s.worktree_path == *path).collect()
    }

    pub fn terminals_for_worktree(&self, path: &PathBuf) -> Vec<&EmbeddedTerminalState> {
        self.terminals.iter().filter(|t| t.worktree_path == *path).collect()
    }

    // ── Update dispatch ──────────────────────────────────────

    pub fn update(&mut self, msg: Message) -> Vec<Action> {
        match msg {
            Message::Key(key) => self.handle_key(key),
            Message::MouseScroll { delta } => self.handle_mouse_scroll(delta),
            Message::Tick => self.handle_tick(),
            Message::WorktreeCreated { session_id, path } => {
                let mut actions = self.handle_worktree_created(session_id, path);
                actions.push(Action::SaveState);
                actions
            }
            Message::WorktreeCreateFailed { session_id, error } => {
                self.handle_worktree_failed(session_id, &error);
                vec![Action::SaveState]
            }
            Message::ClaudeSpawned {
                session_id,
                claude_session_id,
            } => {
                self.handle_claude_spawned(session_id, claude_session_id);
                vec![Action::SaveState]
            }
            Message::ClaudeSpawnFailed { session_id, error } => {
                self.handle_claude_spawn_failed(session_id, &error);
                vec![Action::SaveState]
            }
            Message::ClaudeOutput { session_id, entry } => {
                self.handle_claude_output(session_id, entry);
                vec![]
            }
            Message::ClaudeStatsUpdate { session_id, stats } => {
                self.handle_stats_update(session_id, stats);
                vec![]
            }
            Message::ClaudeProcessExited {
                session_id,
                exit_code,
            } => {
                self.handle_process_exited(session_id, exit_code);
                vec![Action::SaveState]
            }
            Message::FullDiffLoaded { worktree_path, diff } => {
                self.handle_full_diff_loaded(worktree_path, diff);
                vec![]
            }
            Message::PermissionRequested {
                claude_session_id,
                request_id,
                tool_name,
                tool_input,
                request_path,
            } => {
                self.handle_permission_requested(
                    claude_session_id,
                    request_id,
                    tool_name,
                    tool_input,
                    request_path,
                );
                vec![]
            }
            Message::GeneratePlanReceived {
                branch_name,
                agents,
                skip_permissions,
                context,
            } => {
                let mut actions = self.handle_generate_plan_received(branch_name, agents, skip_permissions, context);
                actions.push(Action::SaveState);
                actions
            }
            Message::GeneratePlanFailed { error } => {
                self.handle_generate_plan_failed(&error);
                vec![]
            }
            Message::TerminalOutput { terminal_id, data } => {
                if let Some(term) = self.terminals.iter_mut().find(|t| t.id == terminal_id) {
                    term.parser.process(&data);
                }
                vec![]
            }
            Message::TerminalExited { terminal_id } => {
                self.terminals.retain(|t| t.id != terminal_id);
                // If we were focused on this terminal, return to Terminals tab
                if let AppMode::EmbeddedTerminal { terminal_id: tid, worktree_path } = &self.mode {
                    if *tid == terminal_id {
                        let wt = worktree_path.clone();
                        self.worktree_view.active_tab = WorktreeTab::Terminals;
                        self.mode = AppMode::WorktreeView { worktree_path: wt };
                        self.sidebar_focused = false;
                    }
                }
                vec![]
            }
            Message::Resize { .. } => {
                // PTY resize is handled in main.rs
                vec![]
            }
        }
    }

    // ── Key dispatch ──────────────────────────────────────────

    fn handle_key(&mut self, key: KeyEvent) -> Vec<Action> {
        // Embedded terminal captures all keys except Ctrl+backslash
        if matches!(self.mode, AppMode::EmbeddedTerminal { .. }) {
            return self.handle_terminal_key(key);
        }

        if self.autocomplete.is_some() {
            return self.handle_autocomplete_key(key);
        }
        if self.dialog.is_some() {
            return self.handle_dialog_key(key);
        }
        if let Some(action) = self.handle_global_key(key) {
            return action;
        }

        match &self.mode.clone() {
            AppMode::NewSession => self.handle_new_session_key(key),
            AppMode::WorktreeView { .. } => self.handle_worktree_view_key(key),
            AppMode::SessionChat { .. } => self.handle_chat_key(key),
            AppMode::SessionChatInput { .. } => self.handle_chat_input_key(key),
            AppMode::NewSubAgent { .. } => self.handle_sub_agent_key(key),
            AppMode::Generate => self.handle_generate_key(key),
            AppMode::EmbeddedTerminal { .. } => unreachable!(),
            AppMode::NoteView { note_index } => {
                let idx = *note_index;
                self.handle_note_view_key(key, idx)
            }
            AppMode::NewNote => self.handle_new_note_key(key),
        }
    }

    fn is_text_entry_mode(&self) -> bool {
        matches!(
            self.mode,
            AppMode::SessionChatInput { .. }
                | AppMode::NewSession
                | AppMode::NewSubAgent { .. }
                | AppMode::Generate
                | AppMode::NewNote
        )
    }

    fn handle_global_key(&mut self, key: KeyEvent) -> Option<Vec<Action>> {
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            // In text entry mode, Ctrl+Q exits input focus instead of quitting
            if self.is_text_entry_mode() {
                match &self.mode {
                    AppMode::SessionChatInput { session_id } => {
                        let sid = *session_id;
                        self.mode = AppMode::SessionChat { session_id: sid };
                        self.input_buffer.clear();
                    }
                    AppMode::NewSession | AppMode::NewSubAgent { .. }
                    | AppMode::Generate | AppMode::NewNote => {
                        self.sidebar_focused = true;
                        let groups = self.worktree_groups();
                        if let Some(group) = groups.get(self.selected_index) {
                            self.mode = AppMode::WorktreeView { worktree_path: group.worktree_path.clone() };
                        }
                    }
                    _ => {}
                }
                return Some(vec![]);
            }
            if self.sessions.iter().any(|s| s.status == SessionStatus::Working) {
                self.dialog = Some(Dialog::Confirm {
                    title: "Quit".to_string(),
                    message: "Active sessions are running. Quit anyway?".to_string(),
                    on_confirm: DialogAction::QuitApp,
                });
                return Some(vec![]);
            }
            return Some(vec![Action::Quit]);
        }

        match key.code {
            KeyCode::Char('?') if !self.is_text_entry_mode() => {
                self.dialog = Some(Dialog::Help);
                return Some(vec![]);
            }
            KeyCode::Char('b') if !self.is_text_entry_mode() => {
                self.mode = AppMode::NewSession;
                self.sidebar_focused = false;
                self.new_session_form = NewSessionForm::default();
                return Some(vec![]);
            }
            KeyCode::Char('n') if !self.is_text_entry_mode() => {
                self.mode = AppMode::NewNote;
                self.sidebar_focused = false;
                self.new_note_form = NewNoteForm::default();
                return Some(vec![]);
            }
            KeyCode::Char('a') if !self.is_text_entry_mode() => {
                let wt_path = match &self.mode {
                    AppMode::WorktreeView { worktree_path } => Some(worktree_path.clone()),
                    AppMode::SessionChat { session_id }
                    | AppMode::SessionChatInput { session_id } => {
                        self.sessions.iter().find(|s| s.id == *session_id).map(|s| s.worktree_path.clone())
                    }
                    AppMode::NewSubAgent { worktree_path } => Some(worktree_path.clone()),
                    _ => {
                        let groups = self.worktree_groups();
                        groups.get(self.selected_index).map(|g| g.worktree_path.clone())
                    }
                };
                if let Some(path) = wt_path {
                    self.mode = AppMode::NewSubAgent { worktree_path: path };
                    self.sidebar_focused = false;
                    self.sub_agent_form = SubAgentForm::default();
                }
                return Some(vec![]);
            }
            KeyCode::Char('g') if !self.is_text_entry_mode() && self.sidebar_focused => {
                self.mode = AppMode::Generate;
                self.sidebar_focused = false;
                self.generate_form = GenerateForm::default();
                return Some(vec![]);
            }
            KeyCode::Char('d') if !self.is_text_entry_mode() && self.sidebar_focused => {
                let groups = self.worktree_groups();
                let num_groups = groups.len();
                if self.selected_index < num_groups {
                    if let Some(group) = groups.get(self.selected_index) {
                        let branch = group.branch_name.clone();
                        let path = group.worktree_path.clone();
                        self.dialog = Some(Dialog::Confirm {
                            title: "Delete Worktree".to_string(),
                            message: format!(
                                "Delete worktree '{}'? This removes all agents and the worktree.",
                                branch
                            ),
                            on_confirm: DialogAction::DeleteWorktree(path),
                        });
                    }
                } else {
                    let note_index = self.selected_index - num_groups;
                    if let Some(note) = self.notes.get(note_index) {
                        let title = note.title.clone();
                        self.dialog = Some(Dialog::Confirm {
                            title: "Delete Note".to_string(),
                            message: format!("Delete note '{}'?", title),
                            on_confirm: DialogAction::DeleteNote { note_index },
                        });
                    }
                }
                return Some(vec![]);
            }
            KeyCode::Char(c) if c.is_ascii_digit() && !self.is_text_entry_mode() => {
                let idx = c.to_digit(10).unwrap() as usize;
                let groups = self.worktree_groups();
                if idx >= 1 && idx <= groups.len() {
                    self.selected_index = idx - 1;
                    self.sidebar_state.select(Some(self.selected_index));
                    return Some(self.open_selected_item());
                }
                return Some(vec![]);
            }
            _ => None,
        }
    }

    // ── New Session mode ──────────────────────────────────────

    fn handle_new_session_key(&mut self, key: KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Esc => {
                self.sidebar_focused = true;
                let groups = self.worktree_groups();
                if let Some(group) = groups.get(self.selected_index) {
                    self.mode = AppMode::WorktreeView {
                        worktree_path: group.worktree_path.clone(),
                    };
                }
            }
            KeyCode::Tab => {
                self.new_session_form.focused_field = self.new_session_form.focused_field.next();
            }
            KeyCode::BackTab => {
                self.new_session_form.focused_field = self.new_session_form.focused_field.prev();
            }
            KeyCode::Enter => {
                if self.new_session_form.focused_field == FormField::Submit {
                    return self.submit_new_session();
                } else {
                    self.new_session_form.focused_field = self.new_session_form.focused_field.next();
                }
            }
            KeyCode::Char(' ') if self.new_session_form.focused_field == FormField::SkipPermissions => {
                self.new_session_form.skip_permissions = !self.new_session_form.skip_permissions;
            }
            KeyCode::Backspace if self.new_session_form.focused_field.is_text_field() => {
                get_form_field_mut(&mut self.new_session_form).pop();
            }
            KeyCode::Char('@') if self.new_session_form.focused_field == FormField::Prompt => {
                get_form_field_mut(&mut self.new_session_form).push('@');
                self.start_autocomplete();
            }
            KeyCode::Char(c) if self.new_session_form.focused_field.is_text_field() => {
                get_form_field_mut(&mut self.new_session_form).push(c);
            }
            _ => {}
        }
        vec![]
    }

    // ── Worktree View mode ───────────────────────────────────

    fn handle_worktree_view_key(&mut self, key: KeyEvent) -> Vec<Action> {
        let worktree_path = match &self.mode {
            AppMode::WorktreeView { worktree_path } => worktree_path.clone(),
            _ => return vec![],
        };

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('d') => {
                    if self.sidebar_focused {
                        self.sidebar_half_page_down();
                    } else if self.worktree_view.active_tab == WorktreeTab::Diff {
                        let amount = self.half_page();
                        self.worktree_view.changes.diff_scroll += amount;
                    }
                    return vec![];
                }
                KeyCode::Char('u') => {
                    if self.sidebar_focused {
                        self.sidebar_half_page_up();
                    } else if self.worktree_view.active_tab == WorktreeTab::Diff {
                        let amount = self.half_page();
                        self.worktree_view.changes.diff_scroll =
                            self.worktree_view.changes.diff_scroll.saturating_sub(amount);
                    }
                    return vec![];
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Tab if !self.sidebar_focused => {
                let new_tab = match self.worktree_view.active_tab {
                    WorktreeTab::Diff => WorktreeTab::Agents,
                    WorktreeTab::Agents => WorktreeTab::Terminals,
                    WorktreeTab::Terminals => WorktreeTab::Diff,
                };
                self.worktree_view.active_tab = new_tab;
                // Auto-refresh diff when switching to Diff tab
                if new_tab == WorktreeTab::Diff {
                    return self.refresh_worktree_diff(&worktree_path);
                }
            }
            KeyCode::Char('r') if !self.sidebar_focused && self.worktree_view.active_tab == WorktreeTab::Diff => {
                return self.refresh_worktree_diff(&worktree_path);
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.sidebar_focused = true;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.sidebar_focused {
                    self.sidebar_next();
                } else {
                    match self.worktree_view.active_tab {
                        WorktreeTab::Diff => {
                            self.worktree_view.changes.diff_scroll += 1;
                        }
                        WorktreeTab::Agents => {
                            let count = self.sessions_for_worktree(&worktree_path).len();
                            if count > 0 {
                                self.worktree_view.selected_agent =
                                    (self.worktree_view.selected_agent + 1).min(count - 1);
                            }
                        }
                        WorktreeTab::Terminals => {
                            let count = self.terminals_for_worktree(&worktree_path).len();
                            if count > 0 {
                                self.worktree_view.selected_terminal =
                                    (self.worktree_view.selected_terminal + 1).min(count - 1);
                            }
                        }
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.sidebar_focused {
                    self.sidebar_prev();
                } else {
                    match self.worktree_view.active_tab {
                        WorktreeTab::Diff => {
                            self.worktree_view.changes.diff_scroll =
                                self.worktree_view.changes.diff_scroll.saturating_sub(1);
                        }
                        WorktreeTab::Agents => {
                            self.worktree_view.selected_agent =
                                self.worktree_view.selected_agent.saturating_sub(1);
                        }
                        WorktreeTab::Terminals => {
                            self.worktree_view.selected_terminal =
                                self.worktree_view.selected_terminal.saturating_sub(1);
                        }
                    }
                }
            }
            KeyCode::Char('g') if !self.sidebar_focused => {
                if self.worktree_view.active_tab == WorktreeTab::Diff {
                    self.worktree_view.changes.diff_scroll = 0;
                }
            }
            KeyCode::Char('G') => {
                if self.sidebar_focused {
                    self.sidebar_to_bottom();
                } else if self.worktree_view.active_tab == WorktreeTab::Diff {
                    self.worktree_view.changes.diff_scroll = usize::MAX;
                }
            }
            KeyCode::Enter => {
                if self.sidebar_focused {
                    // If we're already viewing this worktree, just focus the content panel
                    let already_open = matches!(&self.mode, AppMode::WorktreeView { worktree_path: wt } if *wt == worktree_path);
                    if already_open {
                        self.sidebar_focused = false;
                        // Refresh diff if on Diff tab
                        if self.worktree_view.active_tab == WorktreeTab::Diff {
                            return self.refresh_worktree_diff(&worktree_path);
                        }
                    } else {
                        return self.open_selected_item();
                    }
                } else if self.worktree_view.active_tab == WorktreeTab::Agents {
                    let sid = {
                        let sessions = self.sessions_for_worktree(&worktree_path);
                        sessions.get(self.worktree_view.selected_agent).map(|s| s.id)
                    };
                    if let Some(sid) = sid {
                        // Scroll to bottom when entering chat
                        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == sid) {
                            s.scroll_offset = usize::MAX;
                            s.auto_scroll = true;
                        }
                        self.mode = AppMode::SessionChat { session_id: sid };
                        self.sidebar_focused = false;
                    }
                } else if self.worktree_view.active_tab == WorktreeTab::Terminals {
                    let term_ids: Vec<_> = self.terminals_for_worktree(&worktree_path)
                        .iter().map(|t| t.id).collect();
                    if let Some(&tid) = term_ids.get(self.worktree_view.selected_terminal) {
                        self.mode = AppMode::EmbeddedTerminal {
                            worktree_path: worktree_path.clone(),
                            terminal_id: tid,
                        };
                        self.sidebar_focused = false;
                    }
                }
            }
            KeyCode::Char('T') if !self.sidebar_focused => {
                return vec![Action::SpawnTerminal { worktree_path, command: None }];
            }
            KeyCode::Char('L') if !self.sidebar_focused => {
                return vec![Action::SpawnTerminal { worktree_path, command: Some(vec!["lazygit".into()]) }];
            }
            KeyCode::Char('d') if !self.sidebar_focused && self.worktree_view.active_tab == WorktreeTab::Terminals => {
                let term_ids: Vec<_> = self.terminals_for_worktree(&worktree_path)
                    .iter().map(|t| t.id).collect();
                if let Some(&tid) = term_ids.get(self.worktree_view.selected_terminal) {
                    return vec![Action::KillTerminal { terminal_id: tid }];
                }
            }
            KeyCode::Char('l') | KeyCode::Right if self.sidebar_focused => {
                self.sidebar_focused = false;
            }
            KeyCode::Char('h') | KeyCode::Left if !self.sidebar_focused => {
                self.sidebar_focused = true;
            }
            _ => {}
        }
        vec![]
    }

    // ── Mouse scroll ─────────────────────────────────────────

    fn handle_mouse_scroll(&mut self, delta: i16) -> Vec<Action> {
        match &self.mode {
            AppMode::WorktreeView { .. } if !self.sidebar_focused => {
                if self.worktree_view.active_tab == WorktreeTab::Diff {
                    if delta > 0 {
                        self.worktree_view.changes.diff_scroll += delta as usize;
                    } else {
                        self.worktree_view.changes.diff_scroll =
                            self.worktree_view.changes.diff_scroll.saturating_sub((-delta) as usize);
                    }
                }
            }
            AppMode::SessionChat { session_id } => {
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == *session_id) {
                    let pos = if session.auto_scroll { session.rendered_max_scroll } else { session.scroll_offset };
                    if delta > 0 {
                        session.scroll_offset = pos.saturating_add(delta as usize);
                    } else {
                        session.scroll_offset = pos.saturating_sub((-delta) as usize);
                        session.auto_scroll = false;
                    }
                }
            }
            _ => {}
        }
        vec![]
    }

    // ── Worktree diff helpers ─────────────────────────────────

    pub fn refresh_worktree_diff(&self, worktree_path: &PathBuf) -> Vec<Action> {
        vec![Action::LoadFullDiff {
            worktree_path: worktree_path.clone(),
        }]
    }

    fn handle_full_diff_loaded(&mut self, worktree_path: PathBuf, diff: String) {
        if let AppMode::WorktreeView { worktree_path: ref wt } = self.mode {
            if *wt == worktree_path {
                self.worktree_view.changes.set_diff_content(diff);
                self.worktree_view.changes.diff_scroll = 0;
            }
        }
    }

    // ── Chat mode (agent drill-in) ───────────────────────────

    fn handle_chat_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => {
                    if let AppMode::SessionChat { session_id } = &self.mode {
                        return vec![Action::KillProcess { session_id: *session_id }];
                    }
                }
                KeyCode::Char('d') => {
                    if self.sidebar_focused {
                        self.sidebar_half_page_down();
                    } else {
                        self.scroll_half_page_down();
                    }
                    return vec![];
                }
                KeyCode::Char('u') => {
                    if self.sidebar_focused {
                        self.sidebar_half_page_up();
                    } else {
                        self.scroll_half_page_up();
                    }
                    return vec![];
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                // Go back to WorktreeView (AGENTS tab)
                if let AppMode::SessionChat { session_id } = &self.mode {
                    if let Some(session) = self.sessions.iter().find(|s| s.id == *session_id) {
                        let wt = session.worktree_path.clone();
                        self.worktree_view.active_tab = WorktreeTab::Agents;
                        self.mode = AppMode::WorktreeView { worktree_path: wt };
                        self.sidebar_focused = false;
                        return vec![];
                    }
                }
            }
            KeyCode::Char('i') => {
                if let AppMode::SessionChat { session_id } = &self.mode {
                    self.input_buffer.clear();
                    self.mode = AppMode::SessionChatInput { session_id: *session_id };
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.sidebar_focused { self.sidebar_next(); } else { self.scroll_down(); }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.sidebar_focused { self.sidebar_prev(); } else { self.scroll_up(); }
            }
            KeyCode::Char('g') if !self.sidebar_focused => self.scroll_to_top(),
            KeyCode::Char('G') => {
                if self.sidebar_focused { self.sidebar_to_bottom(); } else { self.scroll_to_bottom(); }
            }
            KeyCode::Char('c') => {
                // Go to WorktreeView DIFF tab
                if let AppMode::SessionChat { session_id } = &self.mode {
                    if let Some(session) = self.sessions.iter().find(|s| s.id == *session_id) {
                        let wt = session.worktree_path.clone();
                        self.worktree_view.active_tab = WorktreeTab::Diff;
                        self.mode = AppMode::WorktreeView { worktree_path: wt.clone() };
                        self.sidebar_focused = false;
                        return self.refresh_worktree_diff(&wt);
                    }
                }
            }
            KeyCode::Char('L') => {
                if let AppMode::SessionChat { session_id } = &self.mode {
                    if let Some(session) = self.sessions.iter().find(|s| s.id == *session_id) {
                        return vec![Action::SpawnTerminal {
                            worktree_path: session.worktree_path.clone(),
                            command: Some(vec!["lazygit".into()]),
                        }];
                    }
                }
            }
            KeyCode::Char('T') => {
                if let AppMode::SessionChat { session_id } = &self.mode {
                    if let Some(session) = self.sessions.iter().find(|s| s.id == *session_id) {
                        return vec![Action::SpawnTerminal {
                            worktree_path: session.worktree_path.clone(),
                            command: None,
                        }];
                    }
                }
            }
            KeyCode::Enter if self.sidebar_focused => {
                return self.open_selected_item();
            }
            KeyCode::Char('l') | KeyCode::Right if self.sidebar_focused => {
                self.sidebar_focused = false;
            }
            KeyCode::Char('h') | KeyCode::Left if !self.sidebar_focused => {
                self.sidebar_focused = true;
            }
            _ => {}
        }
        vec![]
    }

    // ── Chat Input mode ───────────────────────────────────────

    fn handle_chat_input_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            self.input_buffer.clear();
            return vec![];
        }

        match key.code {
            KeyCode::Esc => {
                if let AppMode::SessionChatInput { session_id } = &self.mode {
                    self.mode = AppMode::SessionChat { session_id: *session_id };
                    self.input_buffer.clear();
                }
            }
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    return self.submit_input();
                }
            }
            KeyCode::Backspace => { self.input_buffer.pop(); }
            KeyCode::Char('@') => {
                self.input_buffer.push('@');
                self.start_autocomplete();
            }
            KeyCode::Char(c) => { self.input_buffer.push(c); }
            _ => {}
        }
        vec![]
    }

    // ── Sub-Agent mode ─────────────────────────────────────────

    fn handle_sub_agent_key(&mut self, key: KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Esc => {
                if let AppMode::NewSubAgent { worktree_path } = &self.mode {
                    let wt = worktree_path.clone();
                    self.mode = AppMode::WorktreeView { worktree_path: wt };
                    self.sidebar_focused = false;
                }
            }
            KeyCode::Tab => {
                self.sub_agent_form.focused_field = self.sub_agent_form.focused_field.next();
            }
            KeyCode::BackTab => {
                self.sub_agent_form.focused_field = self.sub_agent_form.focused_field.prev();
            }
            KeyCode::Enter => {
                if self.sub_agent_form.focused_field == SubAgentFormField::Submit {
                    return self.submit_sub_agent();
                } else {
                    self.sub_agent_form.focused_field = self.sub_agent_form.focused_field.next();
                }
            }
            KeyCode::Char(' ') if self.sub_agent_form.focused_field == SubAgentFormField::SkipPermissions => {
                self.sub_agent_form.skip_permissions = !self.sub_agent_form.skip_permissions;
            }
            KeyCode::Backspace if self.sub_agent_form.focused_field.is_text_field() => {
                get_sub_agent_field_mut(&mut self.sub_agent_form).pop();
            }
            KeyCode::Char('@') if self.sub_agent_form.focused_field == SubAgentFormField::Prompt => {
                get_sub_agent_field_mut(&mut self.sub_agent_form).push('@');
                self.start_autocomplete();
            }
            KeyCode::Char(c) if self.sub_agent_form.focused_field.is_text_field() => {
                get_sub_agent_field_mut(&mut self.sub_agent_form).push(c);
            }
            _ => {}
        }
        vec![]
    }

    fn submit_sub_agent(&mut self) -> Vec<Action> {
        let worktree_path = match &self.mode {
            AppMode::NewSubAgent { worktree_path } => worktree_path.clone(),
            _ => return vec![],
        };

        let form = self.sub_agent_form.clone();
        if form.initial_prompt.is_empty() {
            return vec![];
        }

        let branch_name = match self.sessions.iter().find(|s| s.worktree_path == worktree_path) {
            Some(s) => s.branch_name.clone(),
            None => return vec![],
        };
        let parent_id = self.root_session_for_worktree(&worktree_path).map(|s| s.id);

        let session_id = Uuid::new_v4();
        let label = if form.label.is_empty() { None } else { Some(form.label.clone()) };

        let session = Session {
            id: session_id,
            branch_name,
            worktree_path: worktree_path.clone(),
            status: SessionStatus::Working,
            claude_session_id: None,
            output_log: Vec::new(),
            created_at: Utc::now(),
            stats: SessionStats::default(),
            scroll_offset: 0,
            auto_scroll: true,
            rendered_max_scroll: 0,

            pending_prompt: None,
            skip_permissions: form.skip_permissions,
            agent_label: label,
            parent_session_id: parent_id,
            pending_permission_count: 0,
        };

        self.sessions.push(session);

        let groups = self.worktree_groups();
        if let Some(idx) = groups.iter().position(|g| g.worktree_path == worktree_path) {
            self.selected_index = idx;
            self.sidebar_state.select(Some(self.selected_index));
        }

        self.worktree_view.active_tab = WorktreeTab::Agents;
        self.mode = AppMode::WorktreeView { worktree_path: worktree_path.clone() };
        self.sidebar_focused = false;

        vec![
            Action::SpawnClaude {
                session_id,
                worktree_path,
                prompt: form.initial_prompt,
                skip_permissions: form.skip_permissions,
            },
            Action::SaveState,
        ]
    }

    // ── Generate mode ─────────────────────────────────────────

    fn handle_generate_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if self.generate_form.waiting {
            if key.code == KeyCode::Esc { self.generate_form.waiting = false; }
            return vec![];
        }

        match key.code {
            KeyCode::Esc => {
                self.sidebar_focused = true;
                let groups = self.worktree_groups();
                if let Some(group) = groups.get(self.selected_index) {
                    self.mode = AppMode::WorktreeView { worktree_path: group.worktree_path.clone() };
                } else {
                    self.mode = AppMode::NewSession;
                }
            }
            KeyCode::Tab => { self.generate_form.focused_field = self.generate_form.focused_field.next(); }
            KeyCode::BackTab => { self.generate_form.focused_field = self.generate_form.focused_field.prev(); }
            KeyCode::Enter => {
                if self.generate_form.focused_field == GenerateFormField::Submit {
                    return self.submit_generate();
                } else {
                    self.generate_form.focused_field = self.generate_form.focused_field.next();
                }
            }
            KeyCode::Char(' ') if self.generate_form.focused_field == GenerateFormField::SkipPermissions => {
                self.generate_form.skip_permissions = !self.generate_form.skip_permissions;
            }
            KeyCode::Backspace if self.generate_form.focused_field.is_text_field() => {
                self.generate_form_active_field_mut().pop();
            }
            KeyCode::Char('@') if self.generate_form.focused_field == GenerateFormField::Context => {
                self.generate_form.context.push('@');
                self.start_autocomplete();
            }
            KeyCode::Char(c) if self.generate_form.focused_field.is_text_field() => {
                self.generate_form_active_field_mut().push(c);
            }
            _ => {}
        }
        vec![]
    }

    fn generate_form_active_field_mut(&mut self) -> &mut String {
        match self.generate_form.focused_field {
            GenerateFormField::Context => &mut self.generate_form.context,
            _ => &mut self.generate_form.description,
        }
    }

    fn submit_generate(&mut self) -> Vec<Action> {
        if self.generate_form.description.is_empty() { return vec![]; }
        self.generate_form.waiting = true;
        self.generate_form.error = None;
        let context = if self.generate_form.context.trim().is_empty() {
            None
        } else {
            Some(self.generate_form.context.clone())
        };
        vec![Action::GeneratePlan {
            description: self.generate_form.description.clone(),
            context,
            repo_root: self.repo_root.clone(),
            skip_permissions: self.generate_form.skip_permissions,
        }]
    }

    fn handle_generate_plan_received(
        &mut self, branch_name: String, agents: Vec<(String, String)>,
        skip_permissions: bool, context: Option<String>,
    ) -> Vec<Action> {
        self.generate_form.waiting = false;
        if agents.is_empty() { return vec![]; }

        let worktree_path = self.repo_root.join(".knute").join("worktrees").join(&branch_name);
        let (first_label, first_prompt) = agents[0].clone();
        let first_prompt = prepend_context(&context, &first_prompt);
        let first_id = Uuid::new_v4();

        let first_session = Session {
            id: first_id, branch_name: branch_name.clone(), worktree_path: worktree_path.clone(),
            status: SessionStatus::Creating, claude_session_id: None, output_log: Vec::new(),
            created_at: Utc::now(), stats: SessionStats::default(), scroll_offset: 0,
            auto_scroll: true, rendered_max_scroll: 0,
            pending_prompt: Some(first_prompt), skip_permissions,
            agent_label: Some(first_label), parent_session_id: None, pending_permission_count: 0,
        };

        self.sessions.push(first_session);
        let groups = self.worktree_groups();
        if let Some(idx) = groups.iter().position(|g| g.worktree_path == worktree_path) {
            self.selected_index = idx;
            self.sidebar_state.select(Some(self.selected_index));
        }
        self.worktree_view.active_tab = WorktreeTab::Agents;
        self.mode = AppMode::WorktreeView { worktree_path: worktree_path.clone() };
        self.sidebar_focused = false;

        self.pending_generate_agents = agents[1..].iter()
            .map(|(label, prompt)| PendingGenerateAgent {
                label: label.clone(),
                prompt: prepend_context(&context, prompt),
            })
            .collect();

        vec![Action::CreateWorktree {
            session_id: first_id, repo_root: self.repo_root.clone(),
            branch: branch_name, base: "main".to_string(),
        }]
    }

    fn handle_generate_plan_failed(&mut self, error: &str) {
        self.generate_form.waiting = false;
        self.generate_form.error = Some(error.to_string());
    }

    // ── Note View mode ───────────────────────────────────────

    fn handle_note_view_key(&mut self, key: KeyEvent, note_index: usize) -> Vec<Action> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                if self.sidebar_focused {
                    // Already on sidebar — no-op, stay here
                } else {
                    self.sidebar_focused = true;
                }
            }
            KeyCode::Char('e') => {
                return self.open_note_in_terminal(note_index);
            }
            KeyCode::Char('d') => {
                if let Some(note) = self.notes.get(note_index) {
                    let title = note.title.clone();
                    self.dialog = Some(Dialog::Confirm {
                        title: "Delete Note".to_string(),
                        message: format!("Delete note '{}'?", title),
                        on_confirm: DialogAction::DeleteNote { note_index },
                    });
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.sidebar_focused { self.sidebar_next(); }
                else if let Some(note) = self.notes.get_mut(note_index) { note.scroll_offset += 1; }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.sidebar_focused { self.sidebar_prev(); }
                else if let Some(note) = self.notes.get_mut(note_index) {
                    note.scroll_offset = note.scroll_offset.saturating_sub(1);
                }
            }
            KeyCode::Char('g') if !self.sidebar_focused => {
                if let Some(note) = self.notes.get_mut(note_index) { note.scroll_offset = 0; }
            }
            KeyCode::Char('G') if !self.sidebar_focused => {
                if let Some(note) = self.notes.get_mut(note_index) { note.scroll_offset = usize::MAX; }
            }
            KeyCode::Enter if self.sidebar_focused => { return self.open_selected_item(); }
            KeyCode::Char('l') | KeyCode::Right if self.sidebar_focused => { self.sidebar_focused = false; }
            KeyCode::Char('h') | KeyCode::Left if !self.sidebar_focused => { self.sidebar_focused = true; }
            _ => {}
        }
        vec![]
    }

    // ── New Note mode ────────────────────────────────────────

    fn handle_new_note_key(&mut self, key: KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Esc => {
                self.sidebar_focused = true;
                let groups = self.worktree_groups();
                if let Some(group) = groups.get(self.selected_index) {
                    self.mode = AppMode::WorktreeView { worktree_path: group.worktree_path.clone() };
                }
            }
            KeyCode::Tab => { self.new_note_form.focused_field = self.new_note_form.focused_field.next(); }
            KeyCode::BackTab => { self.new_note_form.focused_field = self.new_note_form.focused_field.prev(); }
            KeyCode::Enter => {
                if self.new_note_form.focused_field == NoteFormField::Submit {
                    return self.submit_new_note();
                } else {
                    self.new_note_form.focused_field = self.new_note_form.focused_field.next();
                }
            }
            KeyCode::Backspace if self.new_note_form.focused_field.is_text_field() => {
                get_note_form_field_mut(&mut self.new_note_form).pop();
            }
            KeyCode::Char(c) if self.new_note_form.focused_field.is_text_field() => {
                get_note_form_field_mut(&mut self.new_note_form).push(c);
            }
            _ => {}
        }
        vec![]
    }

    fn submit_new_note(&mut self) -> Vec<Action> {
        let title = self.new_note_form.title.trim().to_string();
        if title.is_empty() { return vec![]; }
        let folder = self.new_note_form.folder.trim().to_string();

        match crate::notes::create_note(&self.repo_root, &title, &folder) {
            Ok(_path) => {
                self.notes = crate::notes::scan_notes(&self.repo_root);
                let note_index = self.notes.iter().position(|n| {
                    n.title == title && n.folder.as_deref().unwrap_or("") == folder
                }).unwrap_or(0);

                let groups = self.worktree_groups();
                self.mode = AppMode::NoteView { note_index };
                self.selected_index = groups.len() + note_index;
                self.sidebar_state.select(Some(self.selected_index));
                self.sidebar_focused = false;
                self.open_note_in_terminal(note_index)
            }
            Err(_) => vec![],
        }
    }

    fn open_note_in_terminal(&self, note_index: usize) -> Vec<Action> {
        if let Some(note) = self.notes.get(note_index) {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
            let path = note.path.to_string_lossy().to_string();
            return vec![Action::SpawnTerminal {
                worktree_path: self.repo_root.clone(),
                command: Some(vec![editor, path]),
            }];
        }
        vec![]
    }

    // ── Embedded Terminal mode ────────────────────────────────

    fn handle_terminal_key(&mut self, key: KeyEvent) -> Vec<Action> {
        // Ctrl+\ to detach (terminal keeps running)
        // crossterm 0.28 maps 0x1C (Ctrl+\) to Char('4') + CONTROL on unix
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('4') | KeyCode::Char('\\'))
        {
            return vec![Action::DetachTerminal];
        }

        let terminal_id = match &self.mode {
            AppMode::EmbeddedTerminal { terminal_id, .. } => *terminal_id,
            _ => return vec![],
        };

        let bytes = key_to_bytes(key);
        if !bytes.is_empty() {
            return vec![Action::TerminalInput { terminal_id, data: bytes }];
        }
        vec![]
    }

    // ── Autocomplete ──────────────────────────────────────────

    fn handle_autocomplete_key(&mut self, key: KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Esc => { self.autocomplete = None; }
            KeyCode::Up => {
                if let Some(ac) = &mut self.autocomplete { ac.selected = ac.selected.saturating_sub(1); }
            }
            KeyCode::Down => {
                if let Some(ac) = &mut self.autocomplete {
                    if !ac.matches.is_empty() { ac.selected = (ac.selected + 1).min(ac.matches.len() - 1); }
                }
            }
            KeyCode::Tab | KeyCode::Enter => { self.accept_autocomplete(); }
            KeyCode::Backspace => {
                let should_dismiss = if let Some(ac) = &mut self.autocomplete {
                    if ac.query.is_empty() { true } else { ac.query.pop(); false }
                } else { false };
                if should_dismiss {
                    let buf = self.active_buffer_mut();
                    buf.pop();
                    self.autocomplete = None;
                } else {
                    self.refresh_autocomplete_matches();
                }
            }
            KeyCode::Char(' ') => {
                self.autocomplete = None;
                let buf = self.active_buffer_mut();
                buf.push(' ');
            }
            KeyCode::Char(c) => {
                if let Some(ac) = &mut self.autocomplete { ac.query.push(c); }
                self.refresh_autocomplete_matches();
            }
            _ => {}
        }
        vec![]
    }

    fn start_autocomplete(&mut self) {
        let buf = self.active_buffer();
        let trigger_pos = buf.len().saturating_sub(1);
        let matches = filter_file_list(&self.file_cache, "");
        self.autocomplete = Some(Autocomplete { query: String::new(), matches, selected: 0, trigger_pos });
    }

    fn refresh_autocomplete_matches(&mut self) {
        if let Some(ac) = &mut self.autocomplete {
            ac.matches = filter_file_list(&self.file_cache, &ac.query);
            if ac.selected >= ac.matches.len() { ac.selected = ac.matches.len().saturating_sub(1); }
        }
    }

    fn accept_autocomplete(&mut self) {
        let Some(ac) = self.autocomplete.take() else { return; };
        if ac.matches.is_empty() { return; }
        let selected_path = &ac.matches[ac.selected];
        let buf = self.active_buffer_mut();
        buf.truncate(ac.trigger_pos);
        buf.push_str(&format!("@{} ", selected_path));
    }

    fn active_buffer(&self) -> &str {
        if matches!(self.mode, AppMode::NewSession) && self.new_session_form.focused_field == FormField::Prompt {
            return &self.new_session_form.initial_prompt;
        }
        if matches!(self.mode, AppMode::NewSubAgent { .. }) && self.sub_agent_form.focused_field == SubAgentFormField::Prompt {
            return &self.sub_agent_form.initial_prompt;
        }
        if matches!(self.mode, AppMode::Generate) && self.generate_form.focused_field == GenerateFormField::Context {
            return &self.generate_form.context;
        }
        &self.input_buffer
    }

    fn active_buffer_mut(&mut self) -> &mut String {
        if matches!(self.mode, AppMode::NewSession) && self.new_session_form.focused_field == FormField::Prompt {
            return &mut self.new_session_form.initial_prompt;
        }
        if matches!(self.mode, AppMode::NewSubAgent { .. }) && self.sub_agent_form.focused_field == SubAgentFormField::Prompt {
            return &mut self.sub_agent_form.initial_prompt;
        }
        if matches!(self.mode, AppMode::Generate) && self.generate_form.focused_field == GenerateFormField::Context {
            return &mut self.generate_form.context;
        }
        &mut self.input_buffer
    }

    // ── Dialog ────────────────────────────────────────────────

    fn handle_dialog_key(&mut self, key: KeyEvent) -> Vec<Action> {
        let dialog = self.dialog.as_ref().unwrap();
        match dialog {
            Dialog::Confirm { on_confirm, .. } => match key.code {
                KeyCode::Esc => { self.dialog = None; }
                KeyCode::Enter => {
                    let action = on_confirm.clone();
                    self.dialog = None;
                    return self.execute_dialog_action(action);
                }
                _ => {}
            },
            Dialog::Help => match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => { self.dialog = None; }
                _ => {}
            },
            Dialog::Permission { request_id, claude_session_id, request_path, .. } => {
                let request_id = request_id.clone();
                let claude_session_id = claude_session_id.clone();
                let request_path = request_path.clone();
                match key.code {
                    KeyCode::Enter => {
                        self.dialog = None;
                        self.decrement_permission_count(&claude_session_id);
                        self.show_next_pending_permission();
                        return vec![Action::WritePermissionResponse {
                            request_path, request_id, claude_session_id, allow: true,
                        }];
                    }
                    KeyCode::Esc => {
                        self.dialog = None;
                        self.decrement_permission_count(&claude_session_id);
                        self.show_next_pending_permission();
                        return vec![Action::WritePermissionResponse {
                            request_path, request_id, claude_session_id, allow: false,
                        }];
                    }
                    _ => {}
                }
            }
        }
        vec![]
    }

    fn execute_dialog_action(&mut self, action: DialogAction) -> Vec<Action> {
        match action {
            DialogAction::QuitApp => vec![Action::Quit],
            DialogAction::DeleteWorktree(path) => {
                let session_ids: Vec<Uuid> = self.sessions.iter()
                    .filter(|s| s.worktree_path == path).map(|s| s.id).collect();

                let mut actions = vec![];
                for id in &session_ids {
                    if let Some(idx) = self.sessions.iter().position(|s| s.id == *id) {
                        let session = self.sessions.remove(idx);
                        if let Some(ref csid) = session.claude_session_id {
                            actions.extend(self.drain_permissions_for_session(csid));
                        }
                        if session.status == SessionStatus::Working {
                            actions.push(Action::KillProcess { session_id: *id });
                        }
                    }
                }

                if !session_ids.is_empty() {
                    actions.push(Action::RemoveWorktree { worktree_path: path.clone() });
                }

                let groups = self.worktree_groups();
                if groups.is_empty() {
                    self.selected_index = 0;
                    self.sidebar_state.select(None);
                    self.mode = AppMode::NewSession;
                    self.sidebar_focused = true;
                } else {
                    if self.selected_index >= groups.len() {
                        self.selected_index = groups.len() - 1;
                    }
                    self.sidebar_state.select(Some(self.selected_index));

                    let viewing_deleted = match &self.mode {
                        AppMode::WorktreeView { worktree_path } => *worktree_path == path,
                        AppMode::SessionChat { session_id } | AppMode::SessionChatInput { session_id } => {
                            session_ids.contains(session_id)
                        }
                        AppMode::NewSubAgent { worktree_path } => *worktree_path == path,
                        _ => false,
                    };
                    if viewing_deleted {
                        if let Some(group) = groups.get(self.selected_index) {
                            self.mode = AppMode::WorktreeView { worktree_path: group.worktree_path.clone() };
                        } else {
                            self.mode = AppMode::NewSession;
                        }
                        self.sidebar_focused = true;
                    }
                }

                actions.push(Action::SaveState);
                actions
            }
            DialogAction::DeleteNote { note_index } => {
                vec![Action::DeleteNote { note_index }]
            }
        }
    }

    // ── Permission handling ─────────────────────────────────

    fn handle_permission_requested(
        &mut self, claude_session_id: String, request_id: String,
        tool_name: String, tool_input: String, request_path: std::path::PathBuf,
    ) {
        if let Some(session) = self.sessions.iter_mut()
            .find(|s| s.claude_session_id.as_deref() == Some(&claude_session_id))
        {
            session.pending_permission_count += 1;
        }

        if self.dialog.is_none() {
            self.dialog = Some(Dialog::Permission {
                request_id, claude_session_id, tool_name, tool_input, request_path,
            });
        } else {
            self.pending_permissions.push_back(PendingPermission {
                request_id, claude_session_id, tool_name, tool_input, request_path,
            });
        }
    }

    fn show_next_pending_permission(&mut self) {
        if let Some(perm) = self.pending_permissions.pop_front() {
            self.dialog = Some(Dialog::Permission {
                request_id: perm.request_id, claude_session_id: perm.claude_session_id,
                tool_name: perm.tool_name, tool_input: perm.tool_input, request_path: perm.request_path,
            });
        }
    }

    fn decrement_permission_count(&mut self, claude_session_id: &str) {
        if let Some(session) = self.sessions.iter_mut()
            .find(|s| s.claude_session_id.as_deref() == Some(claude_session_id))
        {
            session.pending_permission_count = session.pending_permission_count.saturating_sub(1);
        }
    }

    pub fn drain_permissions_for_session(&mut self, claude_session_id: &str) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut remaining = VecDeque::new();
        for perm in self.pending_permissions.drain(..) {
            if perm.claude_session_id == claude_session_id {
                actions.push(Action::WritePermissionResponse {
                    request_path: perm.request_path, request_id: perm.request_id,
                    claude_session_id: perm.claude_session_id, allow: false,
                });
            } else {
                remaining.push_back(perm);
            }
        }
        self.pending_permissions = remaining;

        if let Some(Dialog::Permission { claude_session_id: ref csid, ref request_path, ref request_id, .. }) = self.dialog {
            if csid == claude_session_id {
                actions.push(Action::WritePermissionResponse {
                    request_path: request_path.clone(), request_id: request_id.clone(),
                    claude_session_id: csid.clone(), allow: false,
                });
                self.dialog = None;
                self.show_next_pending_permission();
            }
        }

        if let Some(session) = self.sessions.iter_mut()
            .find(|s| s.claude_session_id.as_deref() == Some(claude_session_id))
        {
            session.pending_permission_count = 0;
        }

        actions
    }

    // ── Sidebar navigation ────────────────────────────────────

    fn sidebar_item_count(&self) -> usize {
        self.worktree_groups().len() + self.notes.len()
    }

    fn sidebar_next(&mut self) {
        let total = self.sidebar_item_count();
        if total > 0 {
            self.selected_index = (self.selected_index + 1).min(total - 1);
            self.sidebar_state.select(Some(self.selected_index));
        }
    }

    fn sidebar_prev(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
        self.sidebar_state.select(Some(self.selected_index));
    }

    fn sidebar_to_bottom(&mut self) {
        let total = self.sidebar_item_count();
        if total > 0 {
            self.selected_index = total - 1;
            self.sidebar_state.select(Some(self.selected_index));
        }
    }

    fn sidebar_half_page_down(&mut self) {
        let total = self.sidebar_item_count();
        if total > 0 {
            let amount = (self.content_height / 2).max(1);
            self.selected_index = (self.selected_index + amount).min(total - 1);
            self.sidebar_state.select(Some(self.selected_index));
        }
    }

    fn sidebar_half_page_up(&mut self) {
        let amount = (self.content_height / 2).max(1);
        self.selected_index = self.selected_index.saturating_sub(amount);
        self.sidebar_state.select(Some(self.selected_index));
    }

    fn open_selected_item(&mut self) -> Vec<Action> {
        let groups = self.worktree_groups();
        let num_groups = groups.len();
        if self.selected_index < num_groups {
            if let Some(group) = groups.get(self.selected_index) {
                let wt = group.worktree_path.clone();
                self.worktree_view.active_tab = WorktreeTab::Diff;
                self.worktree_view.selected_agent = 0;
                self.worktree_view.selected_terminal = 0;
                self.worktree_view.changes = ChangesState::default();
                self.mode = AppMode::WorktreeView { worktree_path: wt.clone() };
                self.sidebar_focused = false;
                return self.refresh_worktree_diff(&wt);
            }
        } else {
            let note_index = self.selected_index - num_groups;
            if note_index < self.notes.len() {
                if let Some(note) = self.notes.get_mut(note_index) {
                    note.scroll_offset = 0;
                }
                self.mode = AppMode::NoteView { note_index };
                self.sidebar_focused = false;
                return vec![];
            }
        }
        vec![]
    }

    // ── Scrolling ─────────────────────────────────────────────

    fn half_page(&self) -> usize { (self.content_height / 2).max(1) }

    fn scroll_up(&mut self) {
        if let Some(session) = self.current_session_mut() {
            let pos = if session.auto_scroll { session.rendered_max_scroll } else { session.scroll_offset };
            session.scroll_offset = pos.saturating_sub(1);
            session.auto_scroll = false;
        }
    }

    fn scroll_down(&mut self) {
        if let Some(session) = self.current_session_mut() {
            let pos = if session.auto_scroll { session.rendered_max_scroll } else { session.scroll_offset };
            session.scroll_offset = pos.saturating_add(1);
        }
    }

    fn scroll_half_page_up(&mut self) {
        let amount = self.half_page();
        if let Some(session) = self.current_session_mut() {
            let pos = if session.auto_scroll { session.rendered_max_scroll } else { session.scroll_offset };
            session.scroll_offset = pos.saturating_sub(amount);
            session.auto_scroll = false;
        }
    }

    fn scroll_half_page_down(&mut self) {
        let amount = self.half_page();
        if let Some(session) = self.current_session_mut() {
            let pos = if session.auto_scroll { session.rendered_max_scroll } else { session.scroll_offset };
            session.scroll_offset = pos.saturating_add(amount);
        }
    }

    fn scroll_to_top(&mut self) {
        if let Some(session) = self.current_session_mut() {
            session.scroll_offset = 0;
            session.auto_scroll = false;
        }
    }

    fn scroll_to_bottom(&mut self) {
        if let Some(session) = self.current_session_mut() {
            session.scroll_offset = usize::MAX;
            session.auto_scroll = true;
        }
    }

    fn current_session_mut(&mut self) -> Option<&mut Session> {
        let session_id = match &self.mode {
            AppMode::SessionChat { session_id } | AppMode::SessionChatInput { session_id } => *session_id,
            _ => return None,
        };
        self.sessions.iter_mut().find(|s| s.id == session_id)
    }

    // ── Session lifecycle ─────────────────────────────────────

    fn submit_new_session(&mut self) -> Vec<Action> {
        let form = self.new_session_form.clone();
        if form.branch_name.is_empty() { return vec![]; }

        let session_id = Uuid::new_v4();
        let worktree_path = self.repo_root.join(".knute").join("worktrees").join(&form.branch_name);
        let pending_prompt = if form.initial_prompt.is_empty() { None } else { Some(form.initial_prompt.clone()) };

        let session = Session {
            id: session_id, branch_name: form.branch_name.clone(),
            worktree_path: worktree_path.clone(), status: SessionStatus::Creating,
            claude_session_id: None, output_log: Vec::new(), created_at: Utc::now(),
            stats: SessionStats::default(), scroll_offset: 0, auto_scroll: true, rendered_max_scroll: 0,
            pending_prompt, skip_permissions: form.skip_permissions,
            agent_label: None, parent_session_id: None, pending_permission_count: 0,
        };

        self.sessions.push(session);
        let groups = self.worktree_groups();
        if let Some(idx) = groups.iter().position(|g| g.worktree_path == worktree_path) {
            self.selected_index = idx;
            self.sidebar_state.select(Some(self.selected_index));
        }
        self.worktree_view.active_tab = WorktreeTab::Agents;
        self.mode = AppMode::WorktreeView { worktree_path: worktree_path.clone() };
        self.sidebar_focused = false;

        vec![Action::CreateWorktree {
            session_id, repo_root: self.repo_root.clone(),
            branch: form.branch_name, base: form.base_branch,
        }]
    }

    fn submit_input(&mut self) -> Vec<Action> {
        let message = self.input_buffer.clone();
        self.input_buffer.clear();

        let session_id = match &self.mode {
            AppMode::SessionChatInput { session_id } => *session_id,
            _ => return vec![],
        };

        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.output_log.push(OutputEntry {
                timestamp: Instant::now(), kind: OutputKind::UserMessage(message.clone()),
            });
            self.mode = AppMode::SessionChat { session_id };
            let worktree_path = session.worktree_path.clone();

            if let Some(claude_sid) = &session.claude_session_id {
                session.status = SessionStatus::Working;
                return vec![Action::ResumeClaude {
                    session_id, worktree_path, claude_session_id: claude_sid.clone(),
                    message, skip_permissions: false,
                }];
            } else {
                session.status = SessionStatus::Working;
                return vec![Action::SpawnClaude {
                    session_id, worktree_path, prompt: message, skip_permissions: false,
                }];
            }
        }
        vec![]
    }

    // ── Async event handlers ──────────────────────────────────

    fn handle_tick(&mut self) -> Vec<Action> {
        if let Some(notif) = &self.notification {
            if notif.created_at.elapsed().as_secs() >= notif.duration_secs { self.notification = None; }
        }
        vec![]
    }

    fn handle_worktree_created(&mut self, session_id: Uuid, path: PathBuf) -> Vec<Action> {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.worktree_path = path.clone();
            session.output_log.push(OutputEntry {
                timestamp: Instant::now(),
                kind: OutputKind::SystemMessage(format!("Worktree created at {}", path.display())),
            });

            if let Some(prompt) = session.pending_prompt.take() {
                session.status = SessionStatus::Working;
                let skip = session.skip_permissions;
                let branch_name = session.branch_name.clone();
                let mut actions = vec![Action::SpawnClaude {
                    session_id, worktree_path: path.clone(), prompt, skip_permissions: skip,
                }];

                let pending = std::mem::take(&mut self.pending_generate_agents);
                for agent in pending {
                    let agent_id = Uuid::new_v4();
                    let agent_session = Session {
                        id: agent_id, branch_name: branch_name.clone(),
                        worktree_path: path.clone(), status: SessionStatus::Working,
                        claude_session_id: None, output_log: Vec::new(), created_at: Utc::now(),
                        stats: SessionStats::default(), scroll_offset: 0, auto_scroll: true, rendered_max_scroll: 0,
                        pending_prompt: None,
                        skip_permissions: skip, agent_label: Some(agent.label),
                        parent_session_id: Some(session_id), pending_permission_count: 0,
                    };
                    self.sessions.push(agent_session);
                    actions.push(Action::SpawnClaude {
                        session_id: agent_id, worktree_path: path.clone(),
                        prompt: agent.prompt, skip_permissions: skip,
                    });
                }
                return actions;
            } else {
                session.status = SessionStatus::Idle;
            }
        }
        vec![]
    }

    fn handle_worktree_failed(&mut self, session_id: Uuid, error: &str) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.status = SessionStatus::Error(error.to_string());
            session.output_log.push(OutputEntry {
                timestamp: Instant::now(),
                kind: OutputKind::Error(format!("Worktree creation failed: {}", error)),
            });
        }
    }

    fn handle_claude_spawned(&mut self, session_id: Uuid, claude_session_id: String) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.claude_session_id = Some(claude_session_id);
            session.status = SessionStatus::Working;
        }
    }

    fn handle_claude_spawn_failed(&mut self, session_id: Uuid, error: &str) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.status = SessionStatus::Error(error.to_string());
            session.output_log.push(OutputEntry {
                timestamp: Instant::now(),
                kind: OutputKind::Error(format!("Failed to start Claude: {}", error)),
            });
        }
    }

    fn handle_claude_output(&mut self, session_id: Uuid, entry: OutputEntry) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            let summary = match &entry.kind {
                OutputKind::AssistantText(t) => {
                    let first_line = t.lines().next().unwrap_or("");
                    if first_line.chars().count() > 40 {
                        format!("{}...", first_line.chars().take(40).collect::<String>())
                    } else { first_line.to_string() }
                }
                OutputKind::ToolUse { name, .. } => format!("Running {}", name),
                OutputKind::ToolResult { tool_name, .. } => format!("{} done", tool_name),
                OutputKind::UserMessage(_) => "User message".to_string(),
                OutputKind::SystemMessage(t) => t.clone(),
                OutputKind::Error(t) => format!("Error: {}", t),
            };
            session.stats.last_activity = Some(Instant::now());
            session.stats.last_activity_summary = summary;
            session.output_log.push(entry);
            if session.auto_scroll { session.scroll_offset = usize::MAX; }
        }
    }

    fn handle_stats_update(&mut self, session_id: Uuid, stats: SessionStats) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.stats = stats;
        }
    }

    fn handle_process_exited(&mut self, session_id: Uuid, exit_code: Option<i32>) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            match exit_code {
                Some(0) => {
                    session.status = SessionStatus::Completed;
                    session.stats.last_activity_summary = "Completed".to_string();
                }
                Some(code) => {
                    session.status = SessionStatus::Error(format!("Exited with code {}", code));
                }
                None => {
                    session.status = SessionStatus::Idle;
                    session.stats.last_activity_summary = "Interrupted".to_string();
                }
            }
            session.stats.last_activity = Some(Instant::now());
        }
    }

}

fn get_form_field_mut(form: &mut NewSessionForm) -> &mut String {
    match form.focused_field {
        FormField::BranchName => &mut form.branch_name,
        FormField::BaseBranch => &mut form.base_branch,
        FormField::Prompt => &mut form.initial_prompt,
        _ => unreachable!("is_text_field() guard"),
    }
}

fn get_sub_agent_field_mut(form: &mut SubAgentForm) -> &mut String {
    match form.focused_field {
        SubAgentFormField::Label => &mut form.label,
        SubAgentFormField::Prompt => &mut form.initial_prompt,
        _ => unreachable!("is_text_field() guard"),
    }
}

fn get_note_form_field_mut(form: &mut NewNoteForm) -> &mut String {
    match form.focused_field {
        NoteFormField::Title => &mut form.title,
        NoteFormField::Folder => &mut form.folder,
        _ => unreachable!("is_text_field() guard"),
    }
}

fn prepend_context(context: &Option<String>, prompt: &str) -> String {
    match context {
        Some(ctx) if !ctx.trim().is_empty() => format!("{} {}", ctx, prompt),
        _ => prompt.to_string(),
    }
}

fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Char(c) if ctrl => {
            // Ctrl+letter → ASCII control code (a=1, b=2, ..., z=26)
            let byte = (c as u8).wrapping_sub(b'a').wrapping_add(1);
            if alt {
                vec![0x1b, byte]
            } else {
                vec![byte]
            }
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            if alt {
                let mut v = vec![0x1b];
                v.extend_from_slice(s.as_bytes());
                v
            } else {
                s.as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                vec![0x1b, b'[', b'Z']
            } else {
                vec![b'\t']
            }
        }
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Left => vec![0x1b, b'[', b'D'],
        KeyCode::Home => vec![0x1b, b'[', b'H'],
        KeyCode::End => vec![0x1b, b'[', b'F'],
        KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],
        KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
        KeyCode::Insert => vec![0x1b, b'[', b'2', b'~'],
        KeyCode::F(n) => match n {
            1 => vec![0x1b, b'O', b'P'],
            2 => vec![0x1b, b'O', b'Q'],
            3 => vec![0x1b, b'O', b'R'],
            4 => vec![0x1b, b'O', b'S'],
            5 => vec![0x1b, b'[', b'1', b'5', b'~'],
            6 => vec![0x1b, b'[', b'1', b'7', b'~'],
            7 => vec![0x1b, b'[', b'1', b'8', b'~'],
            8 => vec![0x1b, b'[', b'1', b'9', b'~'],
            9 => vec![0x1b, b'[', b'2', b'0', b'~'],
            10 => vec![0x1b, b'[', b'2', b'1', b'~'],
            11 => vec![0x1b, b'[', b'2', b'3', b'~'],
            12 => vec![0x1b, b'[', b'2', b'4', b'~'],
            _ => vec![],
        },
        _ => vec![],
    }
}

fn filter_file_list(file_cache: &[String], query: &str) -> Vec<String> {
    if query.is_empty() {
        return file_cache.iter().take(MAX_COMPLETIONS).cloned().collect();
    }
    let query_lower = query.to_lowercase();
    let mut scored: Vec<(usize, &String)> = file_cache.iter().filter_map(|path| {
        let path_lower = path.to_lowercase();
        if path_lower.contains(&query_lower) {
            let score = if path_lower.starts_with(&query_lower) { 0 }
            else if path.rsplit('/').next().unwrap_or("").to_lowercase().contains(&query_lower) { 1 }
            else { 2 };
            Some((score, path))
        } else { None }
    }).collect();
    scored.sort_by_key(|(score, path)| (*score, path.len()));
    scored.into_iter().take(MAX_COMPLETIONS).map(|(_, path)| path.clone()).collect()
}
