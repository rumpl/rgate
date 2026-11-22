use gpui::{Context, Entity, Window};
use gpui_component::input::TextareaState;

const MAX_MESSAGES: usize = 200;
const VISIBLE_MESSAGES: usize = 30;

/// Owns the application status line and bounded message log view.
pub struct ConsoleLog {
    pub messages: Vec<String>,
    pub log_view: Entity<TextareaState>,
    pub status: String,
}

impl ConsoleLog {
    pub fn new<T: 'static>(window: &mut Window, cx: &mut Context<T>) -> Self {
        Self {
            messages: vec![
                "RGate 0.1 — TkGate-style circuit editing, powered by Rust + GPUI.".into(),
                "Click a tool to place gates. W connects pins; Space runs; double-click a wire probes it.".into(),
            ],
            log_view: crate::input::text_area("", window, cx),
            status: "Ready".into(),
        }
    }

    pub fn log(&mut self, message: String) {
        self.status = message.clone();
        self.push_raw(message);
    }

    pub fn push_raw(&mut self, message: String) {
        self.messages.push(message);
        if self.messages.len() > MAX_MESSAGES {
            self.messages.remove(0);
        }
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub fn sync_view<T: 'static>(&self, window: &mut Window, cx: &mut Context<T>) {
        let start = self.messages.len().saturating_sub(VISIBLE_MESSAGES);
        let text = self.messages[start..].join("\n");
        if self.log_view.read(cx).value().as_ref() != text {
            let last_row = self.messages.len().saturating_sub(start + 1) as u32;
            self.log_view.update(cx, |log, cx| {
                log.set_value(text, window, cx);
                log.set_cursor_position(
                    gpui_component::input::Position::new(last_row, 0),
                    window,
                    cx,
                );
            });
        }
    }
}
