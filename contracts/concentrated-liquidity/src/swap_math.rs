#![no_std]

use soroban_sdk::{Env, Map};

use crate::{TickBitmap, TickInfo, PoolState, Error, SwapResult};
use crate::tick_math::{get_sqrt_ratio_at_tick, get_tick_at_sqrt_ratio, Q96, MAX_TICK, MIN_TICK};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwapStepResult {
    pub sqrt_price_x96: u128,
    pub tick: i32,
    pub liquidity: u128,
    pub amount_in: u128,
    pub amount_out: u128,
    pub fee_amount: u128,
}

pub fn compute_swap_step(
    sqrt_price_current_x96: u128,
    sqrt_price_target_x96: u128,
    liquidity: u128,
    amount_remaining: u128,
    fee_bps: u128,
    zero_for_one: bool,
) -> Result<SwapStepResult, Error> {
    if zero_for_one {
        let amount_in_max = liquidity
            .checked_mul(sqrt_price_current_x96.checked_sub(sqrt_price_target_x96).ok_or(Error::Underflow)?)
            .and_then(|v| v.checked_div(sqrt_price_target_x96))
            .and_then(|v| v.checked_div(Q96))
            .ok_or(Error::Overflow)?;
        
        let amount_in = amount_remaining.min(amount_in_max);
        
        let sqrt_price_next_x96 = if amount_in == amount_in_max {
            sqrt_price_target_x96
        } else {
            let numerator = liquidity.checked_mul(sqrt_price_current_x96).ok_or(Error::Overflow)?;
            let denominator = liquidity
                .checked_add(amount_in.checked_mul(Q96).ok_or(Error::Overflow)?)
                .ok_or(Error::Overflow)?;
            numerator / denominator
        };
        
        let amount_out = liquidity
            .checked_mul(sqrt_price_current_x96.checked_sub(sqrt_price_next_x96).ok_or(Error::Underflow)?)
            .and_then(|v| v.checked_div(Q96))
            .ok_or(Error::Overflow)?;
        
        let fee_amount = amount_in
            .checked_mul(fee_bps)
            .and_then(|v| v.checked_div(10000))
            .ok_or(Error::Overflow)?;
        
        let tick = get_tick_at_sqrt_ratio(sqrt_price_next_x96)?;
        
        Ok(SwapStepResult {
            sqrt_price_x96: sqrt_price_next_x96,
            tick,
            liquidity,
            amount_in,
            amount_out,
            fee_amount,
        })
    } else {
        let amount_in_max = liquidity
            .checked_mul(sqrt_price_target_x96.checked_sub(sqrt_price_current_x96).ok_or(Error::Underflow)?)
            .and_then(|v| v.checked_div(Q96))
            .ok_or(Error::Overflow)?;
        
        let amount_in = amount_remaining.min(amount_in_max);
        
        let sqrt_price_next_x96 = if amount_in == amount_in_max {
            sqrt_price_target_x96
        } else {
            liquidity
                .checked_add(amount_in.checked_mul(Q96).ok_or(Error::Overflow)?)
                .and_then(|v| v.checked_mul(sqrt_price_current_x96))
                .and_then(|v| v.checked_div(liquidity))
                .ok_or(Error::Overflow)?
        };
        
        let amount_out = liquidity
            .checked_mul(sqrt_price_next_x96.checked_sub(sqrt_price_current_x96).ok_or(Error::Underflow)?)
            .and_then(|v| v.checked_div(Q96))
            .ok_or(Error::Overflow)?;
        
        let fee_amount = amount_in
            .checked_mul(fee_bps)
            .and_then(|v| v.checked_div(10000))
            .ok_or(Error::Overflow)?;
        
        let tick = get_tick_at_sqrt_ratio(sqrt_price_next_x96)?;
        
        Ok(SwapStepResult {
            sqrt_price_x96: sqrt_price_next_x96,
            tick,
            liquidity,
            amount_in,
            amount_out,
            fee_amount,
        })
    }
}

