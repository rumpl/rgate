use crate::{
    canvas::{stroke, text},
    theme,
    waveform::{ROW_HEIGHT, RULER_HEIGHT, WaveRow, WaveformSettings, tick_step, value_at},
};
use gpui::{App, Bounds, ContentMask, Pixels, Window, fill, point, px, rgb, size};
use rgate_core::Logic;
use rgate_sim::{Trace, Transition};

pub fn paint(
    traces: &[Trace],
    time: u64,
    state: &WaveformSettings,
    palette: theme::Palette,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let painter = ScopePainter::new(traces, time, state, palette, bounds);
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        painter.paint(window, cx);
    });
}

struct ScopePainter<'a> {
    traces: &'a [Trace],
    time: u64,
    state: &'a WaveformSettings,
    palette: theme::Palette,
    bounds: Bounds<Pixels>,
    labels: f32,
    x_start: Pixels,
    width: f32,
    start: f64,
    end: f64,
}

impl<'a> ScopePainter<'a> {
    fn new(
        traces: &'a [Trace],
        time: u64,
        state: &'a WaveformSettings,
        palette: theme::Palette,
        bounds: Bounds<Pixels>,
    ) -> Self {
        let labels = state
            .label_width
            .min((f32::from(bounds.size.width) * 0.6).max(100.0));
        let x_start = bounds.left() + px(labels);
        let width = (f32::from(bounds.size.width) - labels - 8.0).max(1.0);
        let start = state.visible_start(time);
        let end = start + state.span;
        Self {
            traces,
            time,
            state,
            palette,
            bounds,
            labels,
            x_start,
            width,
            start,
            end,
        }
    }

    fn x(&self, time: f64) -> Pixels {
        self.x_start + px(((time - self.start) / self.state.span) as f32 * self.width)
    }

    fn paint(&self, window: &mut Window, cx: &mut App) {
        window.paint_quad(fill(self.bounds, rgb(self.palette.panel)));
        self.paint_ruler(window, cx);
        self.paint_rows(window, cx);
        self.paint_cursors(window, cx);
        stroke(
            [
                point(self.x_start, self.bounds.top()),
                point(self.x_start, self.bounds.bottom()),
            ],
            1.0,
            rgb(self.palette.shadow).into(),
            false,
            window,
        );
    }

    fn paint_ruler(&self, window: &mut Window, cx: &mut App) {
        let ruler_bottom = self.bounds.top() + px(RULER_HEIGHT);
        window.paint_quad(fill(
            Bounds::new(
                self.bounds.origin,
                size(self.bounds.size.width, px(RULER_HEIGHT)),
            ),
            rgb(self.palette.light),
        ));
        stroke(
            [
                point(self.bounds.left(), ruler_bottom),
                point(self.bounds.right(), ruler_bottom),
            ],
            1.0,
            rgb(self.palette.shadow).into(),
            false,
            window,
        );
        stroke(
            [
                point(self.x_start, self.bounds.top()),
                point(
                    self.x_start,
                    if self.traces.is_empty() {
                        ruler_bottom
                    } else {
                        self.bounds.bottom()
                    },
                ),
            ],
            1.0,
            rgb(self.palette.shadow).into(),
            false,
            window,
        );
        let step = tick_step(self.state.span, self.width);
        let mut at = (self.start / step).ceil() * step;
        while at <= self.end {
            stroke(
                [
                    point(self.x(at), self.bounds.top() + px(RULER_HEIGHT)),
                    point(self.x(at), self.bounds.bottom()),
                ],
                1.0,
                rgb(self.palette.grid).into(),
                false,
                window,
            );
            text(
                &format!("{at:.0}"),
                point(self.x(at) + px(3.0), self.bounds.top() + px(2.0)),
                11.0,
                rgb(self.palette.muted).into(),
                window,
                cx,
            );
            at += step;
        }
        text(
            "Signal / value at A",
            self.bounds.origin + point(px(8.0), px(2.0)),
            11.0,
            rgb(self.palette.muted).into(),
            window,
            cx,
        );
    }

    fn paint_rows(&self, window: &mut Window, cx: &mut App) {
        let rows = self.state.rows(self.traces);
        let offset = self.state.row_offset.min(rows.len().saturating_sub(1));
        if self.traces.is_empty() {
            text(
                "Double-click a wire or use ○ in Nets to add a probe.",
                self.bounds.origin + point(px(12.0), px(40.0)),
                12.0,
                rgb(self.palette.muted).into(),
                window,
                cx,
            );
        }
        for (index, row) in rows.iter().skip(offset).enumerate() {
            let top = self.bounds.top() + px(RULER_HEIGHT + index as f32 * ROW_HEIGHT);
            if top + px(ROW_HEIGHT) > self.bounds.bottom() {
                break;
            }
            stroke(
                [
                    point(self.bounds.left(), top + px(ROW_HEIGHT)),
                    point(self.bounds.right(), top + px(ROW_HEIGHT)),
                ],
                1.0,
                rgb(self.palette.light).into(),
                false,
                window,
            );
            match row {
                WaveRow::Group(name) => {
                    text(
                        &format!(
                            "{} {name}",
                            if self.state.collapsed.contains(name) {
                                "▸"
                            } else {
                                "▾"
                            }
                        ),
                        point(self.bounds.left() + px(8.0), top + px(5.0)),
                        11.0,
                        rgb(self.palette.text).into(),
                        window,
                        cx,
                    );
                }
                WaveRow::Signal(trace) => {
                    self.paint_signal_gutter(trace, top, window, cx);
                    self.paint_signal_trace(trace, top, window, cx);
                }
            }
        }
    }

