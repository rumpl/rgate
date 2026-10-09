//! Viewer state does not own simulation state. Saved keys are hierarchical signal names.
use rgate_core::{Logic, Signal};
use rgate_sim::Trace;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ROW_HEIGHT: f32 = 28.0;
pub const RULER_HEIGHT: f32 = 24.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Radix {
    Binary,
    #[default]
    Hex,
    Unsigned,
    Signed,
}
impl Radix {
    pub const ALL: [Self; 4] = [Self::Binary, Self::Hex, Self::Unsigned, Self::Signed];
    pub fn label(self) -> &'static str {
        match self {
            Self::Binary => "Bin",
            Self::Hex => "Hex",
            Self::Unsigned => "Dec",
            Self::Signed => "Signed",
        }
    }
    pub fn format(self, value: &Signal) -> String {
        match self {
            Self::Binary => value.bits().iter().rev().map(ToString::to_string).collect(),
            Self::Hex => value.display_value(),
            Self::Unsigned => value
                .decimal_value()
                .unwrap_or_else(|| value.display_value()),
            Self::Signed => {
                if value.bit(usize::from(value.width()) - 1) != Logic::High {
                    return value
                        .decimal_value()
                        .unwrap_or_else(|| value.display_value());
                }
                // Two's-complement magnitude, avoiding a viewer dependency on BigInt.
                let inverse = value.map(|bit| !bit).incremented();
                inverse
                    .decimal_value()
                    .map(|number| format!("-{number}"))
                    .unwrap_or_else(|| value.display_value())
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WaveformSettings {
    pub start: f64,
    pub span: f64,
    pub follow: bool,
    pub label_width: f32,
    pub cursor_a: Option<u64>,
    pub cursor_b: Option<u64>,
    pub order: Vec<String>,
    pub radices: BTreeMap<String, Radix>,
    pub grouped: bool,
    pub collapsed: BTreeSet<String>,
    pub filter: String,
    pub selected: Option<String>,
    pub row_offset: usize,
}
impl Default for WaveformSettings {
    fn default() -> Self {
        Self {
            start: 0.0,
            span: 1000.0,
            follow: true,
            label_width: 250.0,
            cursor_a: None,
            cursor_b: None,
            order: Vec::new(),
            radices: BTreeMap::new(),
            grouped: false,
            collapsed: BTreeSet::new(),
            filter: String::new(),
            selected: None,
            row_offset: 0,
        }
    }
}
impl WaveformSettings {
    pub fn sanitize(&mut self) {
        if !self.start.is_finite() || self.start < 0.0 {
            self.start = 0.0;
        }
        if !self.span.is_finite() {
            self.span = 1000.0;
        }
        self.span = self.span.clamp(1.0, 1e15);
        if !self.label_width.is_finite() {
            self.label_width = 250.0;
        }
        self.label_width = self.label_width.clamp(100.0, 600.0);
    }
    pub fn visible_start(&self, time: u64) -> f64 {
        if self.follow {
            (time as f64 - self.span * 0.9).max(0.0)
        } else {
            self.start
        }
    }
    pub fn zoom(&mut self, factor: f64, fraction: f64, time: u64) {
        let fraction = fraction.clamp(0.0, 1.0);
        let start = self.visible_start(time);
        let anchor = start + self.span * fraction;
        self.span = (self.span * factor).clamp(1.0, 1e15);
        self.start = (anchor - self.span * fraction).max(0.0);
        self.follow = false;
    }
    pub fn pan(&mut self, delta: f64, time: u64) {
        self.start = (self.visible_start(time) + delta).max(0.0);
        self.follow = false;
    }
    pub fn fit(&mut self, traces: &[Trace], time: u64) {
        let start = traces
            .iter()
            .filter_map(|trace| trace.transitions.first().map(|change| change.time))
            .min()
            .unwrap_or(0);
        self.start = start as f64;
        self.span = (time.saturating_sub(start) as f64 * 1.05).max(20.0);
        self.follow = false;
    }
    pub fn radix(&self, name: &str) -> Radix {
        self.radices.get(name).copied().unwrap_or_default()
    }
    pub fn move_selected(&mut self, up: bool, traces: &[Trace]) {
        let rows = self.sorted(traces);
        self.order = rows.iter().map(|trace| trace.name.clone()).collect();
        if let Some(index) = self
            .selected
            .as_ref()
            .and_then(|name| self.order.iter().position(|row| row == name))
        {
            let other = if up {
                index.saturating_sub(1)
            } else {
                (index + 1).min(self.order.len().saturating_sub(1))
            };
            self.order.swap(index, other);
        }
    }
    pub fn sorted<'a>(&self, traces: &'a [Trace]) -> Vec<&'a Trace> {
        let mut result = traces
            .iter()
            .filter(|trace| {
                trace
                    .name
                    .to_lowercase()
                    .contains(&self.filter.to_lowercase())
            })
            .collect::<Vec<_>>();
        result.sort_by_key(|trace| {
            self.order
                .iter()
                .position(|name| name == &trace.name)
                .unwrap_or(usize::MAX)
        });
        result
    }
    pub fn rows(&self, traces: &[Trace]) -> Vec<WaveRow> {
        let sorted = self.sorted(traces);
        if !self.grouped {
            return sorted.into_iter().cloned().map(WaveRow::Signal).collect();
        }
        let mut groups: BTreeMap<String, Vec<Trace>> = BTreeMap::new();
        for trace in sorted {
            let parent = trace
                .name
                .rsplit_once('/')
                .map_or("Top", |(parent, _)| parent);
            groups.entry(parent.into()).or_default().push(trace.clone());
        }
        let mut rows = Vec::new();
        for (name, traces) in groups {
            rows.push(WaveRow::Group(name.clone()));
            if !self.collapsed.contains(&name) {
                rows.extend(traces.into_iter().map(WaveRow::Signal));
            }
        }
        rows
    }
    pub fn time_at(&self, x: f32, width: f32, time: u64) -> u64 {
        (self.visible_start(time) + f64::from(x / width.max(1.0)) * self.span)
            .max(0.0)
            .min(u64::MAX as f64) as u64
    }
    pub fn delta(&self) -> Option<u64> {
        Some(self.cursor_a?.abs_diff(self.cursor_b?))
    }
}
#[derive(Clone, Debug)]
pub enum WaveRow {
    Group(String),
    Signal(Trace),
}
pub fn value_at(trace: &Trace, time: u64) -> Option<&Signal> {
    let index = trace
        .transitions
        .partition_point(|change| change.time <= time);
    index
        .checked_sub(1)
        .map(|index| &trace.transitions[index].value)
}
pub fn tick_step(span: f64, width: f32) -> f64 {
    let target = span / (f64::from(width) / 90.0).max(1.0);
    let power = 10f64.powf(target.log10().floor());
    let normalized = target / power;
    let step = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    (step * power).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgate_core::NetId;
    use rgate_sim::Transition;
    #[test]
    fn zoom_keeps_anchor_pan_disables_follow_and_cursor_delta_is_exact() {
        let mut state = WaveformSettings {
            follow: false,
            start: 100.0,
            span: 1000.0,
            ..Default::default()
        };
        let anchor = state.start + state.span * 0.3;
        state.zoom(0.5, 0.3, 1000);
        assert_eq!(state.start + state.span * 0.3, anchor);
        state.pan(-10000.0, 1000);
        assert_eq!(state.start, 0.0);
        state.cursor_a = Some(15);
        state.cursor_b = Some(45);
        assert_eq!(state.delta(), Some(30));
    }
    #[test]
    fn radix_and_value_at_cover_unknowns_and_wide_signed_values() {
        let value = Signal::from_u64(255, 8);
        assert_eq!(Radix::Signed.format(&value), "-1");
        assert_eq!(Radix::Unsigned.format(&value), "255");
        assert_eq!(Radix::Binary.format(&value), "11111111");
        let trace = Trace {
            net: NetId(1),
            name: "main/A".into(),
            width: 8,
            transitions: vec![
                Transition {
                    time: 10,
                    value: value.clone(),
                },
                Transition {
                    time: 20,
                    value: Signal::from_u64(0, 8),
                },
            ],
        };
        assert!(value_at(&trace, 9).is_none());
        assert_eq!(value_at(&trace, 15), Some(&value));
    }
    #[test]
    fn ordering_grouping_and_filtering_are_stable() {
        let traces = (0..3)
            .map(|index| Trace {
                net: NetId(index),
                name: format!("main/u{}/A", index % 2),
                width: 1,
                transitions: Vec::new(),
            })
            .collect::<Vec<_>>();
        let mut state = WaveformSettings {
            selected: Some("main/u1/A".into()),
            ..Default::default()
        };
        state.move_selected(true, &traces);
        assert_eq!(state.order[0], "main/u1/A");
        state.grouped = true;
        state.collapsed.insert("main/u0".into());
        assert_eq!(state.rows(&traces).len(), 3);
    }
}
