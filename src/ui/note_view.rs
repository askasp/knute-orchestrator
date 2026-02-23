use ratatui::prelude::*;
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};

use crate::model::App;
use crate::notes;
use crate::ui::theme;

pub fn render_note_view(app: &App, frame: &mut Frame, area: Rect, note_index: usize) {
    let note = match app.notes.get(note_index) {
        Some(n) => n,
        None => {
            frame.render_widget(
                Paragraph::new("Note not found").style(theme::muted_style()),
                area,
            );
            return;
        }
    };

    let block = Block::default().padding(Padding::new(4, 4, 2, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [header_area, _gap, content_area, _gap2, footer_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    // Header: title + folder
    let title_text = if let Some(folder) = &note.folder {
        format!("{}/{}", folder, note.title)
    } else {
        note.title.clone()
    };
    frame.render_widget(
        Paragraph::new(title_text).style(theme::title_style()),
        header_area,
    );

    // Content
    let content = notes::read_note_content(&note.path);
    let lines: Vec<Line> = if content.is_empty() {
        vec![Line::styled("(empty)", theme::muted_style())]
    } else {
        content.lines().map(|l| Line::raw(l.to_string())).collect()
    };

    let total_lines = lines.len();
    let visible_height = content_area.height as usize;
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll = note.scroll_offset.min(max_scroll);

    frame.render_widget(
        Paragraph::new(lines)
            .scroll((scroll as u16, 0))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(theme::TEXT_SECONDARY)),
        content_area,
    );

    // Footer
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("e", Style::default().fg(theme::ACCENT_DIM)),
            Span::styled(" EDIT  ", theme::muted_style()),
            Span::styled("d", Style::default().fg(theme::ACCENT_DIM)),
            Span::styled(" DELETE  ", theme::muted_style()),
            Span::styled("esc", Style::default().fg(theme::ACCENT_DIM)),
            Span::styled(" BACK", theme::muted_style()),
        ])),
        footer_area,
    );
}
