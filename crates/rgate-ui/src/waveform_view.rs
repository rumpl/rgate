use crate::{
    app::GateApp,
    scope,
    waveform::{ROW_HEIGHT, RULER_HEIGHT, Radix, WaveRow, WaveformSettings},
};
use gpui::{
    AnyElement, Bounds, Context, CursorStyle, Entity, MouseButton, Pixels, SharedString, Window,
    canvas, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    separator::Separator,
};
use rgate_sim::Trace;

#[derive(Clone, Copy)]
pub enum WaveDrag {
    Cursor(bool),
    Pan { x: f32, start: f64 },
    Labels,
}

/// Owns waveform viewer settings, signal filter input, plot bounds, and active drag state.
pub struct WaveformController {
    pub settings: WaveformSettings,
    pub search: Entity<InputState>,
    pub bounds: Bounds<Pixels>,
    pub drag: Option<WaveDrag>,
}

impl WaveformController {
    pub fn new(window: &mut Window, cx: &mut Context<GateApp>) -> Self {
        let search = crate::input::search_field("Filter signals…", window, cx);
        cx.subscribe(&search, |this, field, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.waveform.settings.filter = field.read(cx).value().to_string();
                this.waveform.settings.row_offset = 0;
                cx.notify();
            }
        })
        .detach();
        Self {
            settings: WaveformSettings::default(),
            search,
            bounds: Bounds::default(),
            drag: None,
        }
    }

    pub fn labels_width(&self) -> f32 {
        self.settings
            .label_width
            .min((f32::from(self.bounds.size.width) * 0.6).max(100.0))
    }

    pub fn plot_width(&self) -> f32 {
        (f32::from(self.bounds.size.width) - self.labels_width() - 8.0).max(1.0)
    }

    pub fn time_at(&self, x: Pixels, sim_time: u64) -> u64 {
        self.settings.time_at(
            f32::from(x - self.bounds.left()) - self.labels_width(),
            self.plot_width(),
            sim_time,
        )
    }

    pub fn jump_edge(&mut self, traces: &[Trace], sim_time: u64, next: bool) {
        let current = self.settings.cursor_a.unwrap_or(sim_time);
        if let Some(trace) = traces
            .iter()
            .find(|trace| self.settings.selected.as_deref() == Some(trace.name.as_str()))
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
                self.settings.cursor_a = Some(edge.time);
                self.settings.follow = false;
                if (edge.time as f64) < self.settings.start
                    || (edge.time as f64) > self.settings.start + self.settings.span
                {
                    self.settings.start = (edge.time as f64 - self.settings.span * 0.5).max(0.0);
                }
            }
        }
    }

    pub fn handle_left_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        traces: &[Trace],
        sim_time: u64,
    ) {
        let local = f32::from(event.position.x - self.bounds.left());
        if (local - self.labels_width()).abs() < 5.0 {
            self.drag = Some(WaveDrag::Labels);
        } else if local < self.labels_width() {
            let index =
                ((f32::from(event.position.y - self.bounds.top()) - RULER_HEIGHT) / ROW_HEIGHT)
                    .floor();
            if index >= 0.0
                && let Some(row) = self
                    .settings
                    .rows(traces)
                    .get(self.settings.row_offset + index as usize)
            {
                match row {
                    WaveRow::Group(name) => {
                        if !self.settings.collapsed.remove(name) {
                            self.settings.collapsed.insert(name.clone());
                        }
                    }
                    WaveRow::Signal(trace) => self.settings.selected = Some(trace.name.clone()),
                }
            }
        } else {
            let secondary = event.modifiers.shift;
            let at = self.time_at(event.position.x, sim_time);
            if secondary {
                self.settings.cursor_b = Some(at);
            } else {
                self.settings.cursor_a = Some(at);
            }
            self.drag = Some(WaveDrag::Cursor(secondary));
        }
    }

    pub fn handle_middle_down(&mut self, event: &gpui::MouseDownEvent, sim_time: u64) {
        self.settings.start = self.settings.visible_start(sim_time);
        self.settings.follow = false;
        self.drag = Some(WaveDrag::Pan {
            x: f32::from(event.position.x),
            start: self.settings.start,
        });
    }

    pub fn handle_move(&mut self, event: &gpui::MouseMoveEvent, sim_time: u64) {
        match self.drag {
            Some(WaveDrag::Cursor(secondary)) if event.dragging() => {
                let at = self.time_at(event.position.x, sim_time);
                if secondary {
                    self.settings.cursor_b = Some(at);
                } else {
                    self.settings.cursor_a = Some(at);
                }
            }
            Some(WaveDrag::Pan { x, start }) if event.pressed_button.is_some() => {
                self.settings.start = (start
                    - f64::from(f32::from(event.position.x) - x) / f64::from(self.plot_width())
                        * self.settings.span)
                    .max(0.0)
            }
            Some(WaveDrag::Labels) if event.dragging() => {
                self.settings.label_width =
                    f32::from(event.position.x - self.bounds.left()).clamp(100.0, 600.0)
            }
            _ => {}
        }
    }

    pub fn handle_scroll(&mut self, event: &gpui::ScrollEvent, traces: &[Trace], sim_time: u64) {
        let delta = event.delta.pixel_delta(px(24.0));
        if event.modifiers.platform || event.modifiers.control {
            let fraction =
                (f32::from(event.position.x - self.bounds.left()) - self.labels_width())
                    / self.plot_width();
            self.settings.zoom(
                (-f64::from(delta.y) / 180.0).exp(),
                f64::from(fraction),
                sim_time,
            );
        } else if event.modifiers.shift || delta.x.abs() > delta.y.abs() {
            self.settings.pan(
                -f64::from(if delta.x.abs() > delta.y.abs() {
                    delta.x
                } else {
                    delta.y
                }) / f64::from(self.plot_width())
                    * self.settings.span,
                sim_time,
            );
        } else {
            let rows = self.settings.rows(traces).len();
            self.settings.row_offset = (self.settings.row_offset as i64
                - (f32::from(delta.y) / 24.0).round() as i64)
                .clamp(0, rows.saturating_sub(1) as i64)
                as usize;
        }
    }
}

