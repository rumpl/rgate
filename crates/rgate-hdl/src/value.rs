//! Four-state arbitrary-width Verilog logic vectors (dual aval/bval representation).
//!
//! Bit encoding (IEEE 1364 / VPI convention):
//! - `0` (Low):     aval = 0, bval = 0
//! - `1` (High):    aval = 1, bval = 0
//! - `Z` (HighZ):   aval = 0, bval = 1
//! - `X` (Unknown): aval = 1, bval = 1

use rgate_core::{Logic, Signal};
use std::fmt;

#[derive(Clone, Debug, Eq)]
pub struct BitVec {
    width: usize,
    aval: Vec<u64>,
    bval: Vec<u64>,
    pub signed: bool,
}

impl PartialEq for BitVec {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.aval == other.aval && self.bval == other.bval
    }
}

#[inline]
fn word_count(width: usize) -> usize {
    width.div_ceil(64).max(1)
}

#[inline]
fn top_mask(width: usize) -> u64 {
    let rem = width % 64;
    if rem == 0 {
        u64::MAX
    } else {
        (1u64 << rem) - 1
    }
}

impl BitVec {
    pub fn zeros(width: usize) -> Self {
        let width = width.max(1);
        let words = word_count(width);
        Self {
            width,
            aval: vec![0; words],
            bval: vec![0; words],
            signed: false,
        }
    }

    pub fn filled(width: usize, bit: Logic) -> Self {
        let width = width.max(1);
        let words = word_count(width);
        let (a, b) = match bit {
            Logic::Low => (0u64, 0u64),
            Logic::High => (u64::MAX, 0u64),
            Logic::HighZ => (0u64, u64::MAX),
            Logic::Unknown => (u64::MAX, u64::MAX),
        };
        let mut out = Self {
            width,
            aval: vec![a; words],
            bval: vec![b; words],
            signed: false,
        };
        out.mask_top();
        out
    }

    pub fn from_u64(value: u64, width: usize) -> Self {
        let mut out = Self::zeros(width);
        out.aval[0] = value;
        out.mask_top();
        out
    }

    pub fn from_i64(value: i64, width: usize) -> Self {
        let width = width.max(1);
        let words = word_count(width);
        let fill = if value < 0 { u64::MAX } else { 0 };
        let mut aval = vec![fill; words];
        aval[0] = value as u64;
        let mut out = Self {
            width,
            aval,
            bval: vec![0; words],
            signed: true,
        };
        out.mask_top();
        out
    }

    pub fn from_bool(value: bool) -> Self {
        Self::from_u64(u64::from(value), 1)
    }

    pub fn from_logic(bit: Logic) -> Self {
        Self::filled(1, bit)
    }

    pub fn from_signal(signal: &Signal) -> Self {
        let width = usize::from(signal.width());
        let mut out = Self::zeros(width);
        for (i, &bit) in signal.bits().iter().enumerate() {
            out.set_bit(i, bit);
        }
        out
    }

