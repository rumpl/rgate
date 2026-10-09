use crate::app::{Dialog, GateApp};
use gpui::{AnyElement, Bounds, Context, canvas, div, fill, point, prelude::*, px, rgb, size};

impl GateApp {
    pub fn vga_display(&self, gate: rgate_core::GateId, cx: &mut Context<Self>) -> AnyElement {
        let frame = self
            .simulation_gate(gate)
            .and_then(|id| {
                self.simulation
                    .as_ref()
                    .and_then(|sim| sim.vga_frame(id).ok())
            })
            .cloned();
        let Some(frame) = frame else {
            return div()
                .child("VGA device is not in this live simulation instance.")
                .into_any_element();
        };
        let pixel_count = frame.sampled_pixels;
        let frames = frame.frames;
        let errors = frame.sync_errors;
        let width = frame.width;
        let height = frame.height;
        div().flex().flex_col().gap(px(8.0))
            .child(format!("{width}×{height} · frames: {frames} · pixels sampled: {pixel_count} · sync/range errors: {errors}"))
            .child(div().h(px(340.0)).w_full().child(canvas(|_,_,_|{},move|bounds,_,window,_|{
                window.paint_quad(fill(bounds,rgb(0x101010)));let scale=(f32::from(bounds.size.width)/f32::from(width)).min(f32::from(bounds.size.height)/f32::from(height));
                let origin=bounds.center()-point(px(f32::from(width)*scale/2.0),px(f32::from(height)*scale/2.0));
                for (index,color) in frame.pixels.iter().enumerate(){let x=index%usize::from(width);let y=index/usize::from(width);window.paint_quad(fill(Bounds::new(origin+point(px(x as f32*scale),px(y as f32*scale)),size(px(scale+0.2),px(scale+0.2))),rgb(*color)));}
            }).size_full()))
            .child(if pixel_count==0 {"No pixels received. Release reset and run enough pixel-clock cycles."}else{"Captured digital RGB444 pixels. Unrecognized pixel data appears magenta."})
            .child(div().flex().gap(px(8.0))
                .child(div().id("vga-next-frame").cursor_pointer().child("Advance one tiny frame (1536 clocks)")
                    .on_click(cx.listener(|this,_,_,cx|this.advance_vga_frame(cx))))
                .child(div().id("vga-run").cursor_pointer().child(if self.running{"Pause"}else{"Run"})
                    .on_click(cx.listener(|this,_,window,cx|this.command(crate::commands::Command::PlayPause,window,cx)))))
            .into_any_element()
    }
    pub fn advance_vga_frame(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.dialog, Some(Dialog::Vga { .. })) {
            return;
        }
        self.running = false;
        let period = self
            .simulation
            .as_ref()
            .map_or(100, |sim| sim.clock_period());
        // Small quanta respect the simulator event budget; this is an example convenience,
        // not a controller implementation. All pixels still come from circuit signals.
        for _ in 0..1536 {
            if let Some(sim) = &mut self.simulation
                && let Err(error) = sim.advance(period)
            {
                self.log(format!("VGA advance failed: {error}"));
                break;
            }
        }
        cx.notify();
    }
}
