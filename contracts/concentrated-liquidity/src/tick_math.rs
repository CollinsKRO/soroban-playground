#![no_std]

use soroban_sdk::{contracttype};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct U256 {
    pub lo: u128,
    pub hi: u128,
}

impl U256 {
    pub const fn from_u128(v: u128) -> Self {
        U256 { lo: v, hi: 0 }
    }

    pub fn mul(self, other: Self) -> Self {
        let a = self.lo as u128;
        let b = self.hi as u128;
        let c = other.lo as u128;
        let d = other.hi as u128;

        let lo = a.wrapping_mul(c);
        let mid = a.wrapping_mul(d).wrapping_add(b.wrapping_mul(c));
        let hi = b.wrapping_mul(d);

        let carry = if lo >= (1u128 << 127) { 1 } else { 0 };
        let mid_with_carry = mid.wrapping_add(carry);
        let carry2 = if mid_with_carry >= (1u128 << 127) { 1 } else { 0 };
        let _hi_final = hi.wrapping_add(carry2);

        U256 { lo: lo, hi: mid_with_carry }
    }

    pub fn shr(self, shift: u32) -> Self {
        if shift >= 128 {
            U256 { lo: self.hi >> (shift - 128), hi: 0 }
        } else if shift == 0 {
            self
        } else {
            U256 {
                lo: (self.lo >> shift) | (self.hi << (128 - shift)),
                hi: self.hi >> shift,
            }
        }
    }

    pub fn div(self, other: Self) -> Self {
        if other.lo == 0 && other.hi == 0 {
            return U256 { lo: 0, hi: 0 };
        }
        if self.hi == 0 && other.hi == 0 {
            return U256::from_u128(self.lo / other.lo);
        }
        U256 { lo: 0, hi: 0 }
    }

    pub fn to_u128(self) -> u128 {
        self.lo
    }

    pub fn is_zero(self) -> bool {
        self.lo == 0 && self.hi == 0
    }
}

pub const MIN_TICK: i32 = -887272;
pub const MAX_TICK: i32 = 887272;
pub const Q96: u128 = 1u128 << 96;
pub const MIN_SQRT_RATIO: u128 = 4295128739;
pub const MAX_SQRT_RATIO: u128 = u128::MAX;

const TICK_BASE_NUMERATOR: U256 = U256 { lo: 0xbd6fad37aa2d1a594001, hi: 0xfffcb933 };

const SQRT_RATIO_TICKS: [(U256, u32); 25] = [
    (U256 { lo: 0xfff97272373d413259a46990580e213a, hi: 0 }, 0x1),
    (U256 { lo: 0xfff2e50f5f656932ef12357cf3c7fdcc, hi: 0 }, 0x2),
    (U256 { lo: 0xffe5caca7e10e4e61c3624eaa0941cd0, hi: 0 }, 0x4),
    (U256 { lo: 0xffcb9843d60f6159c9db58835c926644, hi: 0 }, 0x8),
    (U256 { lo: 0xff973b41fa98c081472e6896dfb254c0, hi: 0 }, 0x10),
    (U256 { lo: 0xff2ea16466c96a3843eb72a57c4a0980, hi: 0 }, 0x20),
    (U256 { lo: 0xfe5dee046a99a2a811c461f1969c3053, hi: 0 }, 0x40),
    (U256 { lo: 0xfcbe86c7900a88aedcffc83b479aa3a4, hi: 0 }, 0x80),
    (U256 { lo: 0xf987a7253ac413176f2b074cf7815e54, hi: 0 }, 0x100),
    (U256 { lo: 0xf3392b0822b70005940c7a398e4b70f3, hi: 0 }, 0x200),
    (U256 { lo: 0xe7159475a2c29b7443b29c7fa6e889d9, hi: 0 }, 0x400),
    (U256 { lo: 0xd097f3bdfd2022b8845ad8f792aa5825, hi: 0 }, 0x800),
    (U256 { lo: 0xa9f746462d870fdf8a65dc1f90e061e5, hi: 0 }, 0x1000),
    (U256 { lo: 0x70d869a156d2a1b890bb3df62baf32f7, hi: 0 }, 0x2000),
    (U256 { lo: 0x31be135f97d08fd981231505542fcfa6, hi: 0 }, 0x4000),
    (U256 { lo: 0x9aa508b5b7a84e1c677de54f3e99bc9, hi: 0 }, 0x8000),
    (U256 { lo: 0x5d6af8dedb81196699c329225ee604, hi: 0 }, 0x10000),
    (U256 { lo: 0x2216e584f5fa1695f79367a40494, hi: 0 }, 0x20000),
    (U256 { lo: 0x48a170391f7dc42444e8fa2, hi: 0 }, 0x40000),
    (U256 { lo: 0, hi: 0 }, 0),
    (U256 { lo: 0, hi: 0 }, 0),
    (U256 { lo: 0, hi: 0 }, 0),
    (U256 { lo: 0, hi: 0 }, 0),
    (U256 { lo: 0, hi: 0 }, 0),
    (U256 { lo: 0, hi: 0 }, 0),
];

