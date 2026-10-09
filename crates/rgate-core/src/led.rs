use crate::{Logic, Point, Signal};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedDisplay {
    #[default]
    Bit,
    Bar,
    Hex,
    Decimal,
    SevenSegment,
}

impl LedDisplay {
    pub const ALL: [Self; 5] = [
        Self::Bit,
        Self::Bar,
        Self::SevenSegment,
        Self::Hex,
        Self::Decimal,
    ];

    pub fn recommended_width(self) -> u16 {
        match self {
            Self::Bit => 1,
            Self::SevenSegment => 7,
            Self::Bar | Self::Hex | Self::Decimal => 8,
        }
    }

    pub fn preview(self, width: u16) -> String {
        match self {
            Self::Bit => format!("{width}-bit input → on/off indicator"),
            Self::Bar => format!("{width}-bit input → {width} lights"),
            Self::Hex => format!("{width}-bit input → {} hex digits", self.digits(width)),
            Self::Decimal => format!("{width}-bit input → {} decimal digits", self.digits(width)),
            Self::SevenSegment => format!(
                "{width}-bit input → {} direct 7-segment digits",
                self.digits(width)
            ),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bit => "Bit LED",
            Self::Bar => "Bar graph",
            Self::Hex => "Hex digits",
            Self::Decimal => "Decimal digits",
            Self::SevenSegment => "Direct 7-segment",
        }
    }

    pub fn digits(self, width: u16) -> usize {
        match self {
            Self::Hex => usize::from(width).div_ceil(4),
            Self::SevenSegment => usize::from(width).div_ceil(7),
            Self::Decimal => {
                let max = Signal::filled(width, Logic::High).decimal_value().unwrap();
                max.len()
            }
            _ => 1,
        }
    }

    pub fn size(self, width: u16) -> Point {
        match self {
            Self::Bit => Point::new(14.0, 14.0),
            Self::Bar => Point::new(4.0 + 6.0 * f32::from(width), 16.0),
            _ => Point::new(4.0 + 24.0 * self.digits(width) as f32, 36.0),
        }
    }

    /// Digits are returned left-to-right. Direct segment bit order matches TkGate:
    /// top, upper-left, upper-right, middle, lower-left, lower-right, bottom.
    pub fn segments(self, value: &Signal) -> Vec<[Logic; 7]> {
        let count = self.digits(value.width());
        let decimal = value.decimal_value();
        let mut result = Vec::with_capacity(count);
        for digit in 0..count {
            let segments = match self {
                Self::SevenSegment => std::array::from_fn(|segment| value.bit(digit * 7 + segment)),
                Self::Hex => {
                    let bits = (0..4)
                        .map(|bit| value.bit(digit * 4 + bit))
                        .collect::<Vec<_>>();
                    if bits
                        .iter()
                        .any(|bit| matches!(bit, Logic::Unknown | Logic::HighZ))
                    {
                        [if bits.contains(&Logic::Unknown) {
                            Logic::Unknown
                        } else {
                            Logic::HighZ
                        }; 7]
                    } else {
                        let number = bits.iter().enumerate().fold(0, |number, (bit, logic)| {
                            number | (usize::from(*logic == Logic::High) << bit)
                        });
                        digit_segments(number)
                    }
                }
                Self::Decimal => match decimal.as_ref() {
                    Some(number) => digit_segments(
                        number
                            .as_bytes()
                            .iter()
                            .rev()
                            .nth(digit)
                            .map(|digit| usize::from(*digit - b'0'))
                            .unwrap_or(0),
                    ),
                    None => {
                        [if value.bits().contains(&Logic::Unknown) {
                            Logic::Unknown
                        } else {
                            Logic::HighZ
                        }; 7]
                    }
                },
                _ => [Logic::Low; 7],
            };
            result.push(segments);
        }
        result.reverse();
        result
    }
}

fn digit_segments(digit: usize) -> [Logic; 7] {
    let masks = [
        0x77, 0x24, 0x5d, 0x6d, 0x2e, 0x6b, 0x7b, 0x25, 0x7f, 0x2f, 0x3f, 0x7a, 0x53, 0x7c, 0x5b,
        0x1b,
    ];
    std::array::from_fn(|segment| Logic::from_bool(masks[digit] & (1 << segment) != 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_counts_cover_full_width_including_u64() {
        assert_eq!(LedDisplay::Hex.digits(8), 2);
        assert_eq!(LedDisplay::Decimal.digits(8), 3);
        assert_eq!(LedDisplay::Decimal.digits(64), 20);
        assert_eq!(LedDisplay::SevenSegment.digits(14), 2);
    }

    #[test]
    fn direct_segments_preserve_unknowns_and_bit_order() {
        let value = Signal::from_bits(vec![
            Logic::High,
            Logic::HighZ,
            Logic::Unknown,
            Logic::Low,
            Logic::Low,
            Logic::High,
            Logic::Low,
        ]);
        assert_eq!(LedDisplay::SevenSegment.segments(&value)[0], value.bits());
        assert_eq!(
            LedDisplay::Hex.segments(&Signal::from_u64(0x81, 8)),
            vec![digit_segments(8), digit_segments(1)]
        );
        assert_eq!(
            LedDisplay::Decimal.segments(&Signal::from_u64(255, 8)),
            vec![digit_segments(2), digit_segments(5), digit_segments(5)]
        );
    }
}