impl GateApp {
    fn wave_button(
        &self,
        id: &'static str,
        label: String,
        active: bool,
        cx: &mut Context<Self>,
        callback: impl Fn(&mut GateApp, &mut Window, &mut Context<GateApp>) + 'static,
    ) -> Button {
        Button::new(id)
            .ghost()
            .xsmall()
            .label(label)
            .selected(active)
            .on_click(cx.listener(move |this, _, window, cx| {
                callback(this, window, cx);
                cx.notify();
            }))
    }

    pub fn waveform_panel(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.waveform.search.read(cx).value().as_ref() != self.waveform.settings.filter {
            let filter = self.waveform.settings.filter.clone();
            self.waveform
                .search
                .update(cx, |field, cx| field.set_value(filter, window, cx));
        }
        let traces = self.sim.traces();
        let time = self.sim.time();
        let state = self.waveform.settings.clone();
        let palette = self.palette();
        let entity = cx.weak_entity();
        let selected_radix = self
            .waveform
            .settings
            .selected
            .as_ref()
            .map(|name| self.waveform.settings.radix(name));
        let mut toolbar = h_flex()
            .id("wave-toolbar")
            .overflow_x_scroll()
            .flex_none()
            .h(px(30.0))
            .px_1()
            .gap_0p5()
            .child(
                self.wave_button("wave-zoom-in", "+".into(), false, cx, |this, _, _| {
                    this.waveform.settings.zoom(0.5, 0.5, this.sim.time())
                }),
            )
            .child(
                self.wave_button("wave-zoom-out", "−".into(), false, cx, |this, _, _| {
                    this.waveform.settings.zoom(2.0, 0.5, this.sim.time())
                }),
            )
            .child(
                self.wave_button("wave-fit", "Fit".into(), false, cx, |this, _, _| {
                    this.waveform
                        .settings
                        .fit(&this.sim.traces(), this.sim.time())
                }),
            )
            .child(self.wave_button(
                "wave-follow",
                "Follow".into(),
                self.waveform.settings.follow,
                cx,
                |this, _, _| this.waveform.settings.follow = !this.waveform.settings.follow,
            ))
            .child(self.wave_button(
                "wave-group",
                "Group".into(),
                self.waveform.settings.grouped,
                cx,
                |this, _, _| {
                    this.waveform.settings.grouped = !this.waveform.settings.grouped;
                    this.waveform.settings.row_offset = 0;
                },
            ))
            .child(
                self.wave_button("wave-up", "↑".into(), false, cx, |this, _, _| {
                    this.waveform
                        .settings
                        .move_selected(true, &this.sim.traces())
                }),
            )
            .child(
                self.wave_button("wave-down", "↓".into(), false, cx, |this, _, _| {
                    this.waveform
                        .settings
                        .move_selected(false, &this.sim.traces())
                }),
            );
        toolbar = toolbar.child(Separator::vertical().h(px(14.0)).mx_1());
        for radix in Radix::ALL {
            toolbar = toolbar.child(
                Button::new(SharedString::from(format!("wave-radix-{radix:?}")))
                    .ghost()
                    .xsmall()
                    .label(radix.label())
                    .selected(selected_radix == Some(radix))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(name) = &this.waveform.settings.selected {
                            this.waveform.settings.radices.insert(name.clone(), radix);
                        }
                        cx.notify();
                    })),
            );
        }
        toolbar = toolbar
            .child(Separator::vertical().h(px(14.0)).mx_1())
            .child(self.wave_button(
                "wave-clear-cursors",
                "Clear A/B".into(),
                false,
                cx,
                |this, _, _| {
                    this.waveform.settings.cursor_a = None;
                    this.waveform.settings.cursor_b = None;
                },
            ))
            .child(self.wave_button(
                "wave-prev-edge",
                "◀ Edge".into(),
                false,
                cx,
                |this, _, _| {
                    let traces = this.sim.traces();
                    this.waveform.jump_edge(&traces, this.sim.time(), false);
                },
            ))
            .child(self.wave_button(
                "wave-next-edge",
                "Edge ▶".into(),
                false,
                cx,
                |this, _, _| {
                    let traces = this.sim.traces();
                    this.waveform.jump_edge(&traces, this.sim.time(), true);
                },
            ))
            .child(
                self.wave_button("wave-remove", "Remove".into(), false, cx, |this, _, _| {
                    if let Some(name) = &this.waveform.settings.selected
                        && let Some(net) = this
                            .sim
                            .traces()
                            .iter()
                            .find(|trace| &trace.name == name)
                            .map(|trace| trace.net)
                    {
                        if let Some(sim) = &mut this.sim.live {
                            sim.unprobe(net);
                        }
                        this.sim.probes.remove(&net);
                        this.sim
                            .saved_probes
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
                .settings
                .cursor_a
                .map_or("—".into(), |time| time.to_string()),
            self.waveform
                .settings
                .cursor_b
                .map_or("—".into(), |time| time.to_string()),
            self.waveform
                .settings
                .delta()
                .map_or("—".into(), |delta| delta.to_string()),
            self.waveform.settings.visible_start(time),
            self.waveform.settings.visible_start(time) + self.waveform.settings.span
        );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(toolbar)
            .child(
                h_flex()
                    .flex_none()
                    .px_1()
                    .pb_1()
                    .gap_2()
                    .child(
                        div()
                            .w(px(180.0))
                            .flex_none()
                            .child(Input::new(&self.waveform.search).xsmall().cleanable(true)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child("Click: A · Shift-click: B · Ctrl/⌘-scroll: zoom · Shift-scroll/middle-drag: pan"),
                    ),
            )
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
                                let _ = entity.update(cx, |this, _| this.waveform.bounds = bounds);
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
                            let traces = this.sim.traces();
                            this.waveform
                                .handle_left_down(event, &traces, this.sim.time());
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.waveform.handle_middle_down(event, this.sim.time());
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        this.waveform.handle_move(event, this.sim.time());
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.waveform.drag = None;
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(|this, _, _, cx| {
                            this.waveform.drag = None;
                            cx.stop_propagation();
                        }),
                    )
                    .on_scroll(cx.listener(|this, event: &gpui::ScrollEvent, _, cx| {
                        let traces = this.sim.traces();
                        this.waveform
                            .handle_scroll(event, &traces, this.sim.time());
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .h(px(20.0))
                    .flex_none()
                    .px_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(status),
            )
            .into_any_element()
    }

    pub fn export_waveforms(&mut self, window: &Window, cx: &mut Context<Self>) {
        let traces = self.sim.traces();
        if traces.is_empty() {
            self.log("No waveform probes to export.".into());
            return;
        }
        let content = match rgate_sim::export_vcd(&traces, self.sim.time()) {
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
