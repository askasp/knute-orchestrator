use chrono::Utc;
use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListItem, Padding, Paragraph};

use crate::model::{App, AppMode, Note, SessionStatus};
use crate::ui::theme;

pub fn render_sidebar(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .bg(theme::SIDEBAR_BG)
        .padding(Padding::new(1, 1, 1, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [logo_area, _gap, list_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(inner);

    // Logo
    let logo = Paragraph::new(Line::from(vec![
        Span::styled(" KNUTE", Style::default().fg(theme::TEXT_SECONDARY)),
    ]))
    .style(Style::default().bg(theme::SIDEBAR_BG));
    frame.render_widget(logo, logo_area);

    let active_worktree_path = match &app.mode {
        AppMode::WorktreeView { worktree_path }
        | AppMode::NewSubAgent { worktree_path } => Some(worktree_path.clone()),
        AppMode::SessionChat { session_id }
        | AppMode::SessionChatInput { session_id } => {
            app.sessions.iter().find(|s| s.id == *session_id).map(|s| s.worktree_path.clone())
        }
        _ => None,
    };

    let active_note_index = match &app.mode {
        AppMode::NoteView { note_index } => Some(*note_index),
        _ => None,
    };

    let groups = app.worktree_groups();
    let items = build_sidebar_items(
        &groups,
        &app.sessions,
        &app.notes,
        active_worktree_path.as_ref(),
        active_note_index,
        app.selected_index,
        app.sidebar_focused,
    );

    if items.is_empty() {
        let empty = Paragraph::new(Line::styled("  no sessions", theme::muted_style()))
            .style(Style::default().bg(theme::SIDEBAR_BG));
        frame.render_widget(empty, list_area);
    } else {
        let list = List::new(items);
        frame.render_stateful_widget(list, list_area, &mut app.sidebar_state);
    }

    // Footer
    let footer_style = Style::default().bg(theme::SIDEBAR_BG).fg(theme::TEXT_DIMMED);
    let footer = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled("B", Style::default().fg(theme::ACCENT_DIM)),
        Span::raw(" BRANCH  "),
        Span::styled("N", Style::default().fg(theme::ACCENT_DIM)),
        Span::raw(" NOTE  "),
        Span::styled("G", Style::default().fg(theme::ACCENT_DIM)),
        Span::raw(" GEN"),
    ]))
    .style(footer_style);
    frame.render_widget(footer, footer_area);
}

