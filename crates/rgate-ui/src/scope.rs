use crate::{
    canvas::{stroke, text},
    theme,
    waveform::{ROW_HEIGHT, RULER_HEIGHT, WaveRow, WaveformSettings, tick_step, value_at},
};
use gpui::{App, Bounds, ContentMask, Pixels, Window, fill, point, px, rgb, size};
use rgate_core::Logic;
use rgate_sim::Trace;

pub fn paint(
    traces: &[Trace],
    time: u64,
    state: &WaveformSettings,
    palette: theme::Palette,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_quad(fill(bounds, rgb(palette.panel)));
        let labels = state
            .label_width
            .min((f32::from(bounds.size.width) * 0.6).max(100.0));
        let x_start = bounds.left() + px(labels);
        let width = (f32::from(bounds.size.width) - labels - 8.0).max(1.0);
        let start = state.visible_start(time);
        let end = start + state.span;
        let x = |time: f64| x_start + px(((time - start) / state.span) as f32 * width);
        let step = tick_step(state.span, width);
        let mut at = (start / step).ceil() * step;
        while at <= end {
            stroke(
                [
                    point(x(at), bounds.top() + px(RULER_HEIGHT)),
                    point(x(at), bounds.bottom()),
                ],
                1.0,
                rgb(palette.grid).into(),
                false,
                window,
            );
            text(
                &format!("{at:.0}"),
                point(x(at) + px(3.0), bounds.top() + px(2.0)),
                10.0,
                rgb(palette.muted).into(),
                window,
                cx,
            );
            at += step;
        }
        text(
            "Signal / value at A",
            bounds.origin + point(px(8.0), px(2.0)),
            10.0,
            rgb(palette.muted).into(),
            window,
            cx,
        );
        let rows = state.rows(traces);
        let offset = state.row_offset.min(rows.len().saturating_sub(1));
        if traces.is_empty() {
            text(
                "Double-click a wire or use ○ in Nets to add a probe.",
                bounds.origin + point(px(12.0), px(40.0)),
                12.0,
                rgb(palette.muted).into(),
                window,
                cx,
            );
        }
        for (index, row) in rows.iter().skip(offset).enumerate() {
            let top = bounds.top() + px(RULER_HEIGHT + index as f32 * ROW_HEIGHT);
            if top + px(ROW_HEIGHT) > bounds.bottom() {
                break;
            }
            stroke(
                [
                    point(bounds.left(), top + px(ROW_HEIGHT)),
                    point(bounds.right(), top + px(ROW_HEIGHT)),
                ],
                1.0,
                rgb(palette.light).into(),
                false,
                window,
            );
            let WaveRow::Signal(trace) = row else {
                if let WaveRow::Group(name) = row {
                    text(
                        &format!(
                            "{} {name}",
                            if state.collapsed.contains(name) {
                                "▸"
                            } else {
                                "▾"
                            }
                        ),
                        point(bounds.left() + px(8.0), top + px(5.0)),
                        11.0,
                        rgb(palette.text).into(),
                        window,
                        cx,
                    );
                }
                continue;
            };
            window.with_content_mask(
                Some(ContentMask {
                    bounds: Bounds::new(
                        point(bounds.left(), top),
                        size(px(labels - 4.0), px(ROW_HEIGHT)),
                    ),
                }),
                |window| {
                    if state.selected.as_deref() == Some(&trace.name) {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(bounds.left(), top),
                                size(px(labels), px(ROW_HEIGHT)),
                            ),
                            rgb(palette.selection),
                        ));
                    }
                    let value = value_at(trace, state.cursor_a.unwrap_or(time));
                    let value = value
                        .map(|value| state.radix(&trace.name).format(value))
                        .unwrap_or_else(|| "—".into());
                    text(
                        &trace.name,
                        point(bounds.left() + px(8.0), top + px(1.0)),
                        10.0,
                        rgb(palette.text).into(),
                        window,
                        cx,
                    );
                    text(
                        &value,
                        point(bounds.left() + px(8.0), top + px(13.0)),
                        9.0,
                        rgb(palette.muted).into(),
                        window,
                        cx,
                    );
                },
            );
            window.with_content_mask(
                Some(ContentMask {
                    bounds: Bounds::new(point(x_start, top), size(px(width), px(ROW_HEIGHT))),
                }),
                |window| {
                    for (index, change) in trace.transitions.iter().enumerate() {
                        let next = trace
                            .transitions
                            .get(index + 1)
                            .map_or(time, |change| change.time)
                            as f64;
                        if next < start || change.time as f64 > end {
                            continue;
                        }
                        let a = x((change.time as f64).max(start));
                        let b = x(next.min(end));
                        if b < a {
                            continue;
                        }
                        let color = palette.signal_color(&change.value);
                        let high = top + px(4.0);
                        let low = top + px(23.0);
                        if trace.width == 1 {
                            let y = match change.value.bit(0) {
                                Logic::High => high,
                                Logic::Low => low,
                                _ => top + px(13.0),
                            };
                            stroke(
                                [point(a, y), point(b, y)],
                                1.4,
                                color,
                                change.value.bit(0) == Logic::HighZ,
                                window,
                            );
                            if index > 0 {
                                let previous = &trace.transitions[index - 1].value;
                                let previous_y = match previous.bit(0) {
                                    Logic::High => high,
                                    Logic::Low => low,
                                    _ => top + px(13.0),
                                };
                                stroke(
                                    [point(a, previous_y), point(a, y)],
                                    1.4,
                                    color,
                                    false,
                                    window,
                                );
                            }
                            if change.value.bit(0) == Logic::Unknown {
                                window.paint_quad(fill(
                                    Bounds::new(point(a, high), size(b - a, low - high)),
                                    gpui::rgba((palette.unknown << 8) | 0x1f),
                                ));
                            }
                        } else {
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
                                    &state.radix(&trace.name).format(&change.value),
                                    point(a + px(5.0), top + px(5.0)),
                                    10.0,
                                    color,
                                    window,
                                    cx,
                                );
                            }
                        }
                    }
                },
            );
        }
        for (label, cursor, color) in [
            ("A", state.cursor_a, palette.selected_wire),
            ("B", state.cursor_b, palette.gate),
        ] {
            if let Some(cursor) = cursor
                && cursor as f64 >= start
                && cursor as f64 <= end
            {
                stroke(
                    [
                        point(x(cursor as f64), bounds.top() + px(RULER_HEIGHT)),
                        point(x(cursor as f64), bounds.bottom()),
                    ],
                    1.3,
                    rgb(color).into(),
                    false,
                    window,
                );
                text(
                    label,
                    point(x(cursor as f64) + px(3.0), bounds.top() + px(10.0)),
                    10.0,
                    rgb(color).into(),
                    window,
                    cx,
                );
            }
        }
        if time as f64 >= start && time as f64 <= end {
            stroke(
                [
                    point(x(time as f64), bounds.top() + px(RULER_HEIGHT)),
                    point(x(time as f64), bounds.bottom()),
                ],
                1.0,
                rgb(palette.muted).into(),
                true,
                window,
            );
        }
        stroke(
            [
                point(x_start, bounds.top()),
                point(x_start, bounds.bottom()),
            ],
            1.0,
            rgb(palette.shadow).into(),
            false,
            window,
        );
    });
}
