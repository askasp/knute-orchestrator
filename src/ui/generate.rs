use ratatui::prelude::*;
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};

use crate::model::{App, GenerateFormField};
use crate::ui::theme;

pub fn render_generate(app: &App, frame: &mut Frame, area: Rect) {
    let form = &app.generate_form;

    if form.waiting {
        render_waiting(frame, area);
        return;
    }

    let block = Block::default().padding(Padding::new(4, 4, 2, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let has_error = form.error.is_some();

    let [title_area, _g0, desc_label, desc_input, _g1, ctx_label, ctx_input, _g2, skip_area, _g3, button_area, _g4, error_area] =
        Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(if has_error { 3 } else { 0 }),
        ])
        .areas(inner);

    // Title
    frame.render_widget(
        Paragraph::new("GENERATE").style(theme::title_style()),
        title_area,
    );

    // Description
    let label_style = if form.focused_field == GenerateFormField::Description {
        theme::input_label_focused_style()
    } else {
        theme::input_label_style()
    };
    frame.render_widget(
        Paragraph::new("DESCRIBE WHAT YOU WANT").style(label_style),
        desc_label,
    );

    let display = if form.focused_field == GenerateFormField::Description {
        format!("{}_", form.description)
    } else if form.description.is_empty() {
        String::new()
    } else {
        form.description.clone()
    };

    let text_style = if form.focused_field == GenerateFormField::Description {
        Style::default().fg(theme::TEXT_PRIMARY)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    frame.render_widget(
        Paragraph::new(display).style(text_style).wrap(Wrap { trim: false }),
        desc_input,
    );

    // Underline for description
    let underline = if form.focused_field == GenerateFormField::Description {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER
    };
    if desc_input.bottom() < frame.area().bottom() {
        let line_area = Rect::new(desc_input.x, desc_input.bottom(), desc_input.width.min(40), 1);
        let bar = "─".repeat(line_area.width as usize);
        frame.render_widget(
            Paragraph::new(bar).style(Style::default().fg(underline)),
            line_area,
        );
    }

    // Context
    let ctx_label_style = if form.focused_field == GenerateFormField::Context {
        theme::input_label_focused_style()
    } else {
        theme::input_label_style()
    };
    frame.render_widget(
        Paragraph::new("CONTEXT FILES (@ to add)").style(ctx_label_style),
        ctx_label,
    );

    let ctx_display = if form.focused_field == GenerateFormField::Context {
        format!("{}_", form.context)
    } else if form.context.is_empty() {
        String::new()
    } else {
        form.context.clone()
    };

    let ctx_text_style = if form.focused_field == GenerateFormField::Context {
        Style::default().fg(theme::TEXT_PRIMARY)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    frame.render_widget(
        Paragraph::new(ctx_display).style(ctx_text_style).wrap(Wrap { trim: false }),
        ctx_input,
    );

    // Underline for context
    let ctx_underline = if form.focused_field == GenerateFormField::Context {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER
    };
    if ctx_input.bottom() < frame.area().bottom() {
        let line_area = Rect::new(ctx_input.x, ctx_input.bottom(), ctx_input.width.min(40), 1);
        let bar = "─".repeat(line_area.width as usize);
        frame.render_widget(
            Paragraph::new(bar).style(Style::default().fg(ctx_underline)),
            line_area,
        );
    }

    // Skip permissions
    let checkbox = if form.skip_permissions { "[x]" } else { "[ ]" };
    let skip_style = if form.focused_field == GenerateFormField::SkipPermissions {
        Style::default().fg(theme::TEXT_PRIMARY)
    } else {
        theme::secondary_style()
    };
    frame.render_widget(
        Paragraph::new(format!("{} AUTO-APPROVE", checkbox)).style(skip_style),
        skip_area,
    );

    // Submit button
    let btn_style = if form.focused_field == GenerateFormField::Submit {
        theme::button_style()
    } else {
        theme::button_inactive_style()
    };
    frame.render_widget(
        Paragraph::new(" GENERATE ").style(btn_style),
        button_area,
    );

    // Error message
    if let Some(ref err) = form.error {
        frame.render_widget(
            Paragraph::new(err.as_str())
                .style(Style::default().fg(theme::DIFF_REMOVE))
                .wrap(Wrap { trim: false }),
            error_area,
        );
    }
}

fn render_waiting(frame: &mut Frame, area: Rect) {
    let block = Block::default().padding(Padding::new(4, 4, 2, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [_top, msg_area, _bottom] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(inner);

    frame.render_widget(
        Paragraph::new("GENERATING PLAN...")
            .style(theme::secondary_style())
            .alignment(Alignment::Center),
        msg_area,
    );
}