pub fn swap(
    state: &mut PoolState,
    bitmap: &TickBitmap,
    ticks: &mut Map<i32, TickInfo>,
    zero_for_one: bool,
    amount_in: u128,
    amount_out_min: u128,
    sqrt_price_limit_x96: u128,
    fee_bps: u128,
) -> Result<SwapResult, Error> {
    let mut amount_remaining = amount_in;
    let mut amount_out_total = 0u128;
    let mut fee_amount_total = 0u128;
    let mut sqrt_price_x96 = state.sqrt_price_x96;
    let mut tick = state.tick;
    let mut liquidity = state.liquidity;

    while amount_remaining > 0 {
        let sqrt_price_target_x96 = if zero_for_one {
            sqrt_price_limit_x96.max(get_sqrt_ratio_at_tick(tick - 1)?)
        } else {
            sqrt_price_limit_x96.min(get_sqrt_ratio_at_tick(tick + 1)?)
        };

        if (zero_for_one && sqrt_price_x96 <= sqrt_price_target_x96) ||
           (!zero_for_one && sqrt_price_x96 >= sqrt_price_target_x96) {
            break;
        }

        let step = compute_swap_step(
            sqrt_price_x96,
            sqrt_price_target_x96,
            liquidity,
            amount_remaining,
            fee_bps,
            zero_for_one,
        )?;

        sqrt_price_x96 = step.sqrt_price_x96;
        tick = step.tick;
        amount_remaining = amount_remaining.checked_sub(step.amount_in).ok_or(Error::Underflow)?;
        amount_out_total = amount_out_total.checked_add(step.amount_out).ok_or(Error::Overflow)?;
        fee_amount_total = fee_amount_total.checked_add(step.fee_amount).ok_or(Error::Overflow)?;

        if tick != state.tick {
            let (next_tick, next_liquidity) = cross_tick(
                bitmap,
                ticks,
                tick,
                zero_for_one,
                liquidity,
                state.fee_growth_global_0_x128,
                state.fee_growth_global_1_x128,
            )?;
            
            tick = next_tick;
            liquidity = next_liquidity;
        }

        if zero_for_one && sqrt_price_x96 <= sqrt_price_limit_x96 {
            break;
        }
        if !zero_for_one && sqrt_price_x96 >= sqrt_price_limit_x96 {
            break;
        }
    }

    if amount_out_total < amount_out_min {
        return Err(Error::SlippageExceeded);
    }

    if zero_for_one {
        state.fee_growth_global_1_x128 = state.fee_growth_global_1_x128
            .checked_add(fee_amount_total.checked_mul(Q96).ok_or(Error::Overflow)?
                .checked_div(liquidity.max(1))
                .ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
    } else {
        state.fee_growth_global_0_x128 = state.fee_growth_global_0_x128
            .checked_add(fee_amount_total.checked_mul(Q96).ok_or(Error::Overflow)?
                .checked_div(liquidity.max(1))
                .ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
    }

    state.sqrt_price_x96 = sqrt_price_x96;
    state.tick = tick;
    state.liquidity = liquidity;

    Ok(SwapResult {
        amount_0: if zero_for_one { (amount_in - amount_remaining) as i128 } else { -((amount_out_total) as i128) },
        amount_1: if zero_for_one { (amount_out_total) as i128 } else { -((amount_in as i128) - (amount_remaining as i128)) },
        fee_amount: fee_amount_total,
        sqrt_price_x96_after: sqrt_price_x96,
        tick_after: tick,
        liquidity_after: liquidity,
    })
}

fn cross_tick(
    bitmap: &TickBitmap,
    ticks: &mut Map<i32, TickInfo>,
    tick: i32,
    zero_for_one: bool,
    liquidity: u128,
    fee_growth_global_0_x128: u128,
    fee_growth_global_1_x128: u128,
) -> Result<(i32, u128), Error> {
    let tick_spacing = bitmap.tick_spacing;
    
    let next_tick = if zero_for_one {
        let (found, next) = bitmap.next_initialized_tick_within_one_word(tick, true, tick_spacing)?;
        if found {
            next
        } else {
            MIN_TICK
        }
    } else {
        let (found, next) = bitmap.next_initialized_tick_within_one_word(tick, false, tick_spacing)?;
        if found {
            next
        } else {
            MAX_TICK
        }
    };

    if next_tick == tick {
        return Ok((tick, liquidity));
    }

    let mut tick_info = ticks.get(next_tick).unwrap_or(TickInfo {
        liquidity_gross: 0,
        liquidity_net: 0,
        fee_growth_outside_0_x128: 0,
        fee_growth_outside_1_x128: 0,
        tick_cumulative_outside: 0,
        seconds_per_liquidity_outside: 0,
        seconds_outside: 0,
        initialized: false,
    });

    if !tick_info.initialized {
        return Ok((next_tick, liquidity));
    }

    let liquidity_delta = tick_info.liquidity_net;
    let new_liquidity = if zero_for_one {
        if liquidity_delta > 0 {
            liquidity.checked_add(liquidity_delta as u128).ok_or(Error::Overflow)?
        } else {
            liquidity.checked_sub((-liquidity_delta) as u128).ok_or(Error::Underflow)?
        }
    } else {
        if liquidity_delta > 0 {
            liquidity.checked_sub(liquidity_delta as u128).ok_or(Error::Underflow)?
        } else {
            liquidity.checked_add((-liquidity_delta) as u128).ok_or(Error::Overflow)?
        }
    };

    if zero_for_one {
        tick_info.fee_growth_outside_0_x128 = fee_growth_global_0_x128;
        tick_info.fee_growth_outside_1_x128 = fee_growth_global_1_x128;
    } else {
        tick_info.fee_growth_outside_0_x128 = fee_growth_global_0_x128;
        tick_info.fee_growth_outside_1_x128 = fee_growth_global_1_x128;
    }

    ticks.set(next_tick, tick_info);

    Ok((next_tick, new_liquidity))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_swap_step_zero_for_one() {
        let sqrt_current = 79228162514264337593543950336u128;
        let sqrt_target = 79228162514264337593543950336u128;
        let liquidity = 1000000000000000000u128;
        let amount = 100000000000000000u128;
        let fee = 300u128;
        
        let result = compute_swap_step(sqrt_current, sqrt_target, liquidity, amount, fee, true).unwrap();
        assert_eq!(result.amount_in, 0);
    }
}