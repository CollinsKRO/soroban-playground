#![no_std]

use soroban_sdk::{Env, Map};

use crate::{TickInfo, Position, Error};
use crate::tick_math::Q96;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeGrowthGlobal {
    pub fee_growth_global_0_x128: u128,
    pub fee_growth_global_1_x128: u128,
}

pub fn compute_fee_growth(
    fee_amount: u128,
    liquidity: u128,
) -> u128 {
    if liquidity == 0 {
        return 0;
    }
    fee_amount
        .checked_mul(Q96)
        .and_then(|v| v.checked_div(liquidity))
        .unwrap_or(0)
}

pub fn update_fee_growth_global(
    fee_growth_global_0_x128: u128,
    fee_growth_global_1_x128: u128,
    fee_amount_0: u128,
    fee_amount_1: u128,
    liquidity: u128,
) -> (u128, u128) {
    let growth_0 = compute_fee_growth(fee_amount_0, liquidity);
    let growth_1 = compute_fee_growth(fee_amount_1, liquidity);
    
    (
        fee_growth_global_0_x128.saturating_add(growth_0),
        fee_growth_global_1_x128.saturating_add(growth_1),
    )
}

pub fn calculate_uncollected_fees_for_position(
    position: &Position,
    tick_lower: i32,
    tick_upper: i32,
    fee_growth_global_0_x128: u128,
    fee_growth_global_1_x128: u128,
    ticks: &Map<i32, TickInfo>,
) -> Result<(u128, u128), Error> {
    let tick_lower_info = ticks.get(tick_lower).ok_or(Error::TickNotInitialized)?;
    let tick_upper_info = ticks.get(tick_upper).ok_or(Error::TickNotInitialized)?;

    let fee_growth_below_0 = tick_lower_info.fee_growth_outside_0_x128;
    let fee_growth_above_0 = tick_upper_info.fee_growth_outside_0_x128;
    let fee_growth_below_1 = tick_lower_info.fee_growth_outside_1_x128;
    let fee_growth_above_1 = tick_upper_info.fee_growth_outside_1_x128;

    let fee_growth_inside_0 = if fee_growth_global_0_x128 >= fee_growth_below_0 && fee_growth_global_0_x128 >= fee_growth_above_0 {
        fee_growth_global_0_x128 - fee_growth_below_0 - fee_growth_above_0
    } else if fee_growth_global_0_x128 >= fee_growth_below_0 {
        fee_growth_global_0_x128 - fee_growth_below_0
    } else if fee_growth_global_0_x128 >= fee_growth_above_0 {
        fee_growth_global_0_x128 - fee_growth_above_0
    } else {
        0
    };

    let fee_growth_inside_1 = if fee_growth_global_1_x128 >= fee_growth_below_1 && fee_growth_global_1_x128 >= fee_growth_above_1 {
        fee_growth_global_1_x128 - fee_growth_below_1 - fee_growth_above_1
    } else if fee_growth_global_1_x128 >= fee_growth_below_1 {
        fee_growth_global_1_x128 - fee_growth_below_1
    } else if fee_growth_global_1_x128 >= fee_growth_above_1 {
        fee_growth_global_1_x128 - fee_growth_above_1
    } else {
        0
    };

    let uncollected_0 = if position.liquidity > 0 {
        let growth_diff_0 = fee_growth_inside_0
            .checked_sub(position.fee_growth_inside_0_last_x128)
            .ok_or(Error::Underflow)?;
        growth_diff_0
            .checked_mul(position.liquidity)
            .and_then(|v| v.checked_div(Q96))
            .ok_or(Error::Overflow)?
    } else {
        0
    };

    let uncollected_1 = if position.liquidity > 0 {
        let growth_diff_1 = fee_growth_inside_1
            .checked_sub(position.fee_growth_inside_1_last_x128)
            .ok_or(Error::Underflow)?;
        growth_diff_1
            .checked_mul(position.liquidity)
            .and_then(|v| v.checked_div(Q96))
            .ok_or(Error::Overflow)?
    } else {
        0
    };

    Ok((uncollected_0, uncollected_1))
}

pub fn collect_fees(
    position: &mut Position,
    uncollected_0: u128,
    uncollected_1: u128,
) -> (u128, u128) {
    let collected_0 = position.tokens_owed_0 + uncollected_0;
    let collected_1 = position.tokens_owed_1 + uncollected_1;
    
    position.tokens_owed_0 = collected_0;
    position.tokens_owed_1 = collected_1;
    
    (collected_0, collected_1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_fee_growth() {
        let fee = 1000000000000000000u128;
        let liquidity = 1000000000000u128;
        
        let growth = compute_fee_growth(fee, liquidity);
        assert!(growth >= 0);
    }

    #[test]
    fn test_compute_fee_growth_zero_liquidity() {
        let fee = 1000000000000000000u128;
        let liquidity = 0u128;
        
        let growth = compute_fee_growth(fee, liquidity);
        assert_eq!(growth, 0);
    }
}