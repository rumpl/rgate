use crate::theme::Theme;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, InspectorElementId,
    KeyDownEvent, LayoutId, MouseButton, PaintQuad, Pixels, ShapedLine, Style, TextRun,
    UTF16Selection, Window, div, fill, point, prelude::*, px, relative, rgb, size,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub struct TextField {
    focus: FocusHandle,
    pub value: String,
    placeholder: String,
    multiline: bool,
    lines: Vec<ShapedLine>,
    selection: Range<usize>,
    marked: Option<Range<usize>>,
    layout: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
}

impl TextField {
    pub fn new(value: impl Into<String>, cx: &mut Context<Self>) -> Self {
        let value = value.into();
        let end = value.len();
        Self {
            focus: cx.focus_handle(),
            value,
            placeholder: String::new(),
            multiline: false,
            lines: Vec::new(),
            selection: end..end,
            marked: None,
            layout: None,
            bounds: None,
        }
    }

    pub fn multiline(mut self) -> Self {
        self.multiline = true;
        self
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn set_value(&mut self, value: String, cx: &mut Context<Self>) {
        let end = value.len();
        self.value = value;
        self.selection = end..end;
        self.marked = None;
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        let mut utf8 = 0;
        for ch in self.value.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.value[..offset].encode_utf16().count()
    }

    fn range_from_utf16(&self, range: Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn previous(&self, offset: usize) -> usize {
        self.value
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next(&self, offset: usize) -> usize {
        self.value
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.value.len())
    }

    fn replace(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let text = if self.multiline {
            text.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            text.replace(['\n', '\r'], " ")
        };
        self.value.replace_range(range.clone(), &text);
        let end = range.start + text.len();
        self.selection = end..end;
        self.marked = None;
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let command = key.modifiers.platform || key.modifiers.control;
        match (command, key.key.as_str()) {
            (true, "a") => self.selection = 0..self.value.len(),
            (true, "c" | "x") => {
                if !self.selection.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.value[self.selection.clone()].into(),
                    ));
                    if key.key == "x" {
                        self.replace(self.selection.clone(), "", cx);
                    }
                }
            }
            (true, "v") => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace(self.selection.clone(), &text, cx);
                }
            }
            (false, "backspace") => {
                let range = if self.selection.is_empty() {
                    self.previous(self.selection.start)..self.selection.end
                } else {
                    self.selection.clone()
                };
                self.replace(range, "", cx);
            }
            (false, "delete") => {
                let range = if self.selection.is_empty() {
                    self.selection.start..self.next(self.selection.end)
                } else {
                    self.selection.clone()
                };
                self.replace(range, "", cx);
            }
            (_, "left") => {
                let offset = if self.selection.is_empty() {
                    self.previous(self.selection.start)
                } else {
                    self.selection.start
                };
                self.selection = offset..offset;
            }
            (_, "right") => {
                let offset = if self.selection.is_empty() {
                    self.next(self.selection.end)
                } else {
                    self.selection.end
                };
                self.selection = offset..offset;
            }
            (false, "up" | "down") if self.multiline => {
                let offset = self.selection.end;
                let start = self.value[..offset].rfind('\n').map_or(0, |i| i + 1);
                let column = offset - start;
                let target = if key.key == "up" {
                    if start == 0 {
                        0
                    } else {
                        let previous = self.value[..start - 1].rfind('\n').map_or(0, |i| i + 1);
                        previous + column.min(start - 1 - previous)
                    }
                } else {
                    let end = self.value[offset..]
                        .find('\n')
                        .map(|i| offset + i + 1)
                        .unwrap_or(self.value.len());
                    let len = self.value[end..]
                        .find('\n')
                        .unwrap_or(self.value.len() - end);
                    end + column.min(len)
                };
                let mut target = target;
                while !self.value.is_char_boundary(target) {
                    target -= 1;
                }
                self.selection = target..target;
            }
            (_, "home") => self.selection = 0..0,
            (_, "end") => self.selection = self.value.len()..self.value.len(),
            (false, "enter") if self.multiline => self.replace(self.selection.clone(), "\n", cx),
            (false, "tab") if self.multiline => self.replace(self.selection.clone(), "    ", cx),
            (false, "enter" | "escape" | "tab") => return,
            _ => return,
        }
        cx.stop_propagation();
        window.refresh();
        cx.notify();
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(range);
        *actual = Some(self.range_to_utf16(range.clone()));
        Some(self.value[range].into())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(self.selection.clone()),
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        self.replace(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        let start = range.start;
        self.replace(range, text, cx);
        self.marked = (!text.is_empty()).then_some(start..start + text.len());
        if let Some(selection) = selection {
            let prefix = self.value[..start].encode_utf16().count();
            self.selection =
                self.range_from_utf16(prefix + selection.start..prefix + selection.end);
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(range);
        let row = if self.multiline {
            self.value[..range.start]
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
        } else {
            0
        };
        let offset = if self.multiline {
            self.value[..range.start].rfind('\n').map_or(0, |i| i + 1)
        } else {
            0
        };
        let line = if self.multiline {
            self.lines.get(row)?
        } else {
            self.layout.as_ref()?
        };
        let line_len = self.value[offset..]
            .find('\n')
            .unwrap_or(self.value.len() - offset);
        let range = (range.start - offset).min(line_len)..(range.end - offset).min(line_len);
        Some(Bounds::new(
            point(
                bounds.left() + line.x_for_index(range.start),
                bounds.top() + px(row as f32 * 24.0),
            ),
            size(
                line.x_for_index(range.end) - line.x_for_index(range.start),
                bounds.size.height,
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.bounds?;
        let index = self
            .layout
            .as_ref()?
            .closest_index_for_x(position.x - bounds.left())
            .min(self.value.len());
        Some(self.offset_to_utf16(index))
    }
}

struct FieldElement {
    field: Entity<TextField>,
}
struct FieldPaint {
    line: ShapedLine,
    cursor: PaintQuad,
    selection: Option<PaintQuad>,
}

impl IntoElement for FieldElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for FieldElement {
    type RequestLayoutState = ();
    type PrepaintState = FieldPaint;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        let field = self.field.read(cx);
        style.size.height = px(if field.multiline {
            field.value.lines().count().max(1) as f32 * 24.0 + 24.0
        } else {
            24.0
        })
        .into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> FieldPaint {
        let palette = cx
            .try_global::<Theme>()
            .copied()
            .unwrap_or_default()
            .palette();
        let field = self.field.read(cx);
        let style = window.text_style();
        let placeholder = field.value.is_empty();
        let displayed = if placeholder {
            &field.placeholder
        } else {
            &field.value
        };
        let displayed = if field.multiline {
            displayed.lines().next().unwrap_or("").to_owned()
        } else {
            displayed.clone()
        };
        let run = TextRun {
            len: displayed.len(),
            font: style.font(),
            color: rgb(if placeholder {
                palette.muted
            } else {
                palette.text
            })
            .into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line =
            window
                .text_system()
                .shape_line(displayed.clone().into(), px(13.0), &[run], None);
        let cursor_index = if field.multiline {
            field.value[..field.selection.end]
                .rsplit('\n')
                .next()
                .unwrap_or("")
                .len()
        } else {
            field.selection.end
        };
        let cursor_y = if field.multiline {
            field.value[..field.selection.end]
                .bytes()
                .filter(|b| *b == b'\n')
                .count() as f32
                * 24.0
        } else {
            0.0
        };
        let cursor_line = if field.multiline {
            let row = field.value[..field.selection.end]
                .bytes()
                .filter(|b| *b == b'\n')
                .count();
            let text = field.value.split('\n').nth(row).unwrap_or("");
            let run = TextRun {
                len: text.len(),
                font: style.font(),
                color: rgb(palette.text).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(text.to_owned().into(), px(13.0), &[run], None)
        } else {
            line.clone()
        };
        let cursor_x = bounds.left() + cursor_line.x_for_index(cursor_index);
        let cursor = fill(
            Bounds::new(
                point(cursor_x, bounds.top() + px(cursor_y + 3.0)),
                size(px(1.0), px(18.0)),
            ),
            rgb(palette.text),
        );
        let selection = (!field.multiline && !field.selection.is_empty()).then(|| {
            fill(
                Bounds::new(
                    point(
                        bounds.left() + line.x_for_index(field.selection.start),
                        bounds.top(),
                    ),
                    size(
                        line.x_for_index(field.selection.end)
                            - line.x_for_index(field.selection.start),
                        bounds.size.height,
                    ),
                ),
                rgb(palette.selection),
            )
        });
        FieldPaint {
            line,
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        state: &mut FieldPaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.field.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );
        if let Some(selection) = state.selection.take() {
            window.paint_quad(selection);
        }
        let field = self.field.read(cx);
        let multiline = field.multiline;
        let value = field.value.clone();
        let mut lines = Vec::new();
        if multiline {
            let style = window.text_style();
            let color = rgb(cx
                .try_global::<Theme>()
                .copied()
                .unwrap_or_default()
                .palette()
                .text)
            .into();
            for (index, text) in value.split('\n').enumerate() {
                let run = TextRun {
                    len: text.len(),
                    font: style.font(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line =
                    window
                        .text_system()
                        .shape_line(text.to_owned().into(), px(13.0), &[run], None);
                let _ = line.paint(
                    bounds.origin + point(px(0.0), px(index as f32 * 24.0)),
                    px(24.0),
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
                lines.push(line);
            }
        } else if let Err(error) = state.line.paint(
            bounds.origin,
            px(24.0),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        ) {
            eprintln!("text field paint failed: {error}");
        }
        if focus.is_focused(window) {
            window.paint_quad(state.cursor.clone());
        }
        self.field.update(cx, |field, _| {
            field.layout = Some(state.line.clone());
            field.lines = lines;
            field.bounds = Some(bounds);
        });
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TextField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx
            .try_global::<Theme>()
            .copied()
            .unwrap_or_default()
            .palette();
        div()
            .w_full()
            .min_h(px(if self.multiline { 300.0 } else { 28.0 }))
            .px(px(5.0))
            .bg(rgb(palette.panel))
            .rounded(px(palette.radius))
            .border_1()
            .border_color(rgb(palette.shadow))
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    if let (Some(bounds), Some(line)) = (this.bounds, this.layout.as_ref()) {
                        let index = if this.multiline {
                            let row = (f32::from(event.position.y - bounds.top()) / 24.0).max(0.0)
                                as usize;
                            let row = row.min(this.lines.len().saturating_sub(1));
                            let offset = this
                                .value
                                .split('\n')
                                .take(row)
                                .map(|s| s.len() + 1)
                                .sum::<usize>();
                            offset
                                + this.lines.get(row).map_or(0, |line| {
                                    line.closest_index_for_x(event.position.x - bounds.left())
                                })
                        } else {
                            line.closest_index_for_x(event.position.x - bounds.left())
                        }
                        .min(this.value.len());
                        this.selection = index..index;
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(FieldElement { field: cx.entity() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn multiline_paste_preserves_source_and_single_line_fields_stay_single_line(
        cx: &mut gpui::TestAppContext,
    ) {
        let field = cx.new(|cx| TextField::new("", cx).multiline());
        field.update(cx, |field, cx| {
            field.replace(0..0, "module test;\r\nendmodule\n", cx)
        });
        assert_eq!(
            field.read_with(cx, |field, _| field.value.clone()),
            "module test;\nendmodule\n"
        );
        let field = cx.new(|cx| TextField::new("", cx));
        field.update(cx, |field, cx| field.replace(0..0, "a\nb", cx));
        assert_eq!(field.read_with(cx, |field, _| field.value.clone()), "a b");
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    #[gpui::test]
    fn multiline_text_run_lengths_match_shaped_lines(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, cx| {
            TextField::new(format!("{}\n{}\nλ", "x".repeat(51), "y".repeat(486)), cx).multiline()
        });
        cx.run_until_parked();
    }
}