    fn paint_signal_gutter(&self, trace: &Trace, top: Pixels, window: &mut Window, cx: &mut App) {
        window.with_content_mask(
            Some(ContentMask {
                bounds: Bounds::new(
                    point(self.bounds.left(), top),
                    size(px(self.labels - 4.0), px(ROW_HEIGHT)),
                ),
            }),
            |window| {
                if self.state.selected.as_deref() == Some(&trace.name) {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(self.bounds.left(), top),
                            size(px(self.labels), px(ROW_HEIGHT)),
                        ),
                        rgb(self.palette.selection),
                    ));
                }
                let value = value_at(trace, self.state.cursor_a.unwrap_or(self.time))
                    .map(|value| self.state.radix(&trace.name).format(value))
                    .unwrap_or_else(|| "—".into());
                text(
                    &trace.name,
                    point(self.bounds.left() + px(8.0), top + px(1.0)),
                    10.0,
                    rgb(self.palette.text).into(),
                    window,
                    cx,
                );
                text(
                    &value,
                    point(self.bounds.left() + px(8.0), top + px(13.0)),
                    9.0,
                    rgb(self.palette.muted).into(),
                    window,
                    cx,
                );
            },
        );
    }

    fn paint_signal_trace(&self, trace: &Trace, top: Pixels, window: &mut Window, cx: &mut App) {
        window.with_content_mask(
            Some(ContentMask {
                bounds: Bounds::new(point(self.x_start, top), size(px(self.width), px(ROW_HEIGHT))),
            }),
            |window| {
                for (index, change) in trace.transitions.iter().enumerate() {
                    let next = trace
                        .transitions
                        .get(index + 1)
                        .map_or(self.time, |change| change.time)
                        as f64;
                    if next < self.start || change.time as f64 > self.end {
                        continue;
                    }
                    let a = self.x((change.time as f64).max(self.start));
                    let b = self.x(next.min(self.end));
                    if b < a {
                        continue;
                    }
                    if trace.width == 1 {
                        let prev = index.checked_sub(1).map(|i| &trace.transitions[i]);
                        self.paint_scalar_segment(change, prev, a, b, top, window);
                    } else {
                        self.paint_bus_segment(trace, change, a, b, top, window, cx);
                    }
                }
            },
        );
    }

    fn paint_scalar_segment(
        &self,
        change: &Transition,
        previous: Option<&Transition>,
        a: Pixels,
        b: Pixels,
        top: Pixels,
        window: &mut Window,
    ) {
        let color = self.palette.signal_color(&change.value);
        let high = top + px(4.0);
        let low = top + px(23.0);
        let bit_y = |bit: Logic| match bit {
            Logic::High => high,
            Logic::Low => low,
            _ => top + px(13.0),
        };
        let y = bit_y(change.value.bit(0));
        stroke(
            [point(a, y), point(b, y)],
            1.4,
            color,
            change.value.bit(0) == Logic::HighZ,
            window,
        );
        if let Some(prev) = previous {
            let prev_y = bit_y(prev.value.bit(0));
            stroke([point(a, prev_y), point(a, y)], 1.4, color, false, window);
        }
        if change.value.bit(0) == Logic::Unknown {
            window.paint_quad(fill(
                Bounds::new(point(a, high), size(b - a, low - high)),
                gpui::rgba((self.palette.unknown << 8) | 0x1f),
            ));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_bus_segment(
        &self,
        trace: &Trace,
        change: &Transition,
        a: Pixels,
        b: Pixels,
        top: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let color = self.palette.signal_color(&change.value);
        let high = top + px(4.0);
        let low = top + px(23.0);
        stroke([point(a, high), point(b, high)], 1.2, color, false, window);
        stroke([point(a, low), point(b, low)], 1.2, color, false, window);
        stroke(
            [point(a - px(2.0), high), point(a + px(2.0), low)],
            1.2,
            color,
            false,
            window,
        );
        if b - a > px(32.0) {
            text(
                &self.state.radix(&trace.name).format(&change.value),
                point(a + px(5.0), top + px(5.0)),
                10.0,
                color,
                window,
                cx,
            );
        }
    }

    fn paint_cursors(&self, window: &mut Window, cx: &mut App) {
        for (label, cursor, color) in [
            ("A", self.state.cursor_a, self.palette.selected_wire),
            ("B", self.state.cursor_b, self.palette.gate),
        ] {
            if let Some(cursor) = cursor
                && cursor as f64 >= self.start
                && cursor as f64 <= self.end
            {
                let x = self.x(cursor as f64);
                stroke(
                    [
                        point(x, self.bounds.top() + px(RULER_HEIGHT)),
                        point(x, self.bounds.bottom()),
                    ],
                    1.3,
                    rgb(color).into(),
                    false,
                    window,
                );
                text(
                    label,
                    point(x + px(3.0), self.bounds.top() + px(10.0)),
                    10.0,
                    rgb(color).into(),
                    window,
                    cx,
                );
            }
        }
        if self.time as f64 >= self.start && self.time as f64 <= self.end {
            let x = self.x(self.time as f64);
            stroke(
                [
                    point(x, self.bounds.top() + px(RULER_HEIGHT)),
                    point(x, self.bounds.bottom()),
                ],
                1.0,
                rgb(self.palette.muted).into(),
                true,
                window,
            );
        }
    }
}
