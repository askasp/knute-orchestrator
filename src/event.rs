use crossterm::event::{Event, EventStream, KeyEventKind, MouseEventKind};
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::message::Message;

pub async fn run_event_reader(tx: mpsc::UnboundedSender<Message>) {
    let mut stream = EventStream::new();
    while let Some(result) = stream.next().await {
        match result {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                if tx.send(Message::Key(key)).is_err() {
                    break;
                }
            }
            Ok(Event::Mouse(mouse)) => {
                let delta = match mouse.kind {
                    MouseEventKind::ScrollUp => -3,
                    MouseEventKind::ScrollDown => 3,
                    _ => continue,
                };
                if tx.send(Message::MouseScroll { delta }).is_err() {
                    break;
                }
            }
            Ok(Event::Resize(cols, rows)) => {
                if tx.send(Message::Resize { cols, rows }).is_err() {
                    break;
                }
            }
            Err(_) => break,
            _ => {}
        }
    }
}

pub async fn run_tick(tx: mpsc::UnboundedSender<Message>, interval_ms: u64) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(interval_ms));
    loop {
        interval.tick().await;
        if tx.send(Message::Tick).is_err() {
            break;
        }
    }
}
