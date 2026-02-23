use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::model::{App, AppMode, Dialog};
use crate::ui::theme;

pub fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let hints = if let Some(ref dialog) = app.dialog {
        match dialog {
            Dialog::Permission { .. } => "ENTER ALLOW  ESC DENY",
            Dialog::Help => "ESC OR ? TO CLOSE",
            Dialog::Confirm { .. } => "ENTER CONFIRM  ESC CANCEL",
        }
    } else if app.autocomplete.is_some() {
        "UP/DOWN NAVIGATE  TAB ACCEPT  ESC DISMISS"
    } else {
        match &app.mode {
            AppMode::NewSession => "TAB NEXT  ENTER SUBMIT  ESC CANCEL",
            AppMode::WorktreeView { .. } => {
                "TAB SWITCH  J/K NAV  ENTER SELECT  Q/ESC BACK  CTRL+Q QUIT"
            }
            AppMode::SessionChat { .. } => {
                "I INPUT  J/K SCROLL  Q/ESC BACK  C CHANGES  A AGENT  ? HELP"
            }
            AppMode::SessionChatInput { .. } => "ENTER SEND  @ FILES  ESC CANCEL",
            AppMode::NewSubAgent { .. } => "TAB NEXT  ENTER SUBMIT  ESC CANCEL",
            AppMode::Generate => {
                if app.generate_form.waiting {
                    "GENERATING PLAN..."
                } else {
                    "TAB NEXT  ENTER SUBMIT  ESC CANCEL"
                }
            }
            AppMode::EmbeddedTerminal { .. } => "CTRL+\\ DETACH  (TERMINAL KEEPS RUNNING)",
            AppMode::NoteView { .. } => "J/K SCROLL  E EDIT  D DELETE  Q/ESC BACK",
            AppMode::NewNote => "TAB NEXT  ENTER SUBMIT  ESC CANCEL",
        }
    };

    let bar = Paragraph::new(Line::from(vec![
        Span::raw(" "),
        Span::styled(hints, theme::muted_style()),
    ]));
    frame.render_widget(bar, area);
}
