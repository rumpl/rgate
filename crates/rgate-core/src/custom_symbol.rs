use crate::{Point, Rect};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SymbolPrimitive {
    Line {
        start: Point,
        end: Point,
    },
    Rectangle {
        start: Point,
        end: Point,
        #[serde(default)]
        filled: bool,
    },
    Ellipse {
        start: Point,
        end: Point,
        #[serde(default)]
        filled: bool,
    },
    Text {
        position: Point,
        text: String,
    },
}
impl SymbolPrimitive {
    pub fn bounds(&self) -> Rect {
        match self {
            Self::Line { start, end }
            | Self::Rectangle { start, end, .. }
            | Self::Ellipse { start, end, .. } => Rect::from_points(*start, *end),
            Self::Text { position, text } => Rect::from_points(
                *position,
                *position + Point::new(text.chars().count() as f32 * 6.0, 14.0),
            ),
        }
    }
    pub fn is_valid(&self) -> bool {
        let bounds = self.bounds();
        bounds.min.is_finite()
            && bounds.max.is_finite()
            && bounds.min.x.abs() <= 10000.0
            && bounds.min.y.abs() <= 10000.0
            && bounds.max.x.abs() <= 10000.0
            && bounds.max.y.abs() <= 10000.0
    }
}
