#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror,
    symbol_short, Address, Env, Map,
};

mod tick_math;
mod position;
mod bitmap;
mod swap_math;
mod fee_math;

pub use tick_math::{get_sqrt_ratio_at_tick, get_tick_at_sqrt_ratio, MAX_TICK, MIN_TICK, Q96, MIN_SQRT_RATIO, MAX_SQRT_RATIO};
pub use position::{Position, PositionKey, PositionInfo};
pub use bitmap::TickBitmap;
pub use swap_math::{compute_swap_step, SwapStepResult};
pub use fee_math::{compute_fee_growth, FeeGrowthGlobal};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolConfig {
    pub token_a: Address,
    pub token_b: Address,
    pub fee_bps: u32,
    pub tick_spacing: i32,
    pub admin: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolState {
    pub sqrt_price_x96: u128,
    pub tick: i32,
    pub liquidity: u128,
    pub fee_growth_global_0_x128: u128,
    pub fee_growth_global_1_x128: u128,
    pub protocol_fees_0: u128,
    pub protocol_fees_1: u128,
    pub liquidity_gross: u128,
    pub liquidity_net: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickInfo {
    pub liquidity_gross: u128,
    pub liquidity_net: i128,
    pub fee_growth_outside_0_x128: u128,
    pub fee_growth_outside_1_x128: u128,
    pub tick_cumulative_outside: i64,
    pub seconds_per_liquidity_outside: u128,
    pub seconds_outside: u64,
    pub initialized: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwapResult {
    pub amount_0: i128,
    pub amount_1: i128,
    pub fee_amount: u128,
    pub sqrt_price_x96_after: u128,
    pub tick_after: i32,
    pub liquidity_after: u128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Unauthorized = 1,
    NotInitialized = 2,
    AlreadyInitialized = 3,
    InvalidTickRange = 4,
    TickLowerGreaterThanUpper = 5,
    TickSpacingMismatch = 6,
    InsufficientLiquidity = 7,
    SlippageExceeded = 8,
    InvalidAmount = 9,
    ZeroLiquidity = 10,
    Overflow = 11,
    Underflow = 12,
    InvalidSqrtPrice = 13,
    TickNotInitialized = 14,
    PositionNotFound = 15,
    InvalidFee = 16,
    PriceLimitExceeded = 17,
}

const MAX_FEE_BPS: u32 = 10000;
const MIN_TICK_SPACING: i32 = 1;
const MAX_TICK_SPACING: i32 = 10000;
const PROTOCOL_FEE_DENOMINATOR: u128 = 10000;
const MAX_PROTOCOL_FEE: u128 = 10000;

#[contract]
pub struct ConcentratedLiquidity;

#[contractimpl]
impl ConcentratedLiquidity {
    pub fn initialize(
        env: Env,
        admin: Address,
        token_a: Address,
        token_b: Address,
        fee_bps: u32,
        tick_spacing: i32,
        sqrt_price_x96: u128,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&symbol_short!("config")) {
            return Err(Error::AlreadyInitialized);
        }

        if token_a == token_b {
            return Err(Error::InvalidAmount);
        }

        if fee_bps == 0 || fee_bps > MAX_FEE_BPS {
            return Err(Error::InvalidFee);
        }

        if tick_spacing < MIN_TICK_SPACING || tick_spacing > MAX_TICK_SPACING {
            return Err(Error::InvalidTickRange);
        }

        if sqrt_price_x96 < MIN_SQRT_RATIO || sqrt_price_x96 > MAX_SQRT_RATIO {
            return Err(Error::InvalidSqrtPrice);
        }

        admin.require_auth();

        let config = PoolConfig {
            token_a: token_a.clone(),
            token_b: token_b.clone(),
            fee_bps,
            tick_spacing,
            admin: admin.clone(),
        };

        let state = PoolState {
            sqrt_price_x96,
            tick: tick_math::get_tick_at_sqrt_ratio(sqrt_price_x96)?,
            liquidity: 0,
            fee_growth_global_0_x128: 0,
            fee_growth_global_1_x128: 0,
            protocol_fees_0: 0,
            protocol_fees_1: 0,
            liquidity_gross: 0,
            liquidity_net: 0,
        };

        let bitmap = TickBitmap::new(tick_spacing);

        env.storage().instance().set(&symbol_short!("config"), &config);
        env.storage().instance().set(&symbol_short!("state"), &state);
        env.storage().instance().set(&symbol_short!("bitmap"), &bitmap);
        env.storage().instance().set(&symbol_short!("positions"), &Map::<PositionKey, Position>::new(&env));
        env.storage().instance().set(&symbol_short!("ticks"), &Map::<i32, TickInfo>::new(&env));

        env.events().publish(
            (symbol_short!("init"),),
            (admin, token_a, token_b, fee_bps, tick_spacing, sqrt_price_x96),
        );

        Ok(())
    }

    pub fn mint_position(
        env: Env,
        owner: Address,
        tick_lower: i32,
        tick_upper: i32,
        amount_0_desired: u128,
        amount_1_desired: u128,
        amount_0_min: u128,
        amount_1_min: u128,
    ) -> Result<PositionKey, Error> {
        let config: PoolConfig = env.storage().instance().get(&symbol_short!("config")).ok_or(Error::NotInitialized)?;
        let mut state: PoolState = env.storage().instance().get(&symbol_short!("state")).ok_or(Error::NotInitialized)?;
        let mut bitmap: TickBitmap = env.storage().instance().get(&symbol_short!("bitmap")).ok_or(Error::NotInitialized)?;
        let mut positions: Map<PositionKey, Position> = env.storage().instance().get(&symbol_short!("positions")).ok_or(Error::NotInitialized)?;
        let mut ticks: Map<i32, TickInfo> = env.storage().instance().get(&symbol_short!("ticks")).ok_or(Error::NotInitialized)?;

        owner.require_auth();

        if tick_lower >= tick_upper {
            return Err(Error::TickLowerGreaterThanUpper);
        }

        if tick_lower % config.tick_spacing != 0 || tick_upper % config.tick_spacing != 0 {
            return Err(Error::TickSpacingMismatch);
        }

        if tick_lower < MIN_TICK || tick_upper > MAX_TICK {
            return Err(Error::InvalidTickRange);
        }

        let liquidity = position::calculate_liquidity(
            state.sqrt_price_x96,
            tick_lower,
            tick_upper,
            amount_0_desired,
            amount_1_desired,
        )?;

        if liquidity == 0 {
            return Err(Error::ZeroLiquidity);
        }

        let (amount_0, amount_1) = position::calculate_amounts(
            state.sqrt_price_x96,
            tick_lower,
            tick_upper,
            liquidity,
        )?;

        if amount_0 < amount_0_min || amount_1 < amount_1_min {
            return Err(Error::SlippageExceeded);
        }

        let position_key = PositionKey { owner: owner.clone(), tick_lower, tick_upper };
        
        let mut position = positions.get(position_key.clone()).unwrap_or(Position {
            liquidity: 0,
            fee_growth_inside_0_last_x128: 0,
            fee_growth_inside_1_last_x128: 0,
            tokens_owed_0: 0,
            tokens_owed_1: 0,
        });

        position.liquidity = position.liquidity.checked_add(liquidity).ok_or(Error::Overflow)?;
        position.fee_growth_inside_0_last_x128 = state.fee_growth_global_0_x128;
        position.fee_growth_inside_1_last_x128 = state.fee_growth_global_1_x128;

        positions.set(position_key.clone(), position);

        bitmap.set_tick_initialized(tick_lower, true)?;
        bitmap.set_tick_initialized(tick_upper, true)?;

        update_tick(&mut ticks, tick_lower, liquidity, true, state.fee_growth_global_0_x128, state.fee_growth_global_1_x128)?;
        update_tick(&mut ticks, tick_upper, liquidity, false, state.fee_growth_global_0_x128, state.fee_growth_global_1_x128)?;

        state.liquidity = state.liquidity.checked_add(liquidity).ok_or(Error::Overflow)?;
        state.liquidity_gross = state.liquidity_gross.checked_add(liquidity).ok_or(Error::Overflow)?;

        env.storage().instance().set(&symbol_short!("state"), &state);
        env.storage().instance().set(&symbol_short!("bitmap"), &bitmap);
        env.storage().instance().set(&symbol_short!("positions"), &positions);
        env.storage().instance().set(&symbol_short!("ticks"), &ticks);

        env.events().publish(
            (symbol_short!("mint"),),
            (owner, tick_lower, tick_upper, liquidity, amount_0, amount_1),
        );

        Ok(position_key)
    }

    pub fn burn_position(
        env: Env,
        owner: Address,
        tick_lower: i32,
        tick_upper: i32,
        liquidity: u128,
    ) -> Result<(u128, u128), Error> {
        let config: PoolConfig = env.storage().instance().get(&symbol_short!("config")).ok_or(Error::NotInitialized)?;
        let mut state: PoolState = env.storage().instance().get(&symbol_short!("state")).ok_or(Error::NotInitialized)?;
        let mut bitmap: TickBitmap = env.storage().instance().get(&symbol_short!("bitmap")).ok_or(Error::NotInitialized)?;
        let mut positions: Map<PositionKey, Position> = env.storage().instance().get(&symbol_short!("positions")).ok_or(Error::NotInitialized)?;
        let mut ticks: Map<i32, TickInfo> = env.storage().instance().get(&symbol_short!("ticks")).ok_or(Error::NotInitialized)?;

        owner.require_auth();

        let position_key = PositionKey { owner: owner.clone(), tick_lower, tick_upper };
        let mut position = positions.get(position_key.clone()).ok_or(Error::PositionNotFound)?;

        if position.liquidity < liquidity {
            return Err(Error::InsufficientLiquidity);
        }

        let (amount_0, amount_1) = position::calculate_amounts(
            state.sqrt_price_x96,
            tick_lower,
            tick_upper,
            liquidity,
        )?;

        position.liquidity = position.liquidity.checked_sub(liquidity).ok_or(Error::Underflow)?;
        let new_liquidity = position.liquidity.checked_sub(liquidity).ok_or(Error::Underflow)?;
        position.liquidity = new_liquidity;
        position.tokens_owed_0 = position.tokens_owed_0.checked_add(amount_0).ok_or(Error::Overflow)?;
        position.tokens_owed_1 = position.tokens_owed_1.checked_add(amount_1).ok_or(Error::Overflow)?;

        positions.set(position_key.clone(), position);

        update_tick(&mut ticks, tick_lower, liquidity, false, state.fee_growth_global_0_x128, state.fee_growth_global_1_x128)?;
        update_tick(&mut ticks, tick_upper, liquidity, true, state.fee_growth_global_0_x128, state.fee_growth_global_1_x128)?;

        state.liquidity = state.liquidity.checked_sub(liquidity).ok_or(Error::Underflow)?;
        state.liquidity_gross = state.liquidity_gross.checked_sub(liquidity).ok_or(Error::Underflow)?;

        if new_liquidity == 0 {
            positions.remove(position_key);
            if !has_position_at_tick(&positions, tick_lower) {
                bitmap.set_tick_initialized(tick_lower, false)?;
            }
            if !has_position_at_tick(&positions, tick_upper) {
                bitmap.set_tick_initialized(tick_upper, false)?;
            }
        }

        env.storage().instance().set(&symbol_short!("state"), &state);
        env.storage().instance().set(&symbol_short!("bitmap"), &bitmap);
        env.storage().instance().set(&symbol_short!("positions"), &positions);
        env.storage().instance().set(&symbol_short!("ticks"), &ticks);

        env.events().publish(
            (symbol_short!("burn"),),
            (owner, tick_lower, tick_upper, liquidity, amount_0, amount_1),
        );

        Ok((amount_0, amount_1))
    }

    pub fn collect_fees(
        env: Env,
        owner: Address,
        tick_lower: i32,
        tick_upper: i32,
    ) -> Result<(u128, u128), Error> {
        let mut positions: Map<PositionKey, Position> = env.storage().instance().get(&symbol_short!("positions")).ok_or(Error::NotInitialized)?;
        let state: PoolState = env.storage().instance().get(&symbol_short!("state")).ok_or(Error::NotInitialized)?;

        owner.require_auth();

        let position_key = PositionKey { owner: owner.clone(), tick_lower, tick_upper };
        let mut position = positions.get(position_key.clone()).ok_or(Error::PositionNotFound)?;

        let uncollected_0 = position.tokens_owed_0;
        let uncollected_1 = position.tokens_owed_1;

        position.tokens_owed_0 = 0;
        position.tokens_owed_1 = 0;

        positions.set(position_key.clone(), position);

        env.storage().instance().set(&symbol_short!("positions"), &positions);

        env.events().publish(
            (symbol_short!("collect"),),
            (owner, tick_lower, tick_upper, uncollected_0, uncollected_1),
        );

        Ok((uncollected_0, uncollected_1))
    }

    pub fn swap(
        env: Env,
        trader: Address,
        token_in: Address,
        amount_in: u128,
        amount_out_min: u128,
        sqrt_price_limit_x96: u128,
    ) -> Result<SwapResult, Error> {
        let config: PoolConfig = env.storage().instance().get(&symbol_short!("config")).ok_or(Error::NotInitialized)?;
        let mut state: PoolState = env.storage().instance().get(&symbol_short!("state")).ok_or(Error::NotInitialized)?;
        let bitmap: TickBitmap = env.storage().instance().get(&symbol_short!("bitmap")).ok_or(Error::NotInitialized)?;
        let mut ticks: Map<i32, TickInfo> = env.storage().instance().get(&symbol_short!("ticks")).ok_or(Error::NotInitialized)?;

        trader.require_auth();

        if amount_in == 0 {
            return Err(Error::InvalidAmount);
        }

        let token_a = config.token_a.clone();
        let zero_for_one = token_in == token_a;

        if sqrt_price_limit_x96 == 0 {
            return Err(Error::InvalidSqrtPrice);
        }

        if zero_for_one && sqrt_price_limit_x96 >= state.sqrt_price_x96 {
            return Err(Error::PriceLimitExceeded);
        }
        if !zero_for_one && sqrt_price_limit_x96 <= state.sqrt_price_x96 {
            return Err(Error::PriceLimitExceeded);
        }

        let fee_bps = config.fee_bps as u128;

        let result = swap_math::swap(
            &mut state,
            &bitmap,
            &mut ticks,
            zero_for_one,
            amount_in,
            amount_out_min,
            sqrt_price_limit_x96,
            fee_bps,
        )?;

        env.storage().instance().set(&symbol_short!("state"), &state);
        env.storage().instance().set(&symbol_short!("ticks"), &ticks);

        env.events().publish(
            (symbol_short!("swap"),),
            (trader, token_in, amount_in, result.amount_0, result.amount_1, result.fee_amount),
        );

        Ok(result)
    }

    pub fn get_position(
        env: Env,
        owner: Address,
        tick_lower: i32,
        tick_upper: i32,
    ) -> Result<PositionInfo, Error> {
        let positions: Map<PositionKey, Position> = env.storage().instance().get(&symbol_short!("positions")).ok_or(Error::NotInitialized)?;
        let state: PoolState = env.storage().instance().get(&symbol_short!("state")).ok_or(Error::NotInitialized)?;

        let position_key = PositionKey { owner, tick_lower, tick_upper };
        let position = positions.get(position_key).ok_or(Error::PositionNotFound)?;

        let fees = calculate_uncollected_fees(
            &position,
            tick_lower,
            tick_upper,
            state.fee_growth_global_0_x128,
            state.fee_growth_global_1_x128,
            &env,
        )?;

        Ok(PositionInfo {
            liquidity: position.liquidity,
            fee_growth_inside_0_last_x128: position.fee_growth_inside_0_last_x128,
            fee_growth_inside_1_last_x128: position.fee_growth_inside_1_last_x128,
            tokens_owed_0: position.tokens_owed_0,
            tokens_owed_1: position.tokens_owed_1,
            uncollected_fees_0: fees.0,
            uncollected_fees_1: fees.1,
        })
    }

    pub fn get_pool_state(env: Env) -> Result<PoolState, Error> {
        env.storage().instance().get(&symbol_short!("state")).ok_or(Error::NotInitialized)
    }

    pub fn get_config(env: Env) -> Result<PoolConfig, Error> {
        env.storage().instance().get(&symbol_short!("config")).ok_or(Error::NotInitialized)
    }

    pub fn get_tick_info(env: Env, tick: i32) -> Result<TickInfo, Error> {
        let ticks: Map<i32, TickInfo> = env.storage().instance().get(&symbol_short!("ticks")).ok_or(Error::NotInitialized)?;
        ticks.get(tick).ok_or(Error::TickNotInitialized)
    }

    pub fn set_protocol_fee(
        env: Env,
        admin: Address,
        protocol_fee_bps: u128,
    ) -> Result<(), Error> {
        let config: PoolConfig = env.storage().instance().get(&symbol_short!("config")).ok_or(Error::NotInitialized)?;
        
        admin.require_auth();
        if admin != config.admin {
            return Err(Error::Unauthorized);
        }

        if protocol_fee_bps > MAX_PROTOCOL_FEE {
            return Err(Error::InvalidFee);
        }

        env.storage().instance().set(&symbol_short!("proto_fee"), &protocol_fee_bps);

        Ok(())
    }
}

fn update_tick(
    ticks: &mut Map<i32, TickInfo>,
    tick: i32,
    liquidity_delta: u128,
    is_lower: bool,
    fee_growth_global_0_x128: u128,
    fee_growth_global_1_x128: u128,
) -> Result<(), Error> {
    let mut tick_info = ticks.get(tick).unwrap_or(TickInfo {
        liquidity_gross: 0,
        liquidity_net: 0,
        fee_growth_outside_0_x128: 0,
        fee_growth_outside_1_x128: 0,
        tick_cumulative_outside: 0,
        seconds_per_liquidity_outside: 0,
        seconds_outside: 0,
        initialized: false,
    });

    tick_info.initialized = true;

    if is_lower {
        tick_info.liquidity_net = tick_info.liquidity_net
            .checked_add(liquidity_delta as i128)
            .ok_or(Error::Overflow)?;
    } else {
        tick_info.liquidity_net = tick_info.liquidity_net
            .checked_sub(liquidity_delta as i128)
            .ok_or(Error::Underflow)?;
    }

    tick_info.liquidity_gross = tick_info.liquidity_gross
        .checked_add(liquidity_delta)
        .ok_or(Error::Overflow)?;

    if tick_info.liquidity_gross == liquidity_delta {
        tick_info.fee_growth_outside_0_x128 = fee_growth_global_0_x128;
        tick_info.fee_growth_outside_1_x128 = fee_growth_global_1_x128;
    }

    ticks.set(tick, tick_info);
    Ok(())
}

fn has_position_at_tick(positions: &Map<PositionKey, Position>, tick: i32) -> bool {
    for key in positions.keys() {
        if key.tick_lower == tick || key.tick_upper == tick {
            if positions.get(key.clone()).unwrap().liquidity > 0 {
                return true;
            }
        }
    }
    false
}

fn calculate_uncollected_fees(
    position: &Position,
    tick_lower: i32,
    tick_upper: i32,
    fee_growth_global_0_x128: u128,
    fee_growth_global_1_x128: u128,
    env: &Env,
) -> Result<(u128, u128), Error> {
    let ticks: Map<i32, TickInfo> = env.storage().instance().get(&symbol_short!("ticks")).ok_or(Error::NotInitialized)?;

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