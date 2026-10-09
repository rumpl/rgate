//! Visual sink only: counters, timing, and pixel generation belong to the circuit.
use rgate_core::Logic;

#[derive(Clone, Debug)]
pub struct VgaFrame {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u32>,
    pub frames: u64,
    pub sampled_pixels: u64,
    pub sync_errors: u64,
}
#[derive(Clone, Debug)]
pub(super) struct VgaState {
    pub frame: VgaFrame,
    previous_clock: Logic,
    previous_hsync: Logic,
    previous_vsync: Logic,
    x: usize,
    y: usize,
    line_has_pixels: bool,
    frame_has_pixels: bool,
}
impl VgaState {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            frame: VgaFrame {
                width,
                height,
                pixels: vec![0; usize::from(width) * usize::from(height)],
                frames: 0,
                sampled_pixels: 0,
                sync_errors: 0,
            },
            previous_clock: Logic::Low,
            previous_hsync: Logic::High,
            previous_vsync: Logic::High,
            x: 0,
            y: 0,
            line_has_pixels: false,
            frame_has_pixels: false,
        }
    }
    pub fn sample(
        &mut self,
        clock: Logic,
        hsync: Logic,
        vsync: Logic,
        enable: Logic,
        color: Option<u32>,
    ) {
        let edge = clock == Logic::High && self.previous_clock == Logic::Low;
        self.previous_clock = clock;
        if !edge {
            return;
        }
        if hsync == Logic::Low && self.previous_hsync == Logic::High {
            self.x = 0;
            if self.line_has_pixels {
                self.y += 1;
                self.line_has_pixels = false;
            }
        }
        if vsync == Logic::Low && self.previous_vsync == Logic::High {
            self.x = 0;
            self.y = 0;
            self.line_has_pixels = false;
            if self.frame_has_pixels {
                self.frame.frames += 1;
                self.frame_has_pixels = false;
            }
        }
        if !matches!(hsync, Logic::High | Logic::Low) || !matches!(vsync, Logic::High | Logic::Low)
        {
            self.frame.sync_errors += 1;
        }
        self.previous_hsync = hsync;
        self.previous_vsync = vsync;
        if enable == Logic::High {
            if self.x < usize::from(self.frame.width) && self.y < usize::from(self.frame.height) {
                self.frame.pixels[self.y * usize::from(self.frame.width) + self.x] =
                    color.unwrap_or(0xff00ff);
                self.frame.sampled_pixels += 1;
            } else {
                self.frame.sync_errors += 1;
            }
            self.x += 1;
            self.line_has_pixels = true;
            self.frame_has_pixels = true;
        } else if !matches!(enable, Logic::Low | Logic::High) {
            self.frame.sync_errors += 1;
        }
    }
}
impl crate::Simulator {
    pub(super) fn evaluate_vga(&mut self, index: usize) {
        let gate = &self.flattened.gates[index];
        let pclk = self.scalar_input(gate, "PCLK", Logic::Low);
        let hsync = self.scalar_input(gate, "HSYNC", Logic::High);
        let vsync = self.scalar_input(gate, "VSYNC", Logic::High);
        let enable = self.scalar_input(gate, "DE", Logic::Low);
        let color = self
            .input(gate, "R")
            .to_u64()
            .zip(self.input(gate, "G").to_u64())
            .zip(self.input(gate, "B").to_u64())
            .map(|((r, g), b)| {
                (((r as u32 & 15) * 17) << 16)
                    | (((g as u32 & 15) * 17) << 8)
                    | ((b as u32 & 15) * 17)
            });
        self.states[index]
            .vga
            .as_mut()
            .expect("VGA state")
            .sample(pclk, hsync, vsync, enable, color);
    }
    pub fn vga_frame(&self, id: rgate_core::GateId) -> Result<&VgaFrame, crate::SimError> {
        let index = self
            .flattened
            .gates
            .iter()
            .position(|gate| gate.id == id && gate.kind == rgate_core::GateKind::Vga)
            .ok_or(crate::SimError::NotInput(id))?;
        Ok(&self.states[index].vga.as_ref().unwrap().frame)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sink_samples_edges_and_resets_scan_on_sync() {
        let mut sink = VgaState::new(2, 2);
        for color in [0xff0000, 0x00ff00] {
            sink.sample(
                Logic::High,
                Logic::High,
                Logic::High,
                Logic::High,
                Some(color),
            );
            sink.sample(Logic::Low, Logic::High, Logic::High, Logic::High, None);
        }
        sink.sample(Logic::High, Logic::Low, Logic::High, Logic::Low, None);
        sink.sample(Logic::Low, Logic::Low, Logic::High, Logic::Low, None);
        sink.sample(
            Logic::High,
            Logic::High,
            Logic::High,
            Logic::High,
            Some(0x0000ff),
        );
        assert_eq!(sink.frame.pixels[..3], [0xff0000, 0x00ff00, 0x0000ff]);
        assert_eq!(sink.frame.sampled_pixels, 3);
    }
}
