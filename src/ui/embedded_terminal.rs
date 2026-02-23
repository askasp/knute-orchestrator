use ratatui::prelude::*;
use ratatui::widgets::{Block, Padding, Paragraph};

use crate::model::{App, AppMode};
use crate::ui::theme;

pub fn render_embedded_terminal(app: &App, frame: &mut Frame, area: Rect) {
    let terminal_id = match &app.mode {
        AppMode::EmbeddedTerminal { terminal_id, .. } => *terminal_id,
        _ => return,
    };
    let Some(term) = app.terminals.iter().find(|t| t.id == terminal_id) else {
        return;
    };

    let [content_area, hint_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let screen = term.parser.screen();
    let rows = screen.size().0 as usize;
    let cols = screen.size().1 as usize;
    let cursor = screen.cursor_position();

    let mut lines: Vec<Line> = Vec::with_capacity(rows);

    for row in 0..rows {
        let mut spans: Vec<Span> = Vec::new();
        let mut col = 0usize;

        while col < cols {
            let cell = screen.cell(row as u16, col as u16);
            let Some(cell) = cell else {
                col += 1;
                continue;
            };

            let ch = cell.contents();
            let is_cursor = row as u16 == cursor.0 && col as u16 == cursor.1;

            let fg = vt100_color_to_ratatui(cell.fgcolor());
            let bg = if is_cursor {
                Color::Rgb(200, 200, 200)
            } else {
                vt100_color_to_ratatui(cell.bgcolor())
            };

            let mut style = Style::default().fg(if is_cursor { Color::Rgb(10, 10, 10) } else { fg }).bg(bg);

            if cell.bold() {
                style = style.add_modifier(Modifier::BOLD);
            }
            if cell.italic() {
                style = style.add_modifier(Modifier::ITALIC);
            }
            if cell.underline() {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if cell.inverse() {
                style = style.add_modifier(Modifier::REVERSED);
            }

            let display = if ch.is_empty() { " ".to_string() } else { ch.to_string() };
            spans.push(Span::styled(display, style));
            col += 1;
        }

        lines.push(Line::from(spans));
    }

    let block = Block::default().padding(Padding::ZERO);
    let p = Paragraph::new(lines).block(block);
    frame.render_widget(p, content_area);

    // Hint bar
    let hint = Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("TERMINAL: {}", term.label), Style::default().fg(theme::ACCENT)),
        Span::styled("  Ctrl+\\ detach", theme::muted_style()),
    ]);
    frame.render_widget(Paragraph::new(hint), hint_area);
}

pub fn vt100_color_to_ratatui(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
