use ratatui::prelude::*;
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};

use crate::model::{App, AppMode, SubAgentFormField};
use crate::ui::theme;

pub fn render_sub_agent_form(app: &App, frame: &mut Frame, area: Rect) {
    let worktree_path = match &app.mode {
        AppMode::NewSubAgent { worktree_path } => worktree_path,
        _ => return,
    };

    let branch_name = app
        .sessions
        .iter()
        .find(|s| s.worktree_path == *worktree_path)
        .map(|s| s.branch_name.as_str())
        .unwrap_or("?");

    let form = &app.sub_agent_form;

    let block = Block::default().padding(Padding::new(4, 4, 2, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [title_area, _g0, parent_area, _g1, label_label, label_input, _g2, prompt_label, prompt_input, _g3, skip_area, _g4, button_area] =
        Layout::vertical([
            Constraint::Length(2),
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
        Paragraph::new("NEW AGENT").style(theme::title_style()),
        title_area,
    );

    // Parent info
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("ON ", theme::muted_style()),
            Span::styled(branch_name, theme::secondary_style()),
        ])),
        parent_area,
    );

    // Label
    render_field(
        frame,
        label_label,
        label_input,
        "LABEL",
        &form.label,
        form.focused_field == SubAgentFormField::Label,
    );

    // Prompt
    render_field(
        frame,
        prompt_label,
        prompt_input,
        "PROMPT",
        &form.initial_prompt,
        form.focused_field == SubAgentFormField::Prompt,
    );

    // Skip permissions
    let checkbox = if form.skip_permissions { "[x]" } else { "[ ]" };
    let skip_style = if form.focused_field == SubAgentFormField::SkipPermissions {
        Style::default().fg(theme::TEXT_PRIMARY)
    } else {
        theme::secondary_style()
    };
    frame.render_widget(
        Paragraph::new(format!("{} AUTO-APPROVE", checkbox)).style(skip_style),
        skip_area,
    );

    // Submit button
    let btn_style = if form.focused_field == SubAgentFormField::Submit {
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

    let lines = vec![Line::styled(&display, text_style)];
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
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
