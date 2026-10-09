use serde::{Deserialize, Serialize};
use std::{fmt, ops::Not};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Logic {
    Low,
    High,
    #[default]
    Unknown,
    HighZ,
}

impl Logic {
    pub const fn from_bool(value: bool) -> Self {
        if value { Self::High } else { Self::Low }
    }

    pub fn buffered(self) -> Self {
        match self {
            Self::Low => Self::Low,
            Self::High => Self::High,
            _ => Self::Unknown,
        }
    }

    pub fn and(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::Low, _) | (_, Self::Low) => Self::Low,
            (Self::High, Self::High) => Self::High,
            _ => Self::Unknown,
        }
    }

    pub fn or(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::High, _) | (_, Self::High) => Self::High,
            (Self::Low, Self::Low) => Self::Low,
            _ => Self::Unknown,
        }
    }

    pub fn xor(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::Low, Self::Low) | (Self::High, Self::High) => Self::Low,
            (Self::Low, Self::High) | (Self::High, Self::Low) => Self::High,
            _ => Self::Unknown,
        }
    }

    pub fn resolve(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::HighZ, other) | (other, Self::HighZ) => other,
            (a, b) if a == b => a,
            _ => Self::Unknown,
        }
    }
}

impl Not for Logic {
    type Output = Self;
    fn not(self) -> Self {
        match self {
            Self::Low => Self::High,
            Self::High => Self::Low,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for Logic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Low => "0",
            Self::High => "1",
            Self::Unknown => "X",
            Self::HighZ => "Z",
        })
    }
}

/// A little-endian, four-state logic vector. The supported width is 1–4096 bits.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    bits: Vec<Logic>,
}

impl Default for Signal {
    fn default() -> Self {
        Self::filled(1, Logic::Low)
    }
}

impl Signal {
    pub fn from_bits(bits: Vec<Logic>) -> Self {
        assert!(
            !bits.is_empty() && bits.len() <= 4096,
            "logic vector must contain 1–4096 bits"
        );
        Self { bits }
    }

    pub fn slice(&self, offset: u16, width: u16) -> Self {
        Self::from_bits(
            (0..width)
                .map(|bit| self.bit(usize::from(offset) + usize::from(bit)))
                .collect(),
        )
    }

    pub fn filled(width: u16, value: Logic) -> Self {
        Self {
            bits: vec![value; usize::from(width.clamp(1, 4096))],
        }
    }

    pub fn from_u64(value: u64, width: u16) -> Self {
        Self {
            bits: (0..width.clamp(1, 4096))
                .map(|bit| Logic::from_bool(bit < 64 && value & (1u64 << bit) != 0))
                .collect(),
        }
    }

    pub fn scalar(value: Logic) -> Self {
        Self { bits: vec![value] }
    }

    pub fn width(&self) -> u16 {
        self.bits.len() as u16
    }

    pub fn bit(&self, index: usize) -> Logic {
        self.bits.get(index).copied().unwrap_or(Logic::Low)
    }

    pub fn bits(&self) -> &[Logic] {
        &self.bits
    }

    pub fn broadcast_or_resize(&self, width: u16) -> Self {
        if self.width() == 1 {
            Self::filled(width, self.bit(0))
        } else {
            self.resized(width)
        }
    }

    pub fn resized(&self, width: u16) -> Self {
        Self {
            bits: (0..usize::from(width.clamp(1, 4096)))
                .map(|index| self.bit(index))
                .collect(),
        }
    }

    pub fn to_biguint(&self) -> Option<num_bigint::BigUint> {
        let mut bytes = vec![0u8; self.bits.len().div_ceil(8)];
        for (bit, value) in self.bits.iter().enumerate() {
            match value {
                Logic::High => bytes[bit / 8] |= 1 << (bit % 8),
                Logic::Low => {}
                _ => return None,
            }
        }
        Some(num_bigint::BigUint::from_bytes_le(&bytes))
    }

    pub fn from_biguint(value: &num_bigint::BigUint, width: u16) -> Self {
        let bytes = value.to_bytes_le();
        Self::from_bits(
            (0..usize::from(width.clamp(1, 4096)))
                .map(|bit| {
                    Logic::from_bool(
                        bytes
                            .get(bit / 8)
                            .is_some_and(|byte| byte & (1 << (bit % 8)) != 0),
                    )
                })
                .collect(),
        )
    }

    pub fn parse(value: &str, width: u16, radix: u32) -> Result<Self, String> {
        let number = num_bigint::BigUint::parse_bytes(value.trim().as_bytes(), radix)
            .ok_or_else(|| "invalid unsigned number".to_owned())?;
        if number.bits() > u64::from(width) {
            return Err(format!("value exceeds {width}-bit width"));
        }
        Ok(Self::from_biguint(&number, width))
    }

    pub fn to_u64(&self) -> Option<u64> {
        use num_traits::ToPrimitive;
        self.to_biguint()?.to_u64()
    }