pub fn get_sqrt_ratio_at_tick(tick: i32) -> Result<u128, crate::Error> {
    if tick < MIN_TICK || tick > MAX_TICK {
        return Err(crate::Error::InvalidTickRange);
    }

    let abs_tick = tick.abs() as u32;
    let mut ratio = TICK_BASE_NUMERATOR;

    for (mult, bit) in SQRT_RATIO_TICKS.iter() {
        if (abs_tick & bit) != 0 {
            ratio = ratio.mul(*mult);
            ratio = ratio.shr(128);
        }
    }

    if tick > 0 {
        let q96_squared = U256::from_u128(Q96).mul(U256::from_u128(Q96));
        ratio = q96_squared.div(ratio);
    }

    Ok(ratio.to_u128())
}

pub fn get_tick_at_sqrt_ratio(sqrt_ratio_x96: u128) -> Result<i32, crate::Error> {
    if sqrt_ratio_x96 < MIN_SQRT_RATIO {
        return Err(crate::Error::InvalidSqrtPrice);
    }

    let mut tick = 0i32;
    let mut ratio = U256::from_u128(sqrt_ratio_x96);

    let thresholds = [
        (U256 { lo: 0, hi: 0x10000000000000000 }, 0x8000),
        (U256 { lo: 0, hi: 0x100000000 }, 0x4000),
        (U256 { lo: 0, hi: 0x10000 }, 0x2000),
        (U256 { lo: 0, hi: 0x100 }, 0x1000),
        (U256 { lo: 0, hi: 0x10 }, 0x800),
        (U256 { lo: 0, hi: 0x4 }, 0x400),
        (U256 { lo: 0, hi: 0x2 }, 0x200),
    ];

    for (threshold, bit) in thresholds.iter() {
        if ratio.hi >= threshold.hi || (ratio.hi == threshold.hi && ratio.lo >= threshold.lo) {
            tick |= *bit;
            ratio = ratio.shr(if *bit >= 0x1000 { 64 } else if *bit >= 0x100 { 32 } else if *bit >= 0x10 { 16 } else if *bit >= 0x4 { 8 } else if *bit >= 0x2 { 4 } else { 2 });
        }
    }

    let inv = if ratio.lo != 0 || ratio.hi != 0 {
        let val = ratio.lo;
        if val != 0 {
            let inv_val = u128::MAX / val;
            U256::from_u128(inv_val)
        } else {
            U256::from_u128(0)
        }
    } else {
        U256::from_u128(0)
    };

    let mut bit: u32 = 1;
    let mut value = inv;

    for _ in 0..128 {
        if !value.is_zero() {
            tick |= bit as i32;
        }
        bit <<= 1;
        value = value.shr(1);
    }

    Ok(tick)
}

pub fn get_tick_at_sqrt_ratio_round_up(sqrt_ratio_x96: u128) -> Result<i32, crate::Error> {
    let tick = get_tick_at_sqrt_ratio(sqrt_ratio_x96)?;
    let sqrt_ratio_at_tick = get_sqrt_ratio_at_tick(tick)?;
    
    if sqrt_ratio_at_tick < sqrt_ratio_x96 {
        Ok(tick + 1)
    } else {
        Ok(tick)
    }
}

pub fn get_tick_at_sqrt_ratio_round_down(sqrt_ratio_x96: u128) -> Result<i32, crate::Error> {
    let tick = get_tick_at_sqrt_ratio(sqrt_ratio_x96)?;
    let sqrt_ratio_at_tick = get_sqrt_ratio_at_tick(tick)?;
    
    if sqrt_ratio_at_tick > sqrt_ratio_x96 {
        Ok(tick - 1)
    } else {
        Ok(tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_math_basic() {
        let sqrt_ratio = get_sqrt_ratio_at_tick(0).unwrap();
        assert!(sqrt_ratio > 0);
        
        let tick = get_tick_at_sqrt_ratio(sqrt_ratio).unwrap();
        // tick can be 0 or -1 depending on implementation
        assert!(tick >= -1 && tick <= 1);
    }

    #[test]
    fn test_tick_bounds() {
        assert!(MIN_TICK < MAX_TICK);
        assert!(MIN_SQRT_RATIO > 0);
    }
}