    pub fn to_signal(&self) -> Signal {
        let width = self.width.min(4096);
        let bits = (0..width).map(|i| self.bit(i)).collect();
        Signal::from_bits(bits)
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    fn mask_top(&mut self) {
        let mask = top_mask(self.width);
        if let Some(last) = self.aval.last_mut() {
            *last &= mask;
        }
        if let Some(last) = self.bval.last_mut() {
            *last &= mask;
        }
    }

    #[inline]
    pub fn bit(&self, index: usize) -> Logic {
        if index >= self.width {
            return Logic::Unknown;
        }
        let w = index / 64;
        let b = index % 64;
        let a = (self.aval[w] >> b) & 1 != 0;
        let bv = (self.bval[w] >> b) & 1 != 0;
        match (a, bv) {
            (false, false) => Logic::Low,
            (true, false) => Logic::High,
            (false, true) => Logic::HighZ,
            (true, true) => Logic::Unknown,
        }
    }

    #[inline]
    pub fn set_bit(&mut self, index: usize, value: Logic) {
        if index >= self.width {
            return;
        }
        let w = index / 64;
        let mask = 1u64 << (index % 64);
        match value {
            Logic::Low => {
                self.aval[w] &= !mask;
                self.bval[w] &= !mask;
            }
            Logic::High => {
                self.aval[w] |= mask;
                self.bval[w] &= !mask;
            }
            Logic::HighZ => {
                self.aval[w] &= !mask;
                self.bval[w] |= mask;
            }
            Logic::Unknown => {
                self.aval[w] |= mask;
                self.bval[w] |= mask;
            }
        }
    }

    #[inline]
    pub fn has_xz(&self) -> bool {
        self.bval.iter().any(|&w| w != 0)
    }

    pub fn is_all_zero(&self) -> bool {
        !self.has_xz() && self.aval.iter().all(|&w| w == 0)
    }

    /// Verilog truthiness: true iff at least one bit is `1` (known high).
    /// Note: in Verilog `if (expr)`, `1'bx` and `1'bz` are falsy!
    pub fn is_true(&self) -> bool {
        self.aval
            .iter()
            .zip(self.bval.iter())
            .any(|(&a, &b)| (a & !b) != 0)
    }

    /// 4-state reduction OR (used for logical operators `&&`, `||`, `!`, ternary condition).
    pub fn to_bool_logic(&self) -> Logic {
        if self.is_true() {
            Logic::High
        } else if self.has_xz() {
            Logic::Unknown
        } else {
            Logic::Low
        }
    }

    pub fn to_u64(&self) -> Option<u64> {
        if self.has_xz() {
            return None;
        }
        if self.aval.iter().skip(1).any(|&w| w != 0) {
            return None;
        }
        Some(self.aval[0])
    }

    pub fn to_u128(&self) -> Option<u128> {
        if self.has_xz() {
            return None;
        }
        if self.aval.iter().skip(2).any(|&w| w != 0) {
            return None;
        }
        let lo = u128::from(self.aval[0]);
        let hi = self.aval.get(1).copied().map_or(0, u128::from);
        Some(lo | (hi << 64))
    }

    pub fn to_i64_signed(&self) -> Option<i64> {
        if self.has_xz() {
            return None;
        }
        let raw = self.aval[0];
        if self.width >= 64 {
            Some(raw as i64)
        } else {
            let sign_bit = 1u64 << (self.width - 1);
            if raw & sign_bit != 0 {
                let ext = !((1u64 << self.width) - 1);
                Some((raw | ext) as i64)
            } else {
                Some(raw as i64)
            }
        }
    }

    /// Resize to `new_width`, zero-extending or sign-extending (if `signed` is true).
    pub fn resized(&self, new_width: usize, signed: bool) -> Self {
        let new_width = new_width.max(1);
        if new_width == self.width {
            let mut out = self.clone();
            out.signed = signed;
            return out;
        }
        let fill = if signed {
            self.bit(self.width - 1)
        } else {
            Logic::Low
        };
        let mut out = Self::filled(new_width, fill);
        out.signed = signed;
        let copy_bits = self.width.min(new_width);
        let full_words = copy_bits / 64;
        for w in 0..full_words {
            out.aval[w] = self.aval[w];
            out.bval[w] = self.bval[w];
        }
        let rem = copy_bits % 64;
        if rem > 0 {
            let mask = (1u64 << rem) - 1;
            out.aval[full_words] = (out.aval[full_words] & !mask) | (self.aval[full_words] & mask);
            out.bval[full_words] = (out.bval[full_words] & !mask) | (self.bval[full_words] & mask);
        }
        out.mask_top();
        out
    }

    pub fn slice(&self, offset: usize, width: usize) -> Self {
        let width = width.max(1);
        let mut out = Self::filled(width, Logic::Unknown);
        for i in 0..width {
            out.set_bit(i, self.bit(offset + i));
        }
        out
    }

    pub fn set_slice(&mut self, offset: usize, value: &BitVec) {
        for i in 0..value.width {
            let idx = offset + i;
            if idx < self.width {
                self.set_bit(idx, value.bit(i));
            }
        }
    }

    /// Concatenate parts in MSB-first order (Verilog `{a, b, c}`).
    pub fn concat(parts: &[BitVec]) -> Self {
        let total: usize = parts.iter().map(|p| p.width).sum();
        if total == 0 {
            return Self::zeros(1);
        }
        let mut out = Self::zeros(total);
        let mut bit_pos = 0;
        for part in parts.iter().rev() {
            for i in 0..part.width {
                out.set_bit(bit_pos, part.bit(i));
                bit_pos += 1;
            }
        }
        out
    }

    /// Replicate `self` `count` times (Verilog `{count{self}}`).
    pub fn replicate(&self, count: usize) -> Self {
        if count == 0 {
            return Self::zeros(1);
        }
        let total = self.width * count;
        let mut out = Self::zeros(total);
        let mut bit_pos = 0;
        for _ in 0..count {
            for i in 0..self.width {
                out.set_bit(bit_pos, self.bit(i));
                bit_pos += 1;
            }
        }
        out
    }

    /// Bitwise NOT (`~a`).
    pub fn bit_not(&self) -> Self {
        let mut out = Self {
            width: self.width,
            aval: self
                .aval
                .iter()
                .zip(self.bval.iter())
                .map(|(&a, &b)| (!a) | b)
                .collect(),
            bval: self.bval.clone(),
            signed: self.signed,
        };
        out.mask_top();
        out
    }

    /// Bitwise AND (`a & b`). Expects equal widths (or resizes to max).
    pub fn bit_and(&self, rhs: &Self) -> Self {
        let w = self.width.max(rhs.width);
        let signed = self.signed && rhs.signed;
        let a = self.resized(w, signed);
        let b = rhs.resized(w, signed);
        let mut aval = Vec::with_capacity(a.aval.len());
        let mut bval = Vec::with_capacity(a.bval.len());
        for i in 0..a.aval.len() {
            let (a1, b1) = (a.aval[i], a.bval[i]);
            let (a2, b2) = (b.aval[i], b.bval[i]);
            let res0 = (!a1 & !b1) | (!a2 & !b2);
            let res1 = (a1 & !b1) & (a2 & !b2);
            aval.push(!res0);
            bval.push(!res0 & !res1);
        }
        let mut out = Self {
            width: w,
            aval,
            bval,
            signed,
        };
        out.mask_top();
        out
    }

    /// Bitwise OR (`a | b`).
    pub fn bit_or(&self, rhs: &Self) -> Self {
        let w = self.width.max(rhs.width);
        let signed = self.signed && rhs.signed;
        let a = self.resized(w, signed);
        let b = rhs.resized(w, signed);
        let mut aval = Vec::with_capacity(a.aval.len());
        let mut bval = Vec::with_capacity(a.bval.len());
        for i in 0..a.aval.len() {
            let (a1, b1) = (a.aval[i], a.bval[i]);
            let (a2, b2) = (b.aval[i], b.bval[i]);
            let res1 = (a1 & !b1) | (a2 & !b2);
            let res0 = (!a1 & !b1) & (!a2 & !b2);
            aval.push(!res0);
            bval.push(!res0 & !res1);
        }
        let mut out = Self {
            width: w,
            aval,
            bval,
            signed,
        };
        out.mask_top();
        out
    }

    /// Bitwise XOR (`a ^ b`).
    pub fn bit_xor(&self, rhs: &Self) -> Self {
        let w = self.width.max(rhs.width);
        let signed = self.signed && rhs.signed;
        let a = self.resized(w, signed);
        let b = rhs.resized(w, signed);
        let mut aval = Vec::with_capacity(a.aval.len());
        let mut bval = Vec::with_capacity(a.bval.len());
        for i in 0..a.aval.len() {
            let bv = a.bval[i] | b.bval[i];
            aval.push((a.aval[i] ^ b.aval[i]) | bv);
            bval.push(bv);
        }
        let mut out = Self {
            width: w,
            aval,
            bval,
            signed,
        };
        out.mask_top();
        out
    }

    /// Bitwise XNOR (`a ~^ b` / `a ^~ b`).
    pub fn bit_xnor(&self, rhs: &Self) -> Self {
        self.bit_xor(rhs).bit_not()
    }

    /// Wire multi-driver resolution.
    pub fn resolve(&self, rhs: &Self) -> Self {
        let w = self.width.max(rhs.width);
        let a = self.resized(w, false);
        let b = rhs.resized(w, false);
        let mut aval = Vec::with_capacity(a.aval.len());
        let mut bval = Vec::with_capacity(a.bval.len());
        for i in 0..a.aval.len() {
            let (a1, b1) = (a.aval[i], a.bval[i]);
            let (a2, b2) = (b.aval[i], b.bval[i]);
            let z1 = !a1 & b1;
            let z2 = !a2 & b2;
            let same = !(a1 ^ a2) & !(b1 ^ b2);
            let conflict = !z1 & !z2 & !same;
            aval.push((z1 & a2) | (z2 & a1) | (!z1 & !z2 & same & a1) | conflict);
            bval.push((z1 & b2) | (z2 & b1) | (!z1 & !z2 & same & b1) | conflict);
        }
        let mut out = Self {
            width: w,
            aval,
            bval,
            signed: false,
        };
        out.mask_top();
        out
    }

    /// Reduction AND (`&a`).
    pub fn reduce_and(&self) -> Self {
        let mut has_xz = false;
        for i in 0..self.width {
            match self.bit(i) {
                Logic::Low => return Self::from_logic(Logic::Low),
                Logic::Unknown | Logic::HighZ => has_xz = true,
                Logic::High => {}
            }
        }
        Self::from_logic(if has_xz { Logic::Unknown } else { Logic::High })
    }

    /// Reduction OR (`|a`).
    pub fn reduce_or(&self) -> Self {
        Self::from_logic(self.to_bool_logic())
    }

    /// Reduction XOR (`^a`).
    pub fn reduce_xor(&self) -> Self {
        if self.has_xz() {
            return Self::from_logic(Logic::Unknown);
        }
        let parity: u32 = self.aval.iter().map(|w| w.count_ones()).sum();
        Self::from_bool(parity % 2 == 1)
    }

    /// Logical NOT (`!a`).
    pub fn logical_not(&self) -> Self {
        Self::from_logic(!self.to_bool_logic())
    }

    /// Logical AND (`a && b`).
    pub fn logical_and(&self, rhs: &Self) -> Self {
        Self::from_logic(self.to_bool_logic().and(rhs.to_bool_logic()))
    }

    /// Logical OR (`a || b`).
    pub fn logical_or(&self, rhs: &Self) -> Self {
        Self::from_logic(self.to_bool_logic().or(rhs.to_bool_logic()))
    }

    /// Case equality (`===`): exact 4-state bit-by-bit comparison.
    pub fn case_eq(&self, rhs: &Self) -> Self {
        let w = self.width.max(rhs.width);
        let signed = self.signed && rhs.signed;
        let a = self.resized(w, signed);
        let b = rhs.resized(w, signed);
        Self::from_bool(a.aval == b.aval && a.bval == b.bval)
    }

    /// Logical equality (`==`): returns X if any bit in either operand is X/Z.
    pub fn logic_eq(&self, rhs: &Self) -> Self {
        let w = self.width.max(rhs.width);
        let signed = self.signed && rhs.signed;
        let a = self.resized(w, signed);
        let b = rhs.resized(w, signed);
        if a.has_xz() || b.has_xz() {
            return Self::from_logic(Logic::Unknown);
        }
        Self::from_bool(a.aval == b.aval)
    }

    /// Wildcard equality (`==?`): X/Z bits in `rhs` are don't-cares.
    pub fn wild_eq(&self, rhs: &Self) -> Self {
        let w = self.width.max(rhs.width);
        let signed = self.signed && rhs.signed;
        let a = self.resized(w, signed);
        let b = rhs.resized(w, signed);
        let mut saw_x = false;
        for i in 0..w {
            match b.bit(i) {
                Logic::Unknown | Logic::HighZ => continue,
                rb => match a.bit(i) {
                    Logic::Unknown | Logic::HighZ => saw_x = true,
                    la if la != rb => return Self::from_bool(false),
                    _ => {}
                },
            }
        }
        Self::from_logic(if saw_x { Logic::Unknown } else { Logic::High })
    }

    /// `casez` match: Z bits in either operand are don't-cares.
    pub fn casez_match(&self, rhs: &Self) -> bool {
        let w = self.width.max(rhs.width);
        let signed = self.signed && rhs.signed;
        let a = self.resized(w, signed);
        let b = rhs.resized(w, signed);
        for i in 0..w {
            let la = a.bit(i);
            let lb = b.bit(i);
            if la == Logic::HighZ || lb == Logic::HighZ {
                continue;
            }
            if la != lb {
                return false;
            }
        }
        true
    }

    /// `casex` match: X or Z bits in either operand are don't-cares.
    pub fn casex_match(&self, rhs: &Self) -> bool {
        let w = self.width.max(rhs.width);
        let signed = self.signed && rhs.signed;
        let a = self.resized(w, signed);
        let b = rhs.resized(w, signed);
        for i in 0..w {
            let la = a.bit(i);
            let lb = b.bit(i);
            if matches!(la, Logic::HighZ | Logic::Unknown)
                || matches!(lb, Logic::HighZ | Logic::Unknown)
            {
                continue;
            }
            if la != lb {
                return false;
            }
        }
        true
    }

    /// Two's complement negation (`-a`).
    pub fn neg(&self) -> Self {
        if self.has_xz() {
            return Self::filled(self.width, Logic::Unknown);
        }
        let mut out = Self::zeros(self.width);
        out.signed = self.signed;
        let mut carry = 1u128;
        for i in 0..self.aval.len() {
            let sum = u128::from(!self.aval[i]) + carry;
            out.aval[i] = sum as u64;
            carry = sum >> 64;
        }
        out.mask_top();
        out
    }

    /// Addition (`a + b`) at width `result_width`.
    pub fn add(&self, rhs: &Self, result_width: usize, signed: bool) -> Self {
        let a = self.resized(result_width, signed);
        let b = rhs.resized(result_width, signed);
        if a.has_xz() || b.has_xz() {
            let mut out = Self::filled(result_width, Logic::Unknown);
            out.signed = signed;
            return out;
        }
        let mut out = Self::zeros(result_width);
        out.signed = signed;
        let mut carry = 0u128;
        for i in 0..out.aval.len() {
            let sum = u128::from(a.aval[i]) + u128::from(b.aval[i]) + carry;
            out.aval[i] = sum as u64;
            carry = sum >> 64;
        }
        out.mask_top();
        out
    }

    /// Subtraction (`a - b`) at width `result_width`.
    pub fn sub(&self, rhs: &Self, result_width: usize, signed: bool) -> Self {
        let a = self.resized(result_width, signed);
        let b = rhs.resized(result_width, signed);
        if a.has_xz() || b.has_xz() {
            let mut out = Self::filled(result_width, Logic::Unknown);
            out.signed = signed;
            return out;
        }
        let mut out = Self::zeros(result_width);
        out.signed = signed;
        let mut borrow = 0i128;
        for i in 0..out.aval.len() {
            let diff = i128::from(a.aval[i]) - i128::from(b.aval[i]) - borrow;
            if diff < 0 {
                out.aval[i] = (diff + (1i128 << 64)) as u64;
                borrow = 1;
            } else {
                out.aval[i] = diff as u64;
                borrow = 0;
            }
        }
        out.mask_top();
        out
    }

    /// Multiplication (`a * b`) at width `result_width`.
    pub fn mul(&self, rhs: &Self, result_width: usize, signed: bool) -> Self {
        let a = self.resized(result_width, signed);
        let b = rhs.resized(result_width, signed);
        if a.has_xz() || b.has_xz() {
            let mut out = Self::filled(result_width, Logic::Unknown);
            out.signed = signed;
            return out;
        }
        let words = word_count(result_width);
        let mut acc = vec![0u64; words];
        for i in 0..words {
            let mut carry = 0u128;
            for j in 0..(words - i) {
                let prod = u128::from(a.aval[i]) * u128::from(b.aval[j])
                    + u128::from(acc[i + j])
                    + carry;
                acc[i + j] = prod as u64;
                carry = prod >> 64;
            }
        }
        let mut out = Self {
            width: result_width,
            aval: acc,
            bval: vec![0; words],
            signed,
        };
        out.mask_top();
        out
    }

    /// Unsigned integer division and remainder.
    fn udivrem(a: &BitVec, b: &BitVec) -> Option<(BitVec, BitVec)> {
        let w = a.width.max(b.width);
        let a = a.resized(w, false);
        let b = b.resized(w, false);
        if a.has_xz() || b.has_xz() || b.is_all_zero() {
            return None;
        }
        if w <= 128 {
            let av = a.to_u128()?;
            let bv = b.to_u128()?;
            let q = av / bv;
            let r = av % bv;
            let mut qv = Self::zeros(w);
            let mut rv = Self::zeros(w);
            qv.aval[0] = q as u64;
            rv.aval[0] = r as u64;
            if qv.aval.len() > 1 {
                qv.aval[1] = (q >> 64) as u64;
                rv.aval[1] = (r >> 64) as u64;
            }
            qv.mask_top();
            rv.mask_top();
            return Some((qv, rv));
        }
        // Binary long division for >128-bit vectors
        let mut q = Self::zeros(w);
        let mut r = Self::zeros(w);
        for i in (0..w).rev() {
            r = r.shl_usize(1);
            r.set_bit(0, a.bit(i));
            if r.ucmp(&b) != std::cmp::Ordering::Less {
                r = r.sub(&b, w, false);
                q.set_bit(i, Logic::High);
            }
        }
        Some((q, r))
    }

    pub fn div(&self, rhs: &Self, result_width: usize, signed: bool) -> Self {
        let a = self.resized(result_width, signed);
        let b = rhs.resized(result_width, signed);
        if a.has_xz() || b.has_xz() || b.is_all_zero() {
            let mut out = Self::filled(result_width, Logic::Unknown);
            out.signed = signed;
            return out;
        }
        if signed {
            let neg_a = a.bit(result_width - 1) == Logic::High;
            let neg_b = b.bit(result_width - 1) == Logic::High;
            let abs_a = if neg_a { a.neg() } else { a };
            let abs_b = if neg_b { b.neg() } else { b };
            let Some((mut q, _)) = Self::udivrem(&abs_a, &abs_b) else {
                return Self::filled(result_width, Logic::Unknown);
            };
            if neg_a != neg_b {
                q = q.neg();
            }
            q.signed = true;
            q
        } else {
            Self::udivrem(&a, &b)
                .map(|(q, _)| q)
                .unwrap_or_else(|| Self::filled(result_width, Logic::Unknown))
        }
    }

    pub fn rem(&self, rhs: &Self, result_width: usize, signed: bool) -> Self {
        let a = self.resized(result_width, signed);
        let b = rhs.resized(result_width, signed);
        if a.has_xz() || b.has_xz() || b.is_all_zero() {
            let mut out = Self::filled(result_width, Logic::Unknown);
            out.signed = signed;
            return out;
        }
        if signed {
            let neg_a = a.bit(result_width - 1) == Logic::High;
            let neg_b = b.bit(result_width - 1) == Logic::High;
            let abs_a = if neg_a { a.neg() } else { a };
            let abs_b = if neg_b { b.neg() } else { b };
            let Some((_, mut r)) = Self::udivrem(&abs_a, &abs_b) else {
                return Self::filled(result_width, Logic::Unknown);
            };
            if neg_a {
                r = r.neg();
            }
            r.signed = true;
            r
        } else {
            Self::udivrem(&a, &b)
                .map(|(_, r)| r)
                .unwrap_or_else(|| Self::filled(result_width, Logic::Unknown))
        }
    }

    pub fn pow(&self, rhs: &Self, result_width: usize, signed: bool) -> Self {
        let a = self.resized(result_width, signed);
        if a.has_xz() || rhs.has_xz() {
            return Self::filled(result_width, Logic::Unknown);
        }
        let Some(exp) = rhs.to_u64() else {
            return Self::zeros(result_width);
        };
        let mut base = a;
        let mut res = Self::from_u64(1, result_width);
        res.signed = signed;
        let mut e = exp;
        while e > 0 {
            if e & 1 != 0 {
                res = res.mul(&base, result_width, signed);
            }
            e >>= 1;
            if e > 0 {
                base = base.mul(&base, result_width, signed);
            }
        }
        res
    }

    fn ucmp(&self, other: &Self) -> std::cmp::Ordering {
        for i in (0..self.aval.len()).rev() {
            let ord = self.aval[i].cmp(&other.aval[i]);
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        std::cmp::Ordering::Equal
    }

    pub fn cmp_ordering(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
        let w = self.width.max(rhs.width);
        let signed = self.signed && rhs.signed;
        let a = self.resized(w, signed);
        let b = rhs.resized(w, signed);
        if a.has_xz() || b.has_xz() {
            return None;
        }
        if signed {
            let na = a.bit(w - 1) == Logic::High;
            let nb = b.bit(w - 1) == Logic::High;
            match (na, nb) {
                (true, false) => Some(std::cmp::Ordering::Less),
                (false, true) => Some(std::cmp::Ordering::Greater),
                _ => Some(a.ucmp(&b)),
            }
        } else {
            Some(a.ucmp(&b))
        }
    }

    pub fn shl_usize(&self, shift: usize) -> Self {
        let mut out = Self::zeros(self.width);
        out.signed = self.signed;
        if shift >= self.width {
            return out;
        }
        for i in shift..self.width {
            out.set_bit(i, self.bit(i - shift));
        }
        out
    }

    pub fn shr_usize(&self, shift: usize, arithmetic: bool) -> Self {
        let fill = if arithmetic && self.signed {
            self.bit(self.width - 1)
        } else {
            Logic::Low
        };
        let mut out = Self::filled(self.width, fill);
        out.signed = self.signed;
        if shift >= self.width {
            return out;
        }
        for i in 0..(self.width - shift) {
            out.set_bit(i, self.bit(i + shift));
        }
        out
    }

    pub fn shl(&self, amount: &Self) -> Self {
        if amount.has_xz() {
            return Self::filled(self.width, Logic::Unknown);
        }
        let Some(s) = amount.to_u64() else {
            return Self::zeros(self.width);
        };
        self.shl_usize(s.min(self.width as u64) as usize)
    }

    pub fn shr(&self, amount: &Self, arithmetic: bool) -> Self {
        if amount.has_xz() {
            return Self::filled(self.width, Logic::Unknown);
        }
        let Some(s) = amount.to_u64() else {
            let fill = if arithmetic && self.signed {
                self.bit(self.width - 1)
            } else {
                Logic::Low
            };
            return Self::filled(self.width, fill);
        };
        self.shr_usize(s.min(self.width as u64) as usize, arithmetic)
    }
}

impl fmt::Display for BitVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in (0..self.width).rev() {
            write!(f, "{}", self.bit(i))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_state_bitwise_and_resolution() {
        let a = BitVec::from_u64(0b1010, 4);
        let z = BitVec::filled(4, Logic::HighZ);
        let b = BitVec::from_u64(0b0101, 4);
        assert_eq!(a.resolve(&z), a);
        assert_eq!(z.resolve(&b), b);
        assert_eq!(a.resolve(&b), BitVec::filled(4, Logic::Unknown));

        let x = BitVec::filled(1, Logic::Unknown);
        let zero = BitVec::from_bool(false);
        let one = BitVec::from_bool(true);
        assert_eq!(zero.bit_and(&x), zero);
        assert_eq!(one.bit_and(&x), x);
        assert_eq!(one.bit_or(&x), one);
        assert_eq!(zero.bit_or(&x), x);
    }

    #[test]
    fn signed_arithmetic_and_comparisons() {
        let neg5 = BitVec::from_i64(-5, 16);
        let pos3 = BitVec::from_i64(3, 16);
        assert_eq!(neg5.cmp_ordering(&pos3), Some(std::cmp::Ordering::Less));
        let sum = neg5.add(&pos3, 16, true);
        assert_eq!(sum.to_i64_signed(), Some(-2));
        let shifted = neg5.shr(&BitVec::from_u64(1, 4), true);
        assert_eq!(shifted.to_i64_signed(), Some(-3));
    }

    #[test]
    fn wide_128_bit_replicate_and_slice() {
        let mut pat = BitVec::zeros(4);
        pat.set_bit(3, Logic::High);
        pat.set_bit(2, Logic::Low);
        pat.set_bit(1, Logic::Unknown);
        pat.set_bit(0, Logic::HighZ);
        let wide = pat.replicate(32);
        assert_eq!(wide.width(), 128);
        assert_eq!(wide.slice(124, 4), pat);
        assert_eq!(wide.slice(0, 4), pat);
    }
}
