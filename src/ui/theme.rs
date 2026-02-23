use ratatui::style::{Color, Style};

// ── Backgrounds ──────────────────────────────────────────
pub const BG: Color = Color::Rgb(10, 10, 10);
pub const SIDEBAR_BG: Color = Color::Rgb(15, 15, 15);
pub const SELECTED_BG: Color = Color::Rgb(26, 26, 26);

// ── Text ─────────────────────────────────────────────────
pub const TEXT_PRIMARY: Color = Color::Rgb(250, 250, 250);
pub const TEXT_SECONDARY: Color = Color::Rgb(115, 115, 115);
pub const TEXT_DIMMED: Color = Color::Rgb(64, 64, 64);

// ── Accent ───────────────────────────────────────────────
pub const ACCENT: Color = Color::Rgb(90, 140, 210);
pub const ACCENT_DIM: Color = Color::Rgb(60, 95, 145);

// ── Status ───────────────────────────────────────────────
pub const STATUS_WORKING: Color = Color::Rgb(90, 140, 210);
pub const STATUS_IDLE: Color = Color::Rgb(115, 115, 115);
pub const STATUS_DONE: Color = Color::Rgb(250, 250, 250);
pub const STATUS_ERROR: Color = Color::Rgb(195, 95, 95);
pub const STATUS_CREATING: Color = Color::Rgb(64, 64, 64);
pub const STATUS_ATTENTION: Color = Color::Rgb(250, 250, 250);
pub const STATUS_PERMISSION: Color = Color::Rgb(210, 170, 70);

// ── Diff ─────────────────────────────────────────────────
pub const DIFF_ADD: Color = Color::Rgb(105, 180, 120);
pub const DIFF_REMOVE: Color = Color::Rgb(195, 95, 95);
pub const DIFF_HUNK: Color = Color::Rgb(115, 115, 115);

// ── Borders ──────────────────────────────────────────────
pub const BORDER: Color = Color::Rgb(38, 38, 38);
pub const BORDER_FOCUSED: Color = Color::Rgb(90, 140, 210);

// ── Style helpers ────────────────────────────────────────

pub fn title_style() -> Style {
    Style::default().fg(TEXT_PRIMARY)
}

pub fn muted_style() -> Style {
    Style::default().fg(TEXT_DIMMED)
}

pub fn secondary_style() -> Style {
    Style::default().fg(TEXT_SECONDARY)
}

pub fn sidebar_selected_style() -> Style {
    Style::default().bg(SELECTED_BG).fg(TEXT_PRIMARY)
}

pub fn tab_active_style() -> Style {
    Style::default().fg(ACCENT)
}

pub fn tab_inactive_style() -> Style {
    Style::default().fg(TEXT_DIMMED)
}

pub fn input_label_style() -> Style {
    Style::default().fg(TEXT_DIMMED)
}

pub fn input_label_focused_style() -> Style {
    Style::default().fg(TEXT_SECONDARY)
}

pub fn button_style() -> Style {
    Style::default().fg(BG).bg(ACCENT)
}

pub fn button_inactive_style() -> Style {
    Style::default().fg(TEXT_DIMMED).bg(SELECTED_BG)
}

pub fn diff_add_style() -> Style {
    Style::default().fg(DIFF_ADD)
}

pub fn diff_remove_style() -> Style {
    Style::default().fg(DIFF_REMOVE)
}

pub fn diff_hunk_style() -> Style {
    Style::default().fg(DIFF_HUNK)
}

pub fn status_color(status: &crate::model::SessionStatus) -> Color {
    match status {
        crate::model::SessionStatus::Creating => STATUS_CREATING,
        crate::model::SessionStatus::Idle => STATUS_IDLE,
        crate::model::SessionStatus::Working => STATUS_WORKING,
        crate::model::SessionStatus::Completed => STATUS_DONE,
        crate::model::SessionStatus::Error(_) => STATUS_ERROR,
    }
}