    pub fn decimal_value(&self) -> Option<String> {
        Some(self.to_biguint()?.to_str_radix(10))
    }

    pub fn incremented(&self) -> Self {
        self.to_biguint()
            .map(|value| Self::from_biguint(&(value + 1u8), self.width()))
            .unwrap_or_else(|| Self::from_u64(0, self.width()))
    }

    pub fn add_with_carry(&self, rhs: &Self, carry: Logic, width: u16) -> (Self, Logic) {
        let mut carry = carry;
        let mut bits = Vec::with_capacity(usize::from(width));
        for bit in 0..usize::from(width) {
            let a = self.bit(bit);
            let b = rhs.bit(bit);
            bits.push(a.xor(b).xor(carry));
            carry = a.and(b).or(a.and(carry)).or(b.and(carry));
        }
        (Self::from_bits(bits), carry)
    }

    pub fn multiply(&self, rhs: &Self, width: u16) -> Self {
        self.to_biguint()
            .zip(rhs.to_biguint())
            .map(|(a, b)| Self::from_biguint(&(a * b), width))
            .unwrap_or_else(|| Self::filled(width, Logic::Unknown))
    }

    pub fn divide(&self, rhs: &Self, width: u16) -> (Self, Self) {
        use num_traits::Zero;
        if let Some((a, b)) = self.to_biguint().zip(rhs.to_biguint())
            && !b.is_zero()
        {
            return (
                Self::from_biguint(&(&a / &b), width),
                Self::from_biguint(&(a % b), width),
            );
        }
        (
            Self::filled(width, Logic::Unknown),
            Self::filled(width, Logic::Unknown),
        )
    }

    pub fn map(&self, f: impl Fn(Logic) -> Logic) -> Self {
        Self {
            bits: self.bits.iter().copied().map(f).collect(),
        }
    }

    pub fn zip(&self, rhs: &Self, f: impl Fn(Logic, Logic) -> Logic) -> Self {
        Self {
            bits: (0..usize::from(self.width().max(rhs.width())))
                .map(|index| f(self.bit(index), rhs.bit(index)))
                .collect(),
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.bits.is_empty() && self.bits.len() <= 4096
    }

    pub fn display_value(&self) -> String {
        if self.width() == 1 {
            self.bit(0).to_string()
        } else if let Some(value) = self.to_biguint() {
            let digits = value.to_str_radix(16).to_uppercase();
            format!(
                "0x{:0>width$}",
                digits,
                width = usize::from(self.width()).div_ceil(4)
            )
        } else if self.bits.iter().all(|bit| *bit == self.bit(0)) {
            self.bit(0).to_string()
        } else {
            self.bits.iter().rev().map(ToString::to_string).collect()
        }
    }
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display_value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlling_values_dominate_unknowns() {
        assert_eq!(Logic::Unknown.and(Logic::Low), Logic::Low);
        assert_eq!(Logic::HighZ.or(Logic::High), Logic::High);
        assert_eq!(Logic::High.xor(Logic::Unknown), Logic::Unknown);
        assert_eq!(!Logic::HighZ, Logic::Unknown);
    }

    #[test]
    fn driver_conflicts_resolve_to_unknown() {
        assert_eq!(Logic::HighZ.resolve(Logic::High), Logic::High);
        assert_eq!(Logic::Low.resolve(Logic::High), Logic::Unknown);
    }

    #[test]
    fn full_width_vectors_do_not_overflow() {
        let signal = Signal::from_u64(u64::MAX, 64);
        assert_eq!(signal.to_u64(), Some(u64::MAX));
        assert_eq!(signal.display_value(), "0xFFFFFFFFFFFFFFFF");
        assert_eq!(Signal::from_u64(3, 8).display_value(), "0x03");
    }
    #[test]
    fn wide_vectors_parse_arithmetic_and_display_without_truncation() {
        let max = Signal::parse("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", 128, 16).unwrap();
        assert_eq!(max.width(), 128);
        assert!(max.to_u64().is_none());
        let one = Signal::from_u64(1, 128);
        let (sum, carry) = max.add_with_carry(&one, Logic::Low, 128);
        assert_eq!(sum.to_u64(), Some(0));
        assert_eq!(carry, Logic::High);
        let value = Signal::parse("100000000000000000000000000000000", 160, 16).unwrap();
        assert_eq!(
            value
                .divide(&Signal::from_u64(2, 160), 160)
                .0
                .display_value(),
            "0x0000000080000000000000000000000000000000"
        );
        assert_eq!(
            Signal::parse("FFFFFFFFFFFFFFFF", 128, 16)
                .unwrap()
                .multiply(&Signal::from_u64(2, 128), 128)
                .display_value(),
            "0x0000000000000001FFFFFFFFFFFFFFFE"
        );
        assert!(Signal::parse("100", 8, 16).is_err());
        assert_eq!(crate::LedDisplay::Decimal.digits(128), 39);
    }
}