fn build_sidebar_items<'a>(
    groups: &[crate::model::WorktreeGroup],
    sessions: &[crate::model::Session],
    notes: &[Note],
    active_worktree_path: Option<&std::path::PathBuf>,
    active_note_index: Option<usize>,
    selected_index: usize,
    sidebar_focused: bool,
) -> Vec<ListItem<'static>> {
    let mut items: Vec<ListItem> = Vec::new();

    // ── Worktree groups ────────────────────────────────────
    for (gi, group) in groups.iter().enumerate() {
        let is_active = active_worktree_path.map_or(false, |p| *p == group.worktree_path);
        let is_selected = gi == selected_index;

        let group_sessions: Vec<&crate::model::Session> = sessions.iter()
            .filter(|s| s.worktree_path == group.worktree_path)
            .collect();

        // Aggregate status dot: permission (yellow) > working (blue) > error (red) > done > idle
        let has_permission = group_sessions.iter().any(|s| s.pending_permission_count > 0);
        let has_attention = group_sessions.iter().any(|s| s.needs_attention());
        let has_working = group_sessions.iter().any(|s| s.status == SessionStatus::Working);
        let has_error = group_sessions.iter().any(|s| matches!(s.status, SessionStatus::Error(_)));
        let all_done = group_sessions.iter().all(|s| s.status == SessionStatus::Completed);

        let dot_color = if has_permission {
            theme::STATUS_PERMISSION
        } else if has_attention {
            theme::STATUS_ATTENTION
        } else if has_working {
            theme::status_color(&SessionStatus::Working)
        } else if has_error {
            theme::STATUS_ERROR
        } else if all_done {
            theme::status_color(&SessionStatus::Completed)
        } else {
            theme::status_color(&SessionStatus::Idle)
        };

        let name_style = if is_selected && sidebar_focused {
            Style::default().fg(theme::TEXT_PRIMARY)
        } else if is_active {
            Style::default().fg(theme::ACCENT_DIM)
        } else {
            Style::default().fg(theme::TEXT_SECONDARY)
        };

        let name_line = Line::from(vec![
            Span::raw("  "),
            Span::styled("● ", Style::default().fg(dot_color)),
            Span::styled(truncate_str(&group.branch_name, 22), name_style),
        ]);

        // Stats line: working count, total cost, elapsed time
        let working_count = group_sessions.iter().filter(|s| s.status == SessionStatus::Working).count();
        let total_cost: f64 = group_sessions.iter().map(|s| s.stats.cost_usd).sum();
        let earliest = group_sessions.iter().map(|s| s.created_at).min();
        let duration = earliest.map(format_duration).unwrap_or_default();

        let mut stats_parts = Vec::new();
        if working_count > 0 {
            stats_parts.push(format!("{}w", working_count));
        }
        let agent_count = group_sessions.len();
        if agent_count > 1 {
            stats_parts.push(format!("{}a", agent_count));
        }
        if total_cost > 0.0 {
            stats_parts.push(format!("${:.2}", total_cost));
        }
        if !duration.is_empty() {
            stats_parts.push(duration);
        }
        let stats_text = format!("    {}", stats_parts.join(" "));
        let stats_line = Line::styled(stats_text, theme::muted_style());

        let style = if is_selected && sidebar_focused {
            theme::sidebar_selected_style()
        } else {
            Style::default().bg(theme::SIDEBAR_BG)
        };

        items.push(ListItem::new(vec![name_line, stats_line]).style(style));
    }

    // ── Notes ─────────────────────────────────────────────
    if !notes.is_empty() {
        // Spacer + header
        items.push(
            ListItem::new(Line::raw("")).style(Style::default().bg(theme::SIDEBAR_BG)),
        );
        items.push(
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled("NOTES", Style::default().fg(theme::TEXT_DIMMED)),
            ]))
            .style(Style::default().bg(theme::SIDEBAR_BG)),
        );

        let num_groups = groups.len();
        let mut current_folder: Option<&str> = None;

        for (ni, note) in notes.iter().enumerate() {
            let global_index = num_groups + ni;
            let is_selected = global_index == selected_index;
            let is_active = active_note_index == Some(ni);

            // Folder header if changed
            if note.folder.as_deref() != current_folder {
                current_folder = note.folder.as_deref();
                if let Some(folder) = current_folder {
                    let folder_line = Line::from(vec![
                        Span::raw("    "),
                        Span::styled(truncate_str(folder, 22), Style::default().fg(theme::TEXT_DIMMED)),
                    ]);
                    items.push(
                        ListItem::new(folder_line).style(Style::default().bg(theme::SIDEBAR_BG)),
                    );
                }
            }

            let name_style = if is_selected && sidebar_focused {
                Style::default().fg(theme::TEXT_PRIMARY)
            } else if is_active {
                Style::default().fg(theme::ACCENT_DIM)
            } else {
                Style::default().fg(theme::TEXT_SECONDARY)
            };

            let indent = if note.folder.is_some() { "    " } else { "  " };
            let name_line = Line::from(vec![
                Span::raw(indent),
                Span::styled("· ", Style::default().fg(theme::TEXT_DIMMED)),
                Span::styled(
                    truncate_str(&note.title, if note.folder.is_some() { 20 } else { 22 }),
                    name_style,
                ),
            ]);

            let style = if is_selected && sidebar_focused {
                theme::sidebar_selected_style()
            } else {
                Style::default().bg(theme::SIDEBAR_BG)
            };

            items.push(ListItem::new(name_line).style(style));
        }
    }

    items
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{}~", truncated)
    } else {
        s.to_string()
    }
}

fn format_duration(created_at: chrono::DateTime<Utc>) -> String {
    let elapsed = Utc::now() - created_at;
    let secs = elapsed.num_seconds();
    if secs < 0 {
        return "0s".to_string();
    }
    if secs < 60 {
        return format!("{}s", secs);
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{}m", mins);
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{}h", hours);
    }
    let days = hours / 24;
    format!("{}d", days)
}
