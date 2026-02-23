use ratatui::prelude::*;
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};

use crate::model::{App, NoteFormField};
use crate::ui::theme;

pub fn render_new_note(app: &App, frame: &mut Frame, area: Rect) {
    let form = &app.new_note_form;

    let block = Block::default().padding(Padding::new(4, 4, 2, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [title_area, _g0, name_label, name_input, _g1, folder_label, folder_input, _g2, button_area] =
        Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .areas(inner);

    // Title
    frame.render_widget(
        Paragraph::new("NEW NOTE").style(theme::title_style()),
        title_area,
    );

    // Title field
    render_field(
        frame,
        name_label,
        name_input,
        "TITLE",
        &form.title,
        form.focused_field == NoteFormField::Title,
    );

    // Folder field
    render_field(
        frame,
        folder_label,
        folder_input,
        "FOLDER",
        &form.folder,
        form.focused_field == NoteFormField::Folder,
    );

    // Submit button
    let btn_style = if form.focused_field == NoteFormField::Submit {
        theme::button_style()
    } else {
        theme::button_inactive_style()
    };
    frame.render_widget(
        Paragraph::new(" CREATE ").style(btn_style),
        button_area,
    );
}

fn render_field(
    frame: &mut Frame,
    label_area: Rect,
    input_area: Rect,
    label: &str,
    value: &str,
    focused: bool,
) {
    let label_style = if focused {
        theme::input_label_focused_style()
    } else {
        theme::input_label_style()
    };
    frame.render_widget(Paragraph::new(label).style(label_style), label_area);

    let display = if focused {
        format!("{}_", value)
    } else if value.is_empty() {
        String::new()
    } else {
        value.to_string()
    };

    let underline = if focused {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER
    };

    let text_style = if focused {
        Style::default().fg(theme::TEXT_PRIMARY)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    frame.render_widget(
        Paragraph::new(Line::styled(&display, text_style)).wrap(Wrap { trim: false }),
        input_area,
    );

    if input_area.bottom() < frame.area().bottom() {
        let line_area = Rect::new(input_area.x, input_area.bottom(), input_area.width.min(40), 1);
        let bar = "─".repeat(line_area.width as usize);
        frame.render_widget(
            Paragraph::new(bar).style(Style::default().fg(underline)),
            line_area,
        );
    }
}
