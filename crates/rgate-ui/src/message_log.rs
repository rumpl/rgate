use crate::theme::Theme;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, FocusHandle, Focusable, FontWeight,
    MouseButton, Pixels, Render, ShapedLine, TextRun, Window, canvas, div, fill, font, point,
    prelude::*, px, rgb, size,
};
use std::ops::Range;

pub struct MessageLog {
    pub text: String,
    focus: FocusHandle,
    selection: Range<usize>,
    anchor: usize,
    dragging: bool,
    layouts: Vec<(Bounds<Pixels>, ShapedLine, usize)>,
}
impl MessageLog {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            text: String::new(),
            focus: cx.focus_handle(),
            selection: 0..0,
            anchor: 0,
            dragging: false,
            layouts: Vec::new(),
        }
    }
    pub fn set_messages(&mut self, messages: &[String]) {
        let text = messages
            .iter()
            .rev()
            .take(30)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if text != self.text {
            self.text = text;
            self.selection = 0..0;
            self.dragging = false;
        }
    }
    fn index(&self, position: gpui::Point<Pixels>) -> usize {
        for (bounds, line, offset) in &self.layouts {
            if position.y < bounds.bottom() {
                return offset + line.closest_index_for_x(position.x - bounds.left());
            }
        }
        self.text.len()
    }
    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.clone()]
    }
}
impl Focusable for MessageLog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for MessageLog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx
            .try_global::<Theme>()
            .copied()
            .unwrap_or_default()
            .palette();
        let text = self.text.clone();
        let selection = self.selection.clone();
        let entity = cx.weak_entity();
        div()
            .id("selectable-message-log")
            .debug_selector(|| "selectable-message-log".into())
            .key_context("MessageLog")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                let key = &event.keystroke;
                if key.modifiers.platform || key.modifiers.control {
                    match key.key.as_str() {
                        "c" => {
                            if !this.selection.is_empty() {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    this.selected_text().into(),
                                ));
                            }
                        }
                        "a" => this.selection = 0..this.text.len(),
                        _ => {}
                    }
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    let index = this.index(event.position);
                    if !event.modifiers.shift {
                        this.anchor = index;
                    }
                    this.selection = this.anchor.min(index)..this.anchor.max(index);
                    this.dragging = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.dragging && event.dragging() {
                    let index = this.index(event.position);
                    this.selection = this.anchor.min(index)..this.anchor.max(index);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.dragging = false;
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.dragging = false;
                }),
            )
            .child(
                canvas(
                    move |bounds, window, _| {
                        let mut offset = 0;
                        let mut lines = Vec::new();
                        for (index, line) in text.split('\n').enumerate() {
                            let mut typeface = font(if cfg!(target_family = "wasm") {
                                "Lilex"
                            } else {
                                "Menlo"
                            });
                            typeface.weight = FontWeight::NORMAL;
                            let run = TextRun {
                                len: line.len(),
                                font: typeface,
                                color: rgb(if line.starts_with("Error:") {
                                    palette.error
                                } else {
                                    palette.text
                                })
                                .into(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let shaped = window.text_system().shape_line(
                                line.to_owned().into(),
                                px(11.0),
                                &[run],
                                None,
                            );
                            lines.push((
                                Bounds::new(
                                    bounds.origin + point(px(0.0), px(index as f32 * 18.0)),
                                    size(bounds.size.width, px(18.0)),
                                ),
                                shaped,
                                offset,
                            ));
                            offset += line.len() + 1;
                        }
                        lines
                    },
                    move |_, lines, window, cx| {
                        for (bounds, line, offset) in &lines {
                            let start =
                                selection.start.max(*offset).min(offset + line.len()) - offset;
                            let end = selection.end.max(*offset).min(offset + line.len()) - offset;
                            if start < end {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        bounds.origin + point(line.x_for_index(start), px(0.0)),
                                        size(
                                            line.x_for_index(end) - line.x_for_index(start),
                                            px(18.0),
                                        ),
                                    ),
                                    rgb(palette.selection),
                                ));
                            }
                            if let Err(error) = line.paint(
                                bounds.origin,
                                px(18.0),
                                gpui::TextAlign::Left,
                                None,
                                window,
                                cx,
                            ) {
                                eprintln!("message rendering: {error}");
                            }
                        }
                        if let Err(error) = entity.update(cx, |this, _| this.layouts = lines) {
                            eprintln!("message layout: {error}");
                        }
                    },
                )
                .w_full()
                .h(px(self.text.lines().count().max(1) as f32 * 18.0)),
            )
    }
}
