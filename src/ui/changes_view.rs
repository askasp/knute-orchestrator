use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem, ListState, Padding, Paragraph, Block, Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::model::ChangesState;
use crate::ui::theme;

pub fn file_list_height(changes: &ChangesState) -> u16 {
    let count = changes.files.len();
    (count as u16 + 2).min(10)
}

pub fn render_file_list(changes: &ChangesState, frame: &mut Frame, area: Rect) {
    let block = Block::default().padding(Padding::new(2, 1, 1, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if changes.files.is_empty() {
        frame.render_widget(
            Paragraph::new("NO CHANGES").style(theme::muted_style()),
            inner,
        );
        return;
    }

    let header_area = Rect::new(inner.x, inner.y, inner.width, 1);
    frame.render_widget(
        Paragraph::new(format!("{} CHANGED", changes.files.len())).style(theme::muted_style()),
        header_area,
    );

    let list_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1));

    let items: Vec<ListItem> = changes.files.iter().map(|file| {
        let status_color = match file.status {
            'A' => theme::DIFF_ADD,
            'D' => theme::DIFF_REMOVE,
            _ => theme::TEXT_DIMMED,
        };

        let line = Line::from(vec![
            Span::styled(format!("{} ", file.status), Style::default().fg(status_color)),
            Span::styled(&file.path, Style::default().fg(theme::TEXT_SECONDARY)),
            Span::raw(" "),
            Span::styled(format!("+{}", file.additions), theme::diff_add_style()),
            Span::raw(" "),
            Span::styled(format!("-{}", file.deletions), theme::diff_remove_style()),
        ]);

        ListItem::new(line)
    }).collect();

    let list = List::new(items)
        .highlight_style(Style::default().bg(theme::SELECTED_BG).fg(theme::TEXT_PRIMARY))
        .highlight_symbol("> ");

    let mut state = ListState::default().with_selected(Some(changes.selected_file));
    frame.render_stateful_widget(list, list_area, &mut state);
}

pub fn render_diff_preview(changes: &ChangesState, frame: &mut Frame, area: Rect) {
    let block = Block::default().padding(Padding::new(2, 1, 0, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Separator
    let sep_area = Rect::new(inner.x, inner.y, inner.width.min(60), 1);
    let sep = "─".repeat(sep_area.width as usize);
    frame.render_widget(
        Paragraph::new(sep).style(Style::default().fg(theme::BORDER)),
        sep_area,
    );

    let content_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1));

    if changes.diff_content.is_empty() {
        frame.render_widget(
            Paragraph::new("SELECT A FILE TO SEE DIFF").style(theme::muted_style()),
            content_area,
        );
        return;
    }

    if let Some(file) = changes.files.get(changes.selected_file) {
        let header_area = Rect::new(content_area.x, content_area.y, content_area.width, 1);
        frame.render_widget(
            Paragraph::new(&*file.path).style(theme::secondary_style()),
            header_area,
        );
    }

    let diff_area = Rect::new(
        content_area.x, content_area.y + 1,
        content_area.width, content_area.height.saturating_sub(1),
    );

    let lines: Vec<Line> = changes.diff_content.lines().map(|line| {
        if line.starts_with('+') && !line.starts_with("+++") {
            Line::styled(line.to_string(), theme::diff_add_style())
        } else if line.starts_with('-') && !line.starts_with("---") {
            Line::styled(line.to_string(), theme::diff_remove_style())
        } else if line.starts_with("@@") {
            Line::styled(line.to_string(), theme::diff_hunk_style())
        } else {
            Line::styled(line.to_string(), theme::muted_style())
        }
    }).collect();

    let total = lines.len();
    let visible = diff_area.height as usize;
    let max_scroll = total.saturating_sub(visible);
    let scroll = changes.diff_scroll.min(max_scroll);

    let p = Paragraph::new(lines).scroll((scroll as u16, 0));
    frame.render_widget(p, diff_area);

    if total > visible {
        let mut sb_area = diff_area;
        sb_area.x = sb_area.right().saturating_sub(1);
        sb_area.width = 1;
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme::TEXT_DIMMED))
            .track_style(Style::default().fg(theme::BORDER));
        let mut state = ScrollbarState::new(max_scroll).position(scroll);
        frame.render_stateful_widget(scrollbar, sb_area, &mut state);
    }
}
