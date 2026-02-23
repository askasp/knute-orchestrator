use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, List, ListItem, ListState};

use crate::model::Autocomplete;
use crate::ui::theme;

/// Render the autocomplete popup above the given anchor area.
pub fn render_autocomplete(ac: &Autocomplete, frame: &mut Frame, anchor: Rect) {
    let frame_area = frame.area();

    if ac.matches.is_empty() {
        let height = 3_u16;
        let width = anchor.width.min(40);
        let x = anchor.x + 2;
        let clamped_width = width.min(frame_area.width.saturating_sub(x));
        let popup = Rect::new(
            x,
            anchor.y.saturating_sub(height),
            clamped_width,
            height,
        );
        frame.render_widget(Clear, popup);
        let block = Block::bordered()
            .border_style(Style::default().fg(theme::BORDER))
            .title(format!(" @{} ", ac.query))
            .title_style(Style::default().fg(theme::TEXT_SECONDARY));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        frame.render_widget(
            ratatui::widgets::Paragraph::new("NO MATCHES")
                .style(theme::muted_style()),
            inner,
        );
        return;
    }

    let num_visible = ac.matches.len().min(10);
    let height = num_visible as u16 + 2;
    let max_width = ac
        .matches
        .iter()
        .take(num_visible)
        .map(|m| m.len())
        .max()
        .unwrap_or(20) as u16
        + 6;
    let width = max_width.min(anchor.width).max(20);

    let x = anchor.x + 2;
    let clamped_width = width.min(frame_area.width.saturating_sub(x));
    let popup = Rect::new(
        x,
        anchor.y.saturating_sub(height),
        clamped_width,
        height,
    );

    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .border_style(Style::default().fg(theme::BORDER))
        .title(format!(" @{} ", ac.query))
        .title_style(Style::default().fg(theme::TEXT_SECONDARY));

    let items: Vec<ListItem> = ac
        .matches
        .iter()
        .take(num_visible)
        .map(|path| {
            ListItem::new(format!(" {}", path))
                .style(Style::default().fg(theme::TEXT_SECONDARY))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(theme::SELECTED_BG)
                .fg(theme::TEXT_PRIMARY),
        )
        .highlight_symbol(">");

    let mut state = ListState::default().with_selected(Some(ac.selected));
    frame.render_stateful_widget(list, popup, &mut state);
}
