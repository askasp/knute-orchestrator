use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::model::Dialog;
use crate::ui::theme;

pub fn render_dialog(dialog: &Dialog, frame: &mut Frame, area: Rect) {
    match dialog {
        Dialog::Confirm { title, message, .. } => render_confirm(title, message, frame, area),
        Dialog::Help => render_help(frame, area),
        Dialog::Permission { tool_name, tool_input, .. } => {
            render_permission(tool_name, tool_input, frame, area)
        }
    }
}

fn centered_rect(width_pct: u16, height_pct: u16, area: Rect) -> Rect {
    let [_, center_v, _] = Layout::vertical([
        Constraint::Percentage((100 - height_pct) / 2),
        Constraint::Percentage(height_pct),
        Constraint::Percentage((100 - height_pct) / 2),
    ])
    .areas(area);

    let [_, center, _] = Layout::horizontal([
        Constraint::Percentage((100 - width_pct) / 2),
        Constraint::Percentage(width_pct),
        Constraint::Percentage((100 - width_pct) / 2),
    ])
    .areas(center_v);

    center
}

fn render_confirm(title: &str, message: &str, frame: &mut Frame, area: Rect) {
    let dialog_area = centered_rect(45, 20, area);
    frame.render_widget(Clear, dialog_area);

    let block = Block::bordered()
        .title(format!(" {} ", title.to_uppercase()))
        .title_style(Style::default().fg(theme::TEXT_SECONDARY))
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let text = vec![
        Line::raw(""),
        Line::styled(message.to_string(), Style::default().fg(theme::TEXT_PRIMARY)),
        Line::raw(""),
        Line::styled("ENTER CONFIRM    ESC CANCEL", theme::muted_style()),
    ];
    let p = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    frame.render_widget(p, inner);
}

fn render_permission(tool_name: &str, tool_input: &str, frame: &mut Frame, area: Rect) {
    let dialog_area = centered_rect(50, 30, area);
    frame.render_widget(Clear, dialog_area);

    let block = Block::bordered()
        .title(" PERMISSION REQUEST ")
        .title_style(Style::default().fg(theme::STATUS_PERMISSION))
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let truncated_input: String = if tool_input.chars().count() > 200 {
        let t: String = tool_input.chars().take(200).collect();
        format!("{}...", t)
    } else {
        tool_input.to_string()
    };

    let text = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Tool: ", theme::muted_style()),
            Span::styled(tool_name, Style::default().fg(theme::TEXT_PRIMARY)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  ", theme::muted_style()),
            Span::styled(truncated_input, Style::default().fg(theme::TEXT_SECONDARY)),
        ]),
        Line::raw(""),
        Line::raw(""),
        Line::styled("  ENTER ALLOW    ESC DENY", theme::muted_style()),
    ];
    let p = Paragraph::new(text).wrap(Wrap { trim: false });
    frame.render_widget(p, inner);
}

fn render_help(frame: &mut Frame, area: Rect) {
    let dialog_area = centered_rect(50, 65, area);
    frame.render_widget(Clear, dialog_area);

    let block = Block::bordered()
        .title(" HELP ")
        .title_style(Style::default().fg(theme::TEXT_SECONDARY))
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let section = |s: &str| -> Line {
        Line::styled(format!(" {}", s.to_uppercase()), Style::default().fg(theme::ACCENT))
    };
    let key = |k: &str, desc: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("   {:10}", k), Style::default().fg(theme::TEXT_SECONDARY)),
            Span::styled(desc.to_string(), theme::muted_style()),
        ])
    };

    let text = vec![
        Line::raw(""),
        section("global"),
        key("b", "new branch/worktree"),
        key("n", "new note"),
        key("a", "new agent"),
        key("d", "delete worktree/note"),
        key("1-9", "jump to worktree"),
        key("g", "generate agents"),
        key("T", "terminal in worktree"),
        key("L", "lazygit in worktree"),
        key("?", "this help"),
        key("ctrl+q", "quit"),
        Line::raw(""),
        section("navigation"),
        key("j/k", "navigate / scroll"),
        key("enter", "open / select"),
        key("l/h", "focus content / sidebar"),
        key("q/esc", "go back"),
        Line::raw(""),
        section("worktree view"),
        key("Tab", "switch diff / agents / terminals"),
        key("T", "terminal"),
        key("L", "lazygit"),
        Line::raw(""),
        section("agent chat"),
        key("i", "input mode"),
        key("g/G", "top / bottom"),
        key("c", "changes (diff tab)"),
        key("ctrl+c", "interrupt"),
        Line::raw(""),
        section("note view"),
        key("e", "edit in $EDITOR"),
        key("d", "delete note"),
        Line::raw(""),
        section("input"),
        key("@", "file autocomplete"),
        key("enter", "send"),
        key("esc", "cancel"),
        key("ctrl+u", "clear"),
        Line::raw(""),
        Line::styled("  ESC OR ? TO CLOSE", theme::muted_style()),
    ];
    let p = Paragraph::new(text);
    frame.render_widget(p, inner);
}
