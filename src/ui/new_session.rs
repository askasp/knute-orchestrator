use ratatui::prelude::*;
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};

use crate::model::{App, FormField};
use crate::ui::theme;

pub fn render_new_session(app: &App, frame: &mut Frame, area: Rect) {
    let form = &app.new_session_form;

    let block = Block::default().padding(Padding::new(4, 4, 2, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [title_area, _g0, branch_label, branch_input, _g1, base_label, base_input, _g2, prompt_label, prompt_input, _g3, skip_area, _g4, button_area] =
        Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .areas(inner);

    // Title
    frame.render_widget(
        Paragraph::new("NEW SESSION").style(theme::title_style()),
        title_area,
    );

    // Branch name
    render_field(
        frame,
        branch_label,
        branch_input,
        "BRANCH",
        &form.branch_name,
        form.focused_field == FormField::BranchName,
    );

    // Base branch
    render_field(
        frame,
        base_label,
        base_input,
        "BASE",
        &form.base_branch,
        form.focused_field == FormField::BaseBranch,
    );

    // Initial prompt
    render_field(
        frame,
        prompt_label,
        prompt_input,
        "PROMPT",
        &form.initial_prompt,
        form.focused_field == FormField::Prompt,
    );

    // Skip permissions
    let checkbox = if form.skip_permissions { "[x]" } else { "[ ]" };
    let skip_style = if form.focused_field == FormField::SkipPermissions {
        Style::default().fg(theme::TEXT_PRIMARY)
    } else {
        theme::secondary_style()
    };
    frame.render_widget(
        Paragraph::new(format!("{} AUTO-APPROVE", checkbox)).style(skip_style),
        skip_area,
    );

    // Submit button
    let btn_style = if form.focused_field == FormField::Submit {
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

    // Render the value with an underline character below
    let lines = vec![
        Line::styled(&display, text_style),
    ];
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        input_area,
    );

    // Draw a subtle underline below the input
    if input_area.bottom() < frame.area().bottom() {
        let line_area = Rect::new(input_area.x, input_area.bottom(), input_area.width.min(40), 1);
        let bar = "─".repeat(line_area.width as usize);
        frame.render_widget(
            Paragraph::new(bar).style(Style::default().fg(underline)),
            line_area,
        );
    }
}
