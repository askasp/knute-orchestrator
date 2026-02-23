use ratatui::prelude::*;
use ratatui::widgets::{Block, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};

use crate::model::{App, AppMode, OutputKind, Session};
use crate::ui::theme;

pub fn render_session_view(app: &mut App, frame: &mut Frame, area: Rect) {
    let session_id = match &app.mode {
        AppMode::SessionChat { session_id } | AppMode::SessionChatInput { session_id } => {
            Some(*session_id)
        }
        _ => None,
    };

    let Some(sid) = session_id else {
        frame.render_widget(
            Paragraph::new("session not found").style(theme::muted_style()),
            area,
        );
        return;
    };

    let [header_area, _sep, output_area, input_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(3),
    ])
    .areas(area);

    // Store content height for half-page scroll calculations
    let block = Block::default().padding(Padding::new(2, 1, 0, 0));
    let inner = block.inner(output_area);
    app.content_height = inner.height as usize;

    if let Some(session) = app.sessions.iter().find(|s| s.id == sid) {
        render_session_header(session, frame, header_area);
    }
    render_output_log(app, sid, frame, output_area);
    render_input_area(app, frame, input_area);
}

fn render_session_header(session: &Session, frame: &mut Frame, area: Rect) {
    let block = Block::default().padding(Padding::new(2, 2, 1, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let status_col = theme::status_color(&session.status);

    // Breadcrumb: branch > agent_label
    let mut spans = vec![
        Span::styled(&session.branch_name, theme::title_style()),
    ];
    if let Some(ref label) = session.agent_label {
        spans.push(Span::styled(" > ", theme::muted_style()));
        spans.push(Span::styled(label, theme::title_style()));
    }
    spans.extend([
        Span::raw("  "),
        Span::styled(
            format!("{}", session.status),
            Style::default().fg(status_col),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} turns  ${:.2}", session.stats.num_turns, session.stats.cost_usd),
            theme::muted_style(),
        ),
    ]);

    let line = Line::from(spans);

    frame.render_widget(Paragraph::new(line), inner);
}

fn render_output_log(app: &mut App, session_id: uuid::Uuid, frame: &mut Frame, area: Rect) {
    let block = Block::default().padding(Padding::new(2, 1, 0, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let session_idx = match app.sessions.iter().position(|s| s.id == session_id) {
        Some(idx) => idx,
        None => return,
    };

    if app.sessions[session_idx].output_log.is_empty() {
        let p = Paragraph::new("WAITING FOR OUTPUT...")
            .style(theme::muted_style());
        frame.render_widget(p, inner);
        return;
    }

    let output_log = &app.sessions[session_idx].output_log;
    let visible_height = inner.height as usize;
    let auto_scroll = app.sessions[session_idx].auto_scroll;

    // For performance: only render a window of entries, not the entire log.
    // When auto-scrolling (viewing the end), render only the tail.
    // When scrolled up, render enough entries around the scroll position.
    const MAX_RENDER_ENTRIES: usize = 150;
    let (entries_to_render, skipped_entries) = if output_log.len() <= MAX_RENDER_ENTRIES {
        (output_log.as_slice(), 0)
    } else if auto_scroll {
        let start = output_log.len() - MAX_RENDER_ENTRIES;
        (&output_log[start..], start)
    } else {
        // Rough estimate: find which entries are near the scroll position.
        // Each entry produces ~2 lines on average.
        let scroll_offset = app.sessions[session_idx].scroll_offset;
        let est_entry = (scroll_offset / 2).min(output_log.len().saturating_sub(1));
        let half_window = MAX_RENDER_ENTRIES / 2;
        let start = est_entry.saturating_sub(half_window);
        let end = (start + MAX_RENDER_ENTRIES).min(output_log.len());
        (&output_log[start..end], start)
    };

    let mut lines: Vec<Line> = Vec::new();
    if skipped_entries > 0 {
        lines.push(Line::styled(
            format!("  ... {} earlier entries ...", skipped_entries),
            theme::muted_style(),
        ));
    }
    for entry in entries_to_render {
        match &entry.kind {
            OutputKind::AssistantText(text) => {
                for line in text.lines() {
                    lines.push(Line::styled(
                        line.to_string(),
                        Style::default().fg(theme::TEXT_PRIMARY),
                    ));
                }
                lines.push(Line::raw(""));
            }
            OutputKind::ToolUse { name, input_summary } => {
                lines.push(Line::from(vec![
                    Span::styled("~ ", Style::default().fg(theme::TEXT_DIMMED)),
                    Span::styled(name.clone(), Style::default().fg(theme::TEXT_SECONDARY)),
                    Span::styled(
                        format!(" {}", input_summary),
                        theme::muted_style(),
                    ),
                ]));
            }
            OutputKind::ToolResult {
                tool_name,
                output_summary,
                success,
            } => {
                let color = if *success {
                    theme::STATUS_DONE
                } else {
                    theme::STATUS_ERROR
                };
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default().fg(color)),
                    Span::styled(tool_name.clone(), Style::default().fg(theme::TEXT_DIMMED)),
                    Span::styled(format!(" {}", output_summary), theme::muted_style()),
                ]));
                lines.push(Line::raw(""));
            }
            OutputKind::UserMessage(text) => {
                lines.push(Line::from(vec![
                    Span::styled("> ", Style::default().fg(theme::ACCENT)),
                    Span::styled(text.clone(), Style::default().fg(theme::ACCENT)),
                ]));
                lines.push(Line::raw(""));
            }
            OutputKind::SystemMessage(text) => {
                lines.push(Line::styled(
                    text.to_string(),
                    theme::muted_style(),
                ));
            }
            OutputKind::Error(text) => {
                lines.push(Line::styled(
                    text.to_string(),
                    Style::default().fg(theme::STATUS_ERROR),
                ));
                lines.push(Line::raw(""));
            }
        }
    }

    let total_lines: usize = if inner.width == 0 {
        lines.len()
    } else {
        lines.iter().map(|l| {
            let w = l.width();
            if w == 0 { 1 } else { (w + inner.width as usize - 1) / inner.width as usize }
        }).sum()
    };
    let max_scroll = total_lines.saturating_sub(visible_height);

    // Store max_scroll so scroll handlers can resolve from auto_scroll position
    let session = &mut app.sessions[session_idx];
    session.rendered_max_scroll = max_scroll;

    // When auto-scrolling, always show the bottom
    let scroll = if session.auto_scroll {
        session.scroll_offset = usize::MAX;
        max_scroll
    } else {
        // Scroll is relative to the rendered window, not the full log
        let scroll_offset = if skipped_entries > 0 {
            // Adjust: the user's global scroll offset minus the lines we skipped
            let skipped_lines_estimate = skipped_entries * 2; // rough estimate
            session.scroll_offset.saturating_sub(skipped_lines_estimate)
        } else {
            session.scroll_offset
        };
        let scroll = scroll_offset.min(max_scroll);
        if scroll_offset >= max_scroll {
            session.auto_scroll = true;
            session.scroll_offset = usize::MAX;
            max_scroll
        } else {
            scroll
        }
    };

    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    frame.render_widget(p, inner);

    if total_lines > visible_height {
        let mut sb_area = inner;
        sb_area.x = sb_area.right().saturating_sub(1);
        sb_area.width = 1;
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme::TEXT_DIMMED))
            .track_style(Style::default().fg(theme::BORDER));
        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scroll);
        frame.render_stateful_widget(scrollbar, sb_area, &mut scrollbar_state);
    }
}

fn render_input_area(app: &App, frame: &mut Frame, area: Rect) {
    let is_input_mode = matches!(app.mode, AppMode::SessionChatInput { .. });

    let block = Block::default().padding(Padding::new(2, 1, 0, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Subtle separator line
    let sep_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let sep = "─".repeat(sep_area.width.min(60) as usize);
    frame.render_widget(
        Paragraph::new(sep).style(Style::default().fg(theme::BORDER)),
        sep_area,
    );

    let text_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1));

    let display_text = if is_input_mode {
        format!("> {}_", app.input_buffer)
    } else {
        "  I TO TYPE...".to_string()
    };

    let style = if is_input_mode {
        Style::default().fg(theme::TEXT_PRIMARY)
    } else {
        theme::muted_style()
    };

    frame.render_widget(Paragraph::new(display_text).style(style), text_area);
}
