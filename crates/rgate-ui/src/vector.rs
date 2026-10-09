use gpui::{Hsla, PathBuilder, Pixels, Window, px};
use rgate_core::Point;

#[derive(Clone, Debug, PartialEq)]
enum Segment {
    Move(Point),
    Line(Point),
    Curve(Point, Point),
    Close,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    segments: Vec<Segment>,
}

impl Shape {
    pub fn polyline(points: &[(f32, f32)]) -> Self {
        let mut shape = Self::default();
        for (index, &(x, y)) in points.iter().enumerate() {
            if index == 0 {
                shape.move_to(x, y);
            } else {
                shape.line_to(x, y);
            }
        }
        shape
    }

    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.segments.push(Segment::Move(Point::new(x, y)));
        self
    }

    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.segments.push(Segment::Line(Point::new(x, y)));
        self
    }

    pub fn curve_to(&mut self, x: f32, y: f32, cx: f32, cy: f32) -> &mut Self {
        self.segments
            .push(Segment::Curve(Point::new(x, y), Point::new(cx, cy)));
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.segments.push(Segment::Close);
        self
    }

    pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Self {
        let r = radius.min(w / 2.0).min(h / 2.0);
        let mut shape = Self::default();
        shape
            .move_to(x + r, y)
            .line_to(x + w - r, y)
            .curve_to(x + w, y + r, x + w, y)
            .line_to(x + w, y + h - r)
            .curve_to(x + w - r, y + h, x + w, y + h)
            .line_to(x + r, y + h)
            .curve_to(x, y + h - r, x, y + h)
            .line_to(x, y + r)
            .curve_to(x + r, y, x, y)
            .close();
        shape
    }

    pub fn circle(x: f32, y: f32, r: f32) -> Self {
        // Eight quadratic arcs keep small circles smooth without bitmap assets.
        let mut shape = Self::default();
        let step = std::f32::consts::TAU / 8.0;
        shape.move_to(x + r, y);
        for index in 0..8 {
            let end = (index + 1) as f32 * step;
            let middle = end - step / 2.0;
            let control_radius = r / (step / 2.0).cos();
            shape.curve_to(
                x + r * end.cos(),
                y + r * end.sin(),
                x + control_radius * middle.cos(),
                y + control_radius * middle.sin(),
            );
        }
        shape.close();
        shape
    }

    pub fn reflected_x(mut self, width: f32) -> Self {
        for segment in &mut self.segments {
            match segment {
                Segment::Move(p) | Segment::Line(p) => p.x = width - p.x,
                Segment::Curve(p, control) => {
                    p.x = width - p.x;
                    control.x = width - control.x;
                }
                Segment::Close => {}
            }
        }
        self
    }

    pub fn paint(
        &self,
        transform: impl Fn(Point) -> gpui::Point<Pixels>,
        width: f32,
        color: Hsla,
        background: Option<Hsla>,
        window: &mut Window,
    ) {
        for fill in [true, false] {
            let Some(paint) = (if fill { background } else { Some(color) }) else {
                continue;
            };
            let mut builder = if fill {
                PathBuilder::fill()
            } else {
                PathBuilder::stroke(px(width))
            };
            for segment in &self.segments {
                match segment {
                    Segment::Move(p) => builder.move_to(transform(*p)),
                    Segment::Line(p) => builder.line_to(transform(*p)),
                    Segment::Curve(p, control) => {
                        builder.curve_to(transform(*p), transform(*control))
                    }
                    Segment::Close => builder.close(),
                }
            }
            match builder.build() {
                Ok(path) => window.paint_path(path, paint),
                Err(error) => eprintln!("vector artwork paint failed: {error}"),
            }
        }
    }
}
