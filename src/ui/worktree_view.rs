use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListItem, ListState, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::model::{App, AppMode, SessionStatus, WorktreeTab};
use crate::ui::theme;

pub fn render_worktree_view(app: &mut App, frame: &mut Frame, area: Rect) {
    let worktree_path = match &app.mode {
        AppMode::WorktreeView { worktree_path } => worktree_path,
        _ => return,
    };

    let sessions: Vec<_> = app.sessions.iter()
        .filter(|s| s.worktree_path == *worktree_path)
        .collect();

    let branch_name = sessions.first().map(|s| s.branch_name.as_str()).unwrap_or("?");

    let [header_area, tab_area, _sep, content_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
    ]).areas(area);

    // Store content height for half-page scroll
    let block = Block::default().padding(Padding::new(2, 1, 0, 0));
    let inner = block.inner(content_area);
    app.content_height = inner.height as usize;

    let terminal_count = app.terminals.iter()
        .filter(|t| t.worktree_path == *worktree_path)
        .count();
    render_header(branch_name, &sessions, frame, header_area);
    render_tabs(&app.worktree_view.active_tab, terminal_count, frame, tab_area);

    let worktree_path_clone = worktree_path.clone();
    match app.worktree_view.active_tab {
        WorktreeTab::Diff => {
            render_full_diff(&mut app.worktree_view.changes, frame, content_area);
        }
        WorktreeTab::Agents => {
            let agents_height = (sessions.len() as u16 + 1).min(15);
            let [agents_area, detail_area] = Layout::vertical([
                Constraint::Length(agents_height),
                Constraint::Min(3),
            ]).areas(content_area);
            render_agents_list(&sessions, app.worktree_view.selected_agent, frame, agents_area);
            render_agent_detail(&sessions, app.worktree_view.selected_agent, frame, detail_area);
        }
        WorktreeTab::Terminals => {
            let terminals: Vec<_> = app.terminals.iter()
                .filter(|t| t.worktree_path == *worktree_path_clone)
                .collect();
            let list_height = (terminals.len() as u16 + 1).min(15);
            let [list_area, preview_area] = Layout::vertical([
                Constraint::Length(list_height),
                Constraint::Min(3),
            ]).areas(content_area);
            render_terminals_list(&terminals, app.worktree_view.selected_terminal, frame, list_area);
            render_terminal_preview(&terminals, app.worktree_view.selected_terminal, frame, preview_area);
        }
    }
}

fn style_diff_line(line: &str) -> Style {
    if line.starts_with('+') && !line.starts_with("+++") {
        theme::diff_add_style()
    } else if line.starts_with('-') && !line.starts_with("---") {
        theme::diff_remove_style()
    } else if line.starts_with("@@") {
        theme::diff_hunk_style()
    } else if line.starts_with("diff ") || line.starts_with("index ") || line.starts_with("---") || line.starts_with("+++") {
        Style::default().fg(theme::TEXT_SECONDARY)
    } else if line.starts_with("new file:") {
        theme::diff_add_style()
    } else {
        theme::muted_style()
    }
}

fn render_full_diff(changes: &mut crate::model::ChangesState, frame: &mut Frame, area: Rect) {
    let block = Block::default().padding(Padding::new(2, 1, 0, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if changes.diff_content.is_empty() {
        frame.render_widget(
            Paragraph::new("NO CHANGES").style(theme::muted_style()),
            inner,
        );
        return;
    }

    let total = changes.diff_line_count;
    let visible = inner.height as usize;
    let max_scroll = total.saturating_sub(visible);
    let scroll = changes.diff_scroll.min(max_scroll);
    changes.diff_scroll = scroll;

    // Only style the visible lines (avoid allocating thousands of Line objects)
    let lines: Vec<Line> = changes.diff_content.lines()
        .skip(scroll)
        .take(visible)
        .map(|line| Line::styled(line.to_string(), style_diff_line(line)))
        .collect();

    let p = Paragraph::new(lines);
    frame.render_widget(p, inner);

    if total > visible {
        let mut sb_area = inner;
        sb_area.x = sb_area.right().saturating_sub(1);
        sb_area.width = 1;
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme::TEXT_DIMMED))
            .track_style(Style::default().fg(theme::BORDER));
        let mut state = ScrollbarState::new(max_scroll).position(scroll);
        frame.render_stateful_widget(scrollbar, sb_area, &mut state);
    }
}

fn render_header(
    branch_name: &str,
    sessions: &[&crate::model::Session],
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::default().padding(Padding::new(2, 2, 1, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut spans = vec![
        Span::styled(branch_name, theme::title_style()),
        Span::raw("  "),
    ];

    for session in sessions {
        let dot_color = if session.pending_permission_count > 0 {
            theme::STATUS_PERMISSION
        } else {
            theme::status_color(&session.status)
        };
        let name = session.agent_label.as_deref().unwrap_or("agent");
        spans.push(Span::styled("● ", Style::default().fg(dot_color)));
        spans.push(Span::styled(
            format!("{} ", name),
            Style::default().fg(theme::TEXT_DIMMED),
        ));
    }

    let total_cost: f64 = sessions.iter().map(|s| s.stats.cost_usd).sum();
    if total_cost > 0.0 {
        spans.push(Span::styled(
            format!(" ${:.2}", total_cost),
            theme::muted_style(),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn render_tabs(active_tab: &WorktreeTab, terminal_count: usize, frame: &mut Frame, area: Rect) {
    let term_label = if terminal_count > 0 {
        format!("TERMINALS ({})", terminal_count)
    } else {
        "TERMINALS".to_string()
    };
    let tabs = Line::from(vec![
        Span::raw("  "),
        if *active_tab == WorktreeTab::Diff {
            Span::styled("DIFF", theme::tab_active_style())
        } else {
            Span::styled("DIFF", theme::tab_inactive_style())
        },
        Span::styled("  ", theme::muted_style()),
        if *active_tab == WorktreeTab::Agents {
            Span::styled("AGENTS", theme::tab_active_style())
        } else {
            Span::styled("AGENTS", theme::tab_inactive_style())
        },
        Span::styled("  ", theme::muted_style()),
        if *active_tab == WorktreeTab::Terminals {
            Span::styled(term_label, theme::tab_active_style())
        } else {
            Span::styled(term_label, theme::tab_inactive_style())
        },
    ]);
    frame.render_widget(Paragraph::new(tabs), area);
}

fn render_agents_list(
    sessions: &[&crate::model::Session],
    selected: usize,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::default().padding(Padding::new(2, 1, 1, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if sessions.is_empty() {
        frame.render_widget(
            Paragraph::new("NO AGENTS").style(theme::muted_style()),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = sessions.iter().map(|session| {
        let dot_color = if session.pending_permission_count > 0 {
            theme::STATUS_PERMISSION
        } else {
            theme::status_color(&session.status)
        };

        let name = session.agent_label.as_deref().unwrap_or("agent");
        let status_text = format!("{}", session.status);

        let activity = if session.stats.last_activity_summary.is_empty() {
            String::new()
        } else {
            format!("  {}", session.stats.last_activity_summary)
        };

        let stats = if session.stats.num_turns > 0 || session.stats.cost_usd > 0.0 {
            format!("  {}t ${:.2}", session.stats.num_turns, session.stats.cost_usd)
        } else {
            String::new()
        };

        let line = Line::from(vec![
            Span::styled("● ", Style::default().fg(dot_color)),
            Span::styled(
                format!("{:<16}", name),
                Style::default().fg(theme::TEXT_SECONDARY),
            ),
            Span::styled(
                format!("{:<10}", status_text),
                Style::default().fg(dot_color),
            ),
            Span::styled(activity, theme::muted_style()),
            Span::styled(stats, theme::muted_style()),
        ]);

        ListItem::new(line)
    }).collect();

    let list = List::new(items)
        .highlight_style(Style::default().bg(theme::SELECTED_BG).fg(theme::TEXT_PRIMARY))
        .highlight_symbol("> ");

    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, inner, &mut state);
}

fn render_agent_detail(
    sessions: &[&crate::model::Session],
    selected: usize,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::default().padding(Padding::new(2, 1, 0, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sep_area = Rect::new(inner.x, inner.y, inner.width.min(60), 1);
    let sep = "─".repeat(sep_area.width as usize);
    frame.render_widget(
        Paragraph::new(sep).style(Style::default().fg(theme::BORDER)),
        sep_area,
    );

    let content_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1));

    let Some(session) = sessions.get(selected) else {
        frame.render_widget(
            Paragraph::new("SELECT AN AGENT").style(theme::muted_style()),
            content_area,
        );
        return;
    };

    let mut lines: Vec<Line> = Vec::new();
    let name = session.agent_label.as_deref().unwrap_or("agent");
    lines.push(Line::from(vec![
        Span::styled(name.to_string(), theme::title_style()),
        Span::raw("  "),
        Span::styled(format!("{}", session.status), Style::default().fg(theme::status_color(&session.status))),
        Span::raw("  "),
        Span::styled(
            format!("{}t ${:.2}", session.stats.num_turns, session.stats.cost_usd),
            theme::muted_style(),
        ),
    ]));
    lines.push(Line::raw(""));

    let start = session.output_log.len().saturating_sub(10);
    for entry in &session.output_log[start..] {
        match &entry.kind {
            crate::model::OutputKind::AssistantText(text) => {
                for line in text.lines().take(3) {
                    lines.push(Line::styled(line.to_string(), Style::default().fg(theme::TEXT_PRIMARY)));
                }
            }
            crate::model::OutputKind::ToolUse { name, input_summary } => {
                lines.push(Line::from(vec![
                    Span::styled("~ ", Style::default().fg(theme::TEXT_DIMMED)),
                    Span::styled(name.clone(), Style::default().fg(theme::TEXT_SECONDARY)),
                    Span::styled(format!(" {}", input_summary), theme::muted_style()),
                ]));
            }
            crate::model::OutputKind::ToolResult { tool_name, output_summary, success } => {
                let color = if *success { theme::STATUS_DONE } else { theme::STATUS_ERROR };
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default().fg(color)),
                    Span::styled(tool_name.clone(), Style::default().fg(theme::TEXT_DIMMED)),
                    Span::styled(format!(" {}", output_summary), theme::muted_style()),
                ]));
            }
            crate::model::OutputKind::Error(text) => {
                lines.push(Line::styled(text.to_string(), Style::default().fg(theme::STATUS_ERROR)));
            }
            crate::model::OutputKind::SystemMessage(text) => {
                lines.push(Line::styled(text.to_string(), theme::muted_style()));
            }
            crate::model::OutputKind::UserMessage(text) => {
                lines.push(Line::from(vec![
                    Span::styled("> ", Style::default().fg(theme::ACCENT)),
                    Span::styled(text.clone(), Style::default().fg(theme::ACCENT)),
                ]));
            }
        }
    }

    if lines.len() <= 2 {
        match session.status {
            SessionStatus::Creating => lines.push(Line::styled("CREATING WORKTREE...", theme::muted_style())),
            SessionStatus::Working => lines.push(Line::styled("WORKING...", theme::muted_style())),
            _ => lines.push(Line::styled("ENTER TO VIEW FULL LOG", theme::muted_style())),
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("ENTER", Style::default().fg(theme::ACCENT_DIM)),
        Span::styled(" OPEN CHAT  ", theme::muted_style()),
        Span::styled("A", Style::default().fg(theme::ACCENT_DIM)),
        Span::styled(" NEW AGENT", theme::muted_style()),
    ]));

    let p = Paragraph::new(lines);
    frame.render_widget(p, content_area);
}

fn render_terminals_list(
    terminals: &[&crate::model::EmbeddedTerminalState],
    selected: usize,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::default().padding(Padding::new(2, 1, 1, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if terminals.is_empty() {
        let lines = vec![
            Line::styled("NO TERMINALS", theme::muted_style()),
            Line::raw(""),
            Line::styled("T NEW SHELL  L LAZYGIT", theme::muted_style()),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let items: Vec<ListItem> = terminals.iter().map(|term| {
        let line = Line::from(vec![
            Span::styled("● ", Style::default().fg(theme::STATUS_WORKING)),
            Span::styled(
                format!("{:<16}", term.label),
                Style::default().fg(theme::TEXT_SECONDARY),
            ),
            Span::styled(
                format!("#{}", term.id),
                theme::muted_style(),
            ),
        ]);
        ListItem::new(line)
    }).collect();

    let list = List::new(items)
        .highlight_style(Style::default().bg(theme::SELECTED_BG).fg(theme::TEXT_PRIMARY))
        .highlight_symbol("> ");

    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, inner, &mut state);
}

fn render_terminal_preview(
    terminals: &[&crate::model::EmbeddedTerminalState],
    selected: usize,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::default().padding(Padding::new(2, 1, 0, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sep_area = Rect::new(inner.x, inner.y, inner.width.min(60), 1);
    let sep = "─".repeat(sep_area.width as usize);
    frame.render_widget(
        Paragraph::new(sep).style(Style::default().fg(theme::BORDER)),
        sep_area,
    );

    let content_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1));

    let Some(term) = terminals.get(selected) else {
        let lines = vec![
            Line::styled("T NEW SHELL  L LAZYGIT", theme::muted_style()),
        ];
        frame.render_widget(Paragraph::new(lines), content_area);
        return;
    };

    // Show last few lines from the vt100 screen as preview
    let screen = term.parser.screen();
    let rows = screen.size().0 as usize;
    let cols = screen.size().1 as usize;
    let preview_rows = (content_area.height as usize).saturating_sub(2).min(rows);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(term.label.clone(), theme::title_style()),
        Span::styled(format!("  #{}", term.id), theme::muted_style()),
    ]));
    lines.push(Line::raw(""));

    // Find last non-empty row for smarter preview
    let mut last_nonempty = 0;
    for row in 0..rows {
        for col in 0..cols {
            if let Some(cell) = screen.cell(row as u16, col as u16) {
                if !cell.contents().is_empty() && cell.contents() != " " {
                    last_nonempty = row;
                }
            }
        }
    }

    let start_row = (last_nonempty + 1).saturating_sub(preview_rows);
    for row in start_row..=(last_nonempty.min(start_row + preview_rows - 1)) {
        let mut spans: Vec<Span> = Vec::new();
        for col in 0..cols.min(content_area.width as usize) {
            if let Some(cell) = screen.cell(row as u16, col as u16) {
                let ch = cell.contents();
                let display = if ch.is_empty() { " " } else { &ch };
                let fg = crate::ui::embedded_terminal::vt100_color_to_ratatui(cell.fgcolor());
                let bg = crate::ui::embedded_terminal::vt100_color_to_ratatui(cell.bgcolor());
                let mut style = Style::default().fg(fg).bg(bg);
                if cell.bold() { style = style.add_modifier(Modifier::BOLD); }
                spans.push(Span::styled(display.to_string(), style));
            }
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "ENTER FOCUS  T NEW SHELL  L LAZYGIT  D CLOSE",
        theme::muted_style(),
    ));

    frame.render_widget(Paragraph::new(lines), content_area);
}
