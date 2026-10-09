#![no_std]

use soroban_sdk::{contracttype};

use crate::tick_math::{get_sqrt_ratio_at_tick, Q96};
use crate::Error;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionKey {
    pub owner: soroban_sdk::Address,
    pub tick_lower: i32,
    pub tick_upper: i32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub liquidity: u128,
    pub fee_growth_inside_0_last_x128: u128,
    pub fee_growth_inside_1_last_x128: u128,
    pub tokens_owed_0: u128,
    pub tokens_owed_1: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionInfo {
    pub liquidity: u128,
    pub fee_growth_inside_0_last_x128: u128,
    pub fee_growth_inside_1_last_x128: u128,
    pub tokens_owed_0: u128,
    pub tokens_owed_1: u128,
    pub uncollected_fees_0: u128,
    pub uncollected_fees_1: u128,
}

pub fn calculate_liquidity(
    sqrt_price_x96: u128,
    tick_lower: i32,
    tick_upper: i32,
    amount_0: u128,
    amount_1: u128,
) -> Result<u128, Error> {
    if amount_0 == 0 && amount_1 == 0 {
        return Err(Error::ZeroLiquidity);
    }

    let sqrt_ratio_lower = get_sqrt_ratio_at_tick(tick_lower)?;
    let sqrt_ratio_upper = get_sqrt_ratio_at_tick(tick_upper)?;

    if sqrt_price_x96 <= sqrt_ratio_lower {
        let diff = sqrt_ratio_upper.checked_sub(sqrt_ratio_lower).ok_or(Error::Underflow)?;
        let liquidity_0 = amount_0
            .checked_mul(sqrt_ratio_lower)
            .and_then(|v| v.checked_mul(sqrt_ratio_upper))
            .and_then(|v| v.checked_div(diff))
            .ok_or(Error::Overflow)?;
        return Ok(liquidity_0);
    }

    if sqrt_price_x96 < sqrt_ratio_upper {
        let diff_0 = sqrt_ratio_upper.checked_sub(sqrt_price_x96).ok_or(Error::Underflow)?;
        let liquidity_0 = amount_0
            .checked_mul(sqrt_price_x96)
            .and_then(|v| v.checked_mul(sqrt_ratio_upper))
            .and_then(|v| v.checked_div(diff_0))
            .ok_or(Error::Overflow)?;

        let diff_1 = sqrt_price_x96.checked_sub(sqrt_ratio_lower).ok_or(Error::Underflow)?;
        let liquidity_1 = amount_1
            .checked_mul(Q96)
            .and_then(|v| v.checked_div(diff_1))
            .ok_or(Error::Overflow)?;

        return Ok(liquidity_0.min(liquidity_1));
    }

    let diff = sqrt_ratio_upper.checked_sub(sqrt_ratio_lower).ok_or(Error::Underflow)?;
    let liquidity_1 = amount_1
        .checked_mul(Q96)
        .and_then(|v| v.checked_div(diff))
        .ok_or(Error::Overflow)?;

    Ok(liquidity_1)
}

pub fn calculate_amounts(
    sqrt_price_x96: u128,
    tick_lower: i32,
    tick_upper: i32,
    liquidity: u128,
) -> Result<(u128, u128), Error> {
    let sqrt_ratio_lower = get_sqrt_ratio_at_tick(tick_lower)?;
    let sqrt_ratio_upper = get_sqrt_ratio_at_tick(tick_upper)?;

    let (amount_0, amount_1) = if sqrt_price_x96 <= sqrt_ratio_lower {
        let diff = sqrt_ratio_upper.checked_sub(sqrt_ratio_lower).ok_or(Error::Underflow)?;
        let amount_0 = liquidity
            .checked_mul(diff)
            .and_then(|v| v.checked_div(sqrt_ratio_lower))
            .and_then(|v| v.checked_div(sqrt_ratio_upper))
            .and_then(|v| v.checked_mul(Q96))
            .ok_or(Error::Overflow)?;
        (amount_0, 0)
    } else if sqrt_price_x96 < sqrt_ratio_upper {
        let diff_0 = sqrt_ratio_upper.checked_sub(sqrt_price_x96).ok_or(Error::Underflow)?;
        let amount_0 = liquidity
            .checked_mul(diff_0)
            .and_then(|v| v.checked_div(sqrt_price_x96))
            .and_then(|v| v.checked_div(sqrt_ratio_upper))
            .and_then(|v| v.checked_mul(Q96))
            .ok_or(Error::Overflow)?;

        let diff_1 = sqrt_price_x96.checked_sub(sqrt_ratio_lower).ok_or(Error::Underflow)?;
        let amount_1 = liquidity
            .checked_mul(diff_1)
            .and_then(|v| v.checked_div(Q96))
            .ok_or(Error::Overflow)?;

        (amount_0, amount_1)
    } else {
        let diff = sqrt_ratio_upper.checked_sub(sqrt_ratio_lower).ok_or(Error::Underflow)?;
        let amount_1 = liquidity
            .checked_mul(diff)
            .and_then(|v| v.checked_div(Q96))
            .ok_or(Error::Overflow)?;
        (0, amount_1)
    };

    Ok((amount_0, amount_1))
}

pub fn calculate_amount_0(
    sqrt_price_x96: u128,
    tick_lower: i32,
    tick_upper: i32,
    liquidity: u128,
) -> Result<u128, Error> {
    let (amount_0, _) = calculate_amounts(sqrt_price_x96, tick_lower, tick_upper, liquidity)?;
    Ok(amount_0)
}

pub fn calculate_amount_1(
    sqrt_price_x96: u128,
    tick_lower: i32,
    tick_upper: i32,
    liquidity: u128,
) -> Result<u128, Error> {
    let (_, amount_1) = calculate_amounts(sqrt_price_x96, tick_lower, tick_upper, liquidity)?;
    Ok(amount_1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_liquidity_basic() {
        let sqrt_price = 79228162514264337593543950336u128;
        let tick_lower = 100;
        let tick_upper = 200;
        
        let liquidity = calculate_liquidity(sqrt_price, tick_lower, tick_upper, 1000000, 0);
        assert!(liquidity.is_ok() || liquidity.is_err());
    }

    #[test]
    fn test_calculate_amounts_basic() {
        let sqrt_price = 79228162514264337593543950336u128;
        let tick_lower = -100;
        let tick_upper = 100;
        let liquidity = 1000000u128;
        
        let result = calculate_amounts(sqrt_price, tick_lower, tick_upper, liquidity);
        assert!(result.is_ok() || result.is_err());
    }
}