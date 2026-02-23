pub mod autocomplete;
pub mod dialog;
pub mod embedded_terminal;
pub mod generate;
pub mod new_note;
pub mod new_session;
pub mod session_view;
pub mod sidebar;
pub mod status_bar;
pub mod sub_agent;
pub mod theme;
pub mod worktree_view;

use ratatui::prelude::*;
use ratatui::widgets::Clear;

use crate::model::{App, AppMode};

pub fn view(app: &mut App, frame: &mut Frame) {
    // Clear the entire frame to prevent terminal artifacts from bleeding through
    frame.render_widget(Clear, frame.area());
    frame.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(theme::BG)),
        frame.area(),
    );

    let [main_row, status_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let [sidebar_area, content_area] = Layout::horizontal([
        Constraint::Length(30),
        Constraint::Min(1),
    ])
    .areas(main_row);

    // Sidebar (always visible)
    sidebar::render_sidebar(app, frame, sidebar_area);

    // Main content panel
    let mode = app.mode.clone();
    match &mode {
        AppMode::NewSession => {
            new_session::render_new_session(app, frame, content_area);
        }
        AppMode::WorktreeView { .. } => {
            worktree_view::render_worktree_view(app, frame, content_area);
        }
        AppMode::SessionChat { .. } | AppMode::SessionChatInput { .. } => {
            session_view::render_session_view(app, frame, content_area);
        }
        AppMode::NewSubAgent { .. } => {
            sub_agent::render_sub_agent_form(app, frame, content_area);
        }
        AppMode::Generate => {
            generate::render_generate(app, frame, content_area);
        }
        AppMode::EmbeddedTerminal { .. } => {
            embedded_terminal::render_embedded_terminal(app, frame, content_area);
        }
        AppMode::NoteView { .. } => {
            // Notes open directly in $EDITOR — this is a transient state
        }
        AppMode::NewNote => {
            new_note::render_new_note(app, frame, content_area);
        }
    }

    // Status bar
    status_bar::render_status_bar(app, frame, status_area);

    // Dialog overlay (confirm, help)
    if let Some(ref dlg) = app.dialog {
        dialog::render_dialog(dlg, frame, frame.area());
    }

    // Autocomplete popup
    if let Some(ref ac) = app.autocomplete {
        let anchor = Rect::new(
            content_area.x,
            content_area.bottom().saturating_sub(3),
            content_area.width,
            3,
        );
        autocomplete::render_autocomplete(ac, frame, anchor);
    }
}
