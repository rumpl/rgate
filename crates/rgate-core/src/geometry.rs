use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }

    pub fn snapped(self, grid: f32) -> Self {
        Self::new(
            (self.x / grid).round() * grid,
            (self.y / grid).round() * grid,
        )
    }

    /// TkGate rotations are counterclockwise in screen coordinates.
    pub fn rotated(self, quarter_turns: u8) -> Self {
        match quarter_turns % 4 {
            0 => self,
            1 => Self::new(self.y, -self.x),
            2 => Self::new(-self.x, -self.y),
            _ => Self::new(-self.y, self.x),
        }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Point {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f32> for Point {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl Div<f32> for Point {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    pub fn from_points(a: Point, b: Point) -> Self {
        Self {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    pub fn around(center: Point, width: f32, height: f32) -> Self {
        let half = Point::new(width / 2.0, height / 2.0);
        Self::from_points(center - half, center + half)
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    pub fn expanded(self, amount: f32) -> Self {
        let delta = Point::new(amount, amount);
        Self::from_points(self.min - delta, self.max + delta)
    }

    pub fn union(self, other: Self) -> Self {
        Self::from_points(
            Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        )
    }

    pub fn size(self) -> Point {
        self.max - self.min
    }

    pub fn center(self) -> Point {
        (self.min + self.max) / 2.0
    }
}

pub fn segment_distance(point: Point, a: Point, b: Point) -> (f32, Point) {
    let delta = b - a;
    let length_squared = delta.x * delta.x + delta.y * delta.y;
    let t = if length_squared > 0.0 {
        let offset = point - a;
        ((offset.x * delta.x + offset.y * delta.y) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let nearest = a + delta * t;
    (point.distance(nearest), nearest)
}

/// Adds an orthogonal elbow without introducing duplicate points.
pub fn route_to(points: &mut Vec<Point>, target: Point) {
    if let Some(&previous) = points.last() {
        if previous == target {
            return;
        }
        if previous.x != target.x && previous.y != target.y {
            points.push(Point::new(target.x, previous.y));
        }
    }
    points.push(target);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_and_inverse_agree() {
        let p = Point::new(3.0, 7.0);
        for rotation in 0..4 {
            assert_eq!(p.rotated(rotation).rotated(4 - rotation), p);
        }
    }

    #[test]
    fn distance_projects_onto_segment() {
        let (distance, nearest) =
            segment_distance(Point::new(5.0, 3.0), Point::ZERO, Point::new(10.0, 0.0));
        assert_eq!(distance, 3.0);
        assert_eq!(nearest, Point::new(5.0, 0.0));
    }
}
