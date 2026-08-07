//! Cross-architecture float determinism: fixed-point + soft-float bits.

use core::cmp::Ordering;
use serde::{Deserialize, Serialize};

/// Q16.16 fixed-point for consensus-critical math.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FixedPoint(pub i32);

impl FixedPoint {
    pub const SCALE: i32 = 1 << 16;

    pub fn from_i32(v: i32) -> Self {
        Self(v.saturating_mul(Self::SCALE))
    }

    pub fn from_f32_truncated(v: f32) -> Self {
        let bits = canonicalize_f32_bits(v.to_bits());
        let v = f32::from_bits(bits);
        Self((v * Self::SCALE as f32) as i32)
    }

    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }

    pub fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    pub fn mul(self, rhs: Self) -> Self {
        let wide = (self.0 as i64) * (rhs.0 as i64);
        Self((wide >> 16) as i32)
    }
}

/// Canonicalize IEEE-754 f32 bit patterns: collapse all NaNs to a single quiet NaN,
/// and flatten -0.0 → +0.0 for consensus equality.
pub fn canonicalize_f32_bits(bits: u32) -> u32 {
    let exp = (bits >> 23) & 0xff;
    let frac = bits & 0x7f_ffff;
    if exp == 0xff && frac != 0 {
        // Canonical quiet NaN (positive sign, MSB of frac set).
        return 0x7fc0_0000;
    }
    if bits == 0x8000_0000 {
        return 0; // -0 → +0
    }
    bits
}

/// Soft-float wrapper storing canonical bits only.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SoftF32 {
    bits: u32,
}

impl SoftF32 {
    pub fn from_bits(bits: u32) -> Self {
        Self {
            bits: canonicalize_f32_bits(bits),
        }
    }

    pub fn from_f32(v: f32) -> Self {
        Self::from_bits(v.to_bits())
    }

    pub fn to_f32(self) -> f32 {
        f32::from_bits(self.bits)
    }

    pub fn bits(self) -> u32 {
        self.bits
    }

    /// Deterministic add via host f32 then canonicalize (documented: not bit-identical
    /// across all CPUs for non-associative chains; prefer FixedPoint for consensus).
    pub fn add(self, rhs: Self) -> Self {
        Self::from_f32(self.to_f32() + rhs.to_f32())
    }

    pub fn mul(self, rhs: Self) -> Self {
        Self::from_f32(self.to_f32() * rhs.to_f32())
    }
}

impl PartialEq for SoftF32 {
    fn eq(&self, other: &Self) -> bool {
        self.bits == other.bits
    }
}

impl Eq for SoftF32 {}

impl PartialOrd for SoftF32 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SoftF32 {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare as signed magnitude after canonicalize — NaNs already collapsed.
        self.to_f32()
            .partial_cmp(&other.to_f32())
            .unwrap_or(Ordering::Equal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_canonical() {
        let a = SoftF32::from_f32(f32::NAN);
        let b = SoftF32::from_bits(0x7f80_0001);
        assert_eq!(a.bits(), 0x7fc0_0000);
        assert_eq!(a, b);
    }

    #[test]
    fn neg_zero() {
        let z = SoftF32::from_f32(-0.0);
        assert_eq!(z.bits(), 0);
        assert_eq!(z, SoftF32::from_f32(0.0));
    }

    #[test]
    fn fixed_mul() {
        let a = FixedPoint::from_i32(3);
        let b = FixedPoint::from_i32(4);
        assert_eq!(a.mul(b).to_f32(), 12.0);
    }
}
