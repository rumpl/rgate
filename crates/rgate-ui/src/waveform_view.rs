use crate::{
    app::GateApp,
    scope,
    waveform::{ROW_HEIGHT, RULER_HEIGHT, Radix, WaveRow},
};
use gpui::{
    AnyElement, Context, CursorStyle, MouseButton, Pixels, SharedString, Window, canvas, div,
    prelude::*, px, rgb,
};
use rgate_sim::Trace;

#[derive(Clone, Copy)]
pub enum WaveDrag {
    Cursor(bool),
    Pan { x: f32, start: f64 },
    Labels,
}
impl GateApp {
    pub fn waveform_traces(&self) -> Vec<Trace> {
        self.simulation
            .as_ref()
            .map(|sim| sim.traces().values().cloned().collect())
            .unwrap_or_default()
    }
    fn wave_time(&self) -> u64 {
        self.simulation.as_ref().map_or(0, |sim| sim.time())
    }
    fn wave_labels(&self) -> f32 {
        self.waveform
            .label_width
            .min((f32::from(self.wave_bounds.size.width) * 0.6).max(100.0))
    }
    fn wave_width(&self) -> f32 {
        (f32::from(self.wave_bounds.size.width) - self.wave_labels() - 8.0).max(1.0)
    }
    fn wave_at(&self, x: Pixels) -> u64 {
        self.waveform.time_at(
            f32::from(x - self.wave_bounds.left()) - self.wave_labels(),
            self.wave_width(),
            self.wave_time(),
        )
    }
    fn wave_button(
        &self,
        id: &'static str,
        label: String,
        active: bool,
        cx: &mut Context<Self>,
        callback: impl Fn(&mut GateApp, &mut Window, &mut Context<GateApp>) + 'static,
    ) -> AnyElement {
        div()
            .id(id)
            .debug_selector(move || id.into())
            .px(px(6.0))
            .h(px(24.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .bg(rgb(if active {
                self.theme.palette().selection
            } else {
                self.theme.palette().chrome
            }))
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                callback(this, window, cx);
                cx.notify();
            }))
            .into_any_element()
    }
    pub fn waveform_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.waveform_search.read(cx).value != self.waveform.filter {
            let filter = self.waveform.filter.clone();
            self.waveform_search
                .update(cx, |field, cx| field.set_value(filter, cx));
        }
        let traces = self.waveform_traces();
        let time = self.wave_time();
        let state = self.waveform.clone();
        let palette = self.theme.palette();
        let entity = cx.weak_entity();
        let mut toolbar = div()
            .id("wave-toolbar")
            .overflow_x_scroll()
            .flex()
            .flex_none()
            .h(px(27.0))
            .items_center()
            .gap(px(3.0))
            .child(
                self.wave_button("wave-zoom-in", "+".into(), false, cx, |this, _, _| {
                    this.waveform.zoom(0.5, 0.5, this.wave_time())
                }),
            )
            .child(
                self.wave_button("wave-zoom-out", "−".into(), false, cx, |this, _, _| {
                    this.waveform.zoom(2.0, 0.5, this.wave_time())
                }),
            )
            .child(
                self.wave_button("wave-fit", "Fit".into(), false, cx, |this, _, _| {
                    this.waveform.fit(&this.waveform_traces(), this.wave_time())
                }),
            )
            .child(self.wave_button(
                "wave-follow",
                "Follow".into(),
                self.waveform.follow,
                cx,
                |this, _, _| this.waveform.follow = !this.waveform.follow,
            ))
            .child(self.wave_button(
                "wave-group",
                "Group".into(),
                self.waveform.grouped,
                cx,
                |this, _, _| {
                    this.waveform.grouped = !this.waveform.grouped;
                    this.waveform.row_offset = 0;
                },
            ))
            .child(
                self.wave_button("wave-up", "↑".into(), false, cx, |this, _, _| {
                    this.waveform.move_selected(true, &this.waveform_traces())
                }),
            )
            .child(
                self.wave_button("wave-down", "↓".into(), false, cx, |this, _, _| {
                    this.waveform.move_selected(false, &this.waveform_traces())
                }),
            );
        for radix in Radix::ALL {
            toolbar = toolbar.child(
                div()
                    .id(SharedString::from(format!("wave-radix-{radix:?}")))
                    .px(px(4.0))
                    .cursor_pointer()
                    .child(radix.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(name) = &this.waveform.selected {
                            this.waveform.radices.insert(name.clone(), radix);
                        }
                        cx.notify();
                    })),
            );
        }
        toolbar = toolbar
            .child(self.wave_button(
                "wave-clear-cursors",
                "Clear A/B".into(),
                false,
                cx,
                |this, _, _| {
                    this.waveform.cursor_a = None;
                    this.waveform.cursor_b = None;
                },
            ))
            .child(self.wave_button(
                "wave-prev-edge",
                "◀ Edge".into(),
                false,
                cx,
                |this, _, _| this.wave_edge(false),
            ))
            .child(self.wave_button(
                "wave-next-edge",
                "Edge ▶".into(),
                false,
                cx,
                |this, _, _| this.wave_edge(true),
            ));
        toolbar = toolbar
            .child(
                self.wave_button("wave-remove", "Remove".into(), false, cx, |this, _, _| {
                    if let Some(name) = &this.waveform.selected
                        && let Some(net) = this
                            .waveform_traces()
                            .iter()
                            .find(|trace| &trace.name == name)
                            .map(|trace| trace.net)
                    {
                        if let Some(sim) = &mut this.simulation {
                            sim.unprobe(net);
                        }
                        this.probes.remove(&net);
                        this.saved_probes
                            .retain(|probe| format!("{}/{}", probe.path, probe.net) != *name);
                    }
                }),
            )
            .child(self.wave_button(
                "wave-vcd",
                "VCD…".into(),
                false,
                cx,
                |this, window, cx| this.export_waveforms(window, cx),
            ));
        let status = format!(
            "A: {}  B: {}  Δ: {} ns · {:.0}–{:.0} ns",
            self.waveform
                .cursor_a
                .map_or("—".into(), |time| time.to_string()),
            self.waveform
                .cursor_b
                .map_or("—".into(), |time| time.to_string()),
            self.waveform
                .delta()
                .map_or("—".into(), |delta| delta.to_string()),
            self.waveform.visible_start(time),
            self.waveform.visible_start(time) + self.waveform.span
        );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(toolbar)
            .child(div().flex_none().h(px(28.0)).flex().items_center().gap(px(6.0))
                .child(div().w(px(180.0)).flex_none().child(self.waveform_search.clone()))
                .child(div().text_size(px(10.0)).truncate().child("Click: A · Shift-click: B · Ctrl/⌘-scroll: zoom · Shift-scroll/middle-drag: pan")))
            .child(
                div()
                    .id("wave-canvas")
                    .debug_selector(|| "wave-canvas".into())
                    .flex_1()
                    .min_h_0()
                    .cursor(CursorStyle::Crosshair)
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                let _ = entity.update(cx, |this, _| this.wave_bounds = bounds);
                            },
                            move |bounds, _, window, cx| {
                                scope::paint(&traces, time, &state, palette, bounds, window, cx)
                            },
                        )
                        .size_full(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            let local = f32::from(event.position.x - this.wave_bounds.left());
                            if (local - this.wave_labels()).abs() < 5.0 {
                                this.wave_drag = Some(WaveDrag::Labels);
                            } else if local < this.wave_labels() {
                                let index =
                                    ((f32::from(event.position.y - this.wave_bounds.top())
                                        - RULER_HEIGHT)
                                        / ROW_HEIGHT)
                                        .floor();
                                if index >= 0.0
                                    && let Some(row) = this
                                        .waveform
                                        .rows(&this.waveform_traces())
                                        .get(this.waveform.row_offset + index as usize)
                                {
                                    match row {
                                        WaveRow::Group(name) => {
                                            if !this.waveform.collapsed.remove(name) {
                                                this.waveform.collapsed.insert(name.clone());
                                            }
                                        }
                                        WaveRow::Signal(trace) => {
                                            this.waveform.selected = Some(trace.name.clone())
                                        }
                                    }
                                }
                            } else {
                                let secondary = event.modifiers.shift;
                                let at = this.wave_at(event.position.x);
                                if secondary {
                                    this.waveform.cursor_b = Some(at);
                                } else {
                                    this.waveform.cursor_a = Some(at);
                                }
                                this.wave_drag = Some(WaveDrag::Cursor(secondary));
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.waveform.start = this.waveform.visible_start(this.wave_time());
                            this.waveform.follow = false;
                            this.wave_drag = Some(WaveDrag::Pan {
                                x: f32::from(event.position.x),
                                start: this.waveform.start,
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        match this.wave_drag {
                            Some(WaveDrag::Cursor(secondary)) if event.dragging() => {
                                let at = this.wave_at(event.position.x);
                                if secondary {
                                    this.waveform.cursor_b = Some(at);
                                } else {
                                    this.waveform.cursor_a = Some(at);
                                }
                            }
                            Some(WaveDrag::Pan { x, start }) if event.pressed_button.is_some() => {
                                this.waveform.start = (start
                                    - f64::from(f32::from(event.position.x) - x)
                                        / f64::from(this.wave_width())
                                        * this.waveform.span)
                                    .max(0.0)
                            }
                            Some(WaveDrag::Labels) if event.dragging() => {
                                this.waveform.label_width =
                                    f32::from(event.position.x - this.wave_bounds.left())
                                        .clamp(100.0, 600.0)
                            }
                            _ => {}
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.wave_drag = None;
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(|this, _, _, cx| {
                            this.wave_drag = None;
                            cx.stop_propagation();
                        }),
                    )
                    .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                        let delta = event.delta.pixel_delta(px(24.0));
                        if event.modifiers.platform || event.modifiers.control {
                            let fraction = (f32::from(event.position.x - this.wave_bounds.left())
                                - this.wave_labels())
                                / this.wave_width();
                            this.waveform.zoom(
                                (-f64::from(delta.y) / 180.0).exp(),
                                f64::from(fraction),
                                this.wave_time(),
                            );
                        } else if event.modifiers.shift || delta.x.abs() > delta.y.abs() {
                            this.waveform.pan(
                                -f64::from(if delta.x.abs() > delta.y.abs() {
                                    delta.x
                                } else {
                                    delta.y
                                }) / f64::from(this.wave_width())
                                    * this.waveform.span,
                                this.wave_time(),
                            );
                        } else {
                            let rows = this.waveform.rows(&this.waveform_traces()).len();
                            this.waveform.row_offset = (this.waveform.row_offset as i64
                                - (f32::from(delta.y) / 24.0).round() as i64)
                                .clamp(0, rows.saturating_sub(1) as i64)
                                as usize;
                        }
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .h(px(20.0))
                    .flex_none()
                    .px(px(6.0))
                    .text_size(px(10.0))
                    .truncate()
                    .child(status),
            )
            .into_any_element()
    }
    fn wave_edge(&mut self, next: bool) {
        let traces = self.waveform_traces();
        let current = self.waveform.cursor_a.unwrap_or(self.wave_time());
        if let Some(trace) = traces
            .iter()
            .find(|trace| self.waveform.selected.as_deref() == Some(trace.name.as_str()))
        {
            let edge = if next {
                trace
                    .transitions
                    .iter()
                    .find(|change| change.time > current)
            } else {
                trace
                    .transitions
                    .iter()
                    .rev()
                    .find(|change| change.time < current)
            };
            if let Some(edge) = edge {
                self.waveform.cursor_a = Some(edge.time);
                self.waveform.follow = false;
                if (edge.time as f64) < self.waveform.start
                    || (edge.time as f64) > self.waveform.start + self.waveform.span
                {
                    self.waveform.start = (edge.time as f64 - self.waveform.span * 0.5).max(0.0);
                }
            }
        }
    }

    pub fn export_waveforms(&mut self, window: &Window, cx: &mut Context<Self>) {
        let traces = self.waveform_traces();
        if traces.is_empty() {
            self.log("No waveform probes to export.".into());
            return;
        }
        let content = match rgate_sim::export_vcd(&traces, self.wave_time()) {
            Ok(content) => content,
            Err(error) => {
                self.log(format!("VCD export failed: {error}"));
                return;
            }
        };
        #[cfg(target_family = "wasm")]
        {
            let _ = (window, cx);
            if let Err(error) = crate::browser::download("rgate-waveforms.vcd", &content) {
                self.log(format!("VCD download failed: {error:?}"));
            }
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let directory =
                std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into()));
            let prompt = cx.prompt_for_new_path(&directory, Some("rgate-waveforms.vcd"));
            cx.spawn_in(window, async move |entity, cx| {
                let result = prompt.await;
                let _ = entity.update_in(cx, |this, _, cx| {
                    match result {
                        Ok(Ok(Some(path))) => {
                            match rgate_format::native::save_atomic(&path, &content) {
                                Ok(()) => this.log(format!("Exported {}", path.display())),
                                Err(error) => this.log(format!("VCD save failed: {error:#}")),
                            }
                        }
                        Ok(Ok(None)) => {}
                        other => this.log(format!("VCD dialog failed: {other:?}")),
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }
}
