use anyhow;
use std::collections:: BTreeMap;
use fixed::types::{U64F64, I64F64};
use std::num::Wrapping;
use ethnum::U256;

#[derive(Debug, Clone)]
pub struct Tick {
    // id: UID,
    pub liquidity_gross: u128,
    pub liquidity_net: i128,
    pub fee_growth_outside_a: u128,
    pub fee_growth_outside_b: u128,
    // reward_growths_outside: Vec<u128>,
    pub initialized: bool,
}

#[derive(Debug, Clone)]
pub struct Pool {
    // id: UID,
    // coin_a: Balance<CoinTypeA>,
    // coin_b: Balance<CoinTypeB>,
    pub protocol_fees_a: u64,
    pub protocol_fees_b: u64,
    pub sqrt_price: u128,
    pub tick_current_index: i32,
    pub tick_spacing: u32,
    pub max_liquidity_per_tick: u128,
    pub fee: u32,
    pub fee_protocol: u32,
    pub unlocked: bool,
    pub fee_growth_global_a: u128,
    pub fee_growth_global_b: u128,
    pub liquidity: u128,
    pub ticks: BTreeMap<i32, Tick> , // new
    pub tick_map: BTreeMap<i32, U256> // Move: Table<i32, U256>,
    // deploy_time_ms: u64,
    // reward_infos: vector<PoolRewardInfo>,
    // reward_last_updated_time_ms: u64,
}

#[derive(Clone, Debug)]
pub struct ComputeSwapState {
    pub amount_a: u128,
    pub amount_b: u128, 
    pub amount_specified_remaining: u128,
    pub amount_calculated: u128,
    pub sqrt_price: u128,
    pub tick_current_index: i32,
    pub fee_growth_global: u128,
    pub protocol_fee: u128,
    pub liquidity: u128,
    pub fee_amount: u128,
}

use std::collections::HashSet;
pub fn count_init_ticks_in_tick_map(pool: &mut Pool) -> HashSet<i32> {
    let mut tick_count = 0;
    let tick_spacing_i32 = pool.tick_spacing as i32;
    let mut tick_index_set = HashSet::new();

    for (word_pos, word) in pool.tick_map.iter() {
        for bit_pos in 0..256 {
            if (*word >> bit_pos) & U256::from(1_u8) == U256::from(1_u8) {
                tick_count += 1;

                let tick_index = (((*word_pos) << 8) | bit_pos) * tick_spacing_i32;
                tick_index_set.insert(tick_index);

                // println!("TICK MAP: tick_index: {}", tick_index);

                println!("TICK_MAP: ({}, {})", word_pos, bit_pos);
            }
        }
    }

    println!("TICKS IN TICK MAP: {}", tick_count);
    
    tick_index_set
}

pub fn count_init_tick_in_ticks(pool: &mut Pool) -> HashSet<i32> {
    let mut tick_count = 0;
    let tick_spacing_i32 = pool.tick_spacing as i32;
    let mut tick_index_set = HashSet::new();

    for (tick_index, tick) in pool.ticks.iter() {
        if tick.initialized {
            tick_count += 1;

            tick_index_set.insert(*tick_index);

            // println!("TICKS: tick_index: {}", tick_index);

            let compressed = tick_index / tick_spacing_i32;
            println!("TICKS: {:?}", position_tick(compressed));
            // I think theres a fuckign problem with position_tick;
        }
    } 

    println!("TICKS IN TICKS: {}", tick_count);

    tick_index_set
}

// Seems that this does most of the work in swap()
pub fn compute_swap_result(
    pool: &mut Pool,
    a_to_b: bool,
    amount_specified: u128,
    amount_specified_is_input: bool,
    sqrt_price_limit: u128,
    simulating: bool
) -> ComputeSwapState {

    // /// TESTING
    // let tick_map_tick_index_set = count_init_ticks_in_tick_map(pool);
    // let ticks_tick_index_set = count_init_tick_in_ticks(pool);

    // println!("SETS ARE SAME: {}", tick_map_tick_index_set.difference(&ticks_tick_index_set).cloned().collect::<Vec<i32>>().len() == 0);

    // // INDUCE FAILURE
    // assert!(1 == 0);

    assert!(pool.unlocked);
    assert!(amount_specified != 0);
    assert!(
        if a_to_b {
            sqrt_price_limit < pool.sqrt_price && sqrt_price_limit > math_tick::MIN_SQRT_PRICE_X64
        } else {
            sqrt_price_limit > pool.sqrt_price && sqrt_price_limit < math_tick::MAX_SQRT_PRICE_X64
        }
    );

    // TODO: When we're doing backruns & whatnot. L2 orderbook style client side delta application.
    // let next_pool_reward_infos = vec![]; // next_pool_reward_infos()

    let tick_current_index = pool.tick_current_index as i32;
    let sqrt_price = pool.sqrt_price;
    let amount_specified_remaining = amount_specified;
    
    let mut compute_swap_state = ComputeSwapState {
        amount_a: 0,
        amount_b: 0, 
        amount_specified_remaining,
        amount_calculated: 0,
        sqrt_price,
        tick_current_index,
        fee_growth_global: 0,
        protocol_fee: 0,
        liquidity: pool.liquidity,
        fee_amount: 0,
    };

    while compute_swap_state.amount_specified_remaining > 0 && compute_swap_state.sqrt_price != sqrt_price_limit {

        let sqrt_price_start = compute_swap_state.sqrt_price;
        let (mut tick_next, initialized) = next_initialized_tick_within_one_word(
            pool,
            compute_swap_state.tick_current_index,
            a_to_b
        );

        if tick_next < math_tick::MIN_TICK_INDEX {
            tick_next = math_tick::MIN_TICK_INDEX;
        } else if tick_next > math_tick::MAX_TICK_INDEX {
            tick_next = math_tick::MAX_TICK_INDEX;
        }

        let sqrt_price_next = math_tick::sqrt_price_from_tick_index(tick_next);

        let amount_in;
        let amount_out;
        let mut fee_amount;

        println!("current tick: {}, target tick: {}, target_tick_initialized?: {}", compute_swap_state.tick_current_index, tick_next, initialized);

        (compute_swap_state.sqrt_price, amount_in, amount_out, fee_amount) =
            math_swap::compute_swap(
                compute_swap_state.sqrt_price,
                if {
                    if a_to_b {
                        sqrt_price_next < sqrt_price_limit
                    } else {
                        sqrt_price_next > sqrt_price_limit
                    }
                } {
                    sqrt_price_limit
                } else {
                    sqrt_price_next
                },
                compute_swap_state.liquidity,
                compute_swap_state.amount_specified_remaining,
                amount_specified_is_input,
                pool.fee
            );

        println!("amount_in = {}, amount_out = {}", amount_in, amount_out);
        
        if amount_specified_is_input {
            // println!("amount_specified_is_input == true: amount calc = {}, amount_in = {}, amount_out = {}", compute_swap_state.amount_calculated, amount_in, amount_out);
            compute_swap_state.amount_specified_remaining -= amount_in + fee_amount;
            compute_swap_state.amount_calculated += amount_out;
        } else {
            // println!("amount_specified_is_input == false: amount calc = {}, amount_out = {}", compute_swap_state.amount_calculated, amount_out);
            compute_swap_state.amount_specified_remaining -= amount_out;
            compute_swap_state.amount_calculated += amount_in + fee_amount;
        }

        compute_swap_state.fee_amount += fee_amount;

        if pool.fee_protocol > 0 {
            let delta = (fee_amount * pool.fee_protocol as u128) / 1000000;
            fee_amount -= delta;
            compute_swap_state.protocol_fee = (Wrapping(compute_swap_state.protocol_fee as u128) + Wrapping(delta)).0;
        } 

        if compute_swap_state.liquidity > 0 {
            let temp = full_math_u128::mul_div_floor(
                fee_amount,
                math_liquidity::Q64,
                compute_swap_state.liquidity
            );

            compute_swap_state.fee_growth_global = (Wrapping(compute_swap_state.fee_growth_global) + Wrapping(temp)).0;
        }

        if compute_swap_state.sqrt_price == sqrt_price_next {
            if initialized {
                let mut liquidity_net = cross_tick(
                    pool,
                    tick_next,
                    if a_to_b {
                        compute_swap_state.fee_growth_global
                    } else {
                        pool.fee_growth_global_a
                    },
                    if a_to_b {
                        pool.fee_growth_global_b
                    } else {
                        compute_swap_state.fee_growth_global
                    },
                    // &next_pool_reward_infos,
                    simulating
                );

                if a_to_b {
                    liquidity_net = -liquidity_net;
                }

                // println!("liquidity: {}", compute_swap_state.liquidity);
                // println!("liquidity_net: {}", liquidity_net);

                compute_swap_state.liquidity = math_liquidity::add_delta(
                    compute_swap_state.liquidity,
                    liquidity_net
                );
            }

            compute_swap_state.tick_current_index = if a_to_b {
                tick_next - 1
            } else {
                tick_next
            }
        } else if compute_swap_state.sqrt_price != sqrt_price_start {
            compute_swap_state.tick_current_index = math_tick::tick_index_from_sqrt_price(
                compute_swap_state.sqrt_price
            );
        }

        println!("end of loop 1: compute_swap_state.amount_specified_remaining = {}", compute_swap_state.amount_specified_remaining);
    }

    // TODO: When we're doing backruns & whatnot. L2 orderbook style client side delta application.
    if !simulating {
        // lines 413 - 513 in disassembeld function
        if compute_swap_state.tick_current_index != pool.tick_current_index {
            pool.sqrt_price = compute_swap_state.sqrt_price;
            pool.tick_current_index = compute_swap_state.tick_current_index;
        } else {
            pool.sqrt_price = compute_swap_state.sqrt_price;
        }

        if pool.liquidity != compute_swap_state.liquidity {
            pool.liquidity = compute_swap_state.liquidity;
        }

        if a_to_b {
            pool.fee_growth_global_a = compute_swap_state.fee_growth_global;
            if compute_swap_state.protocol_fee > 0 {
                pool.protocol_fees_a += compute_swap_state.protocol_fee as u64;
            }
        } else {
            pool.fee_growth_global_b = compute_swap_state.fee_growth_global;
            if compute_swap_state.protocol_fee > 0 {
                pool.protocol_fees_b += compute_swap_state.protocol_fee as u64;
            }
        }
    }

    (compute_swap_state.amount_a, compute_swap_state.amount_b) = if a_to_b == amount_specified_is_input {
        (amount_specified - compute_swap_state.amount_specified_remaining, compute_swap_state.amount_calculated)
    } else {
        (compute_swap_state.amount_calculated, amount_specified - compute_swap_state.amount_specified_remaining)
    };

    compute_swap_state
}

pub fn next_initialized_tick_within_one_word(
    pool: &mut Pool,
    tick: i32,
    lte: bool,
) -> (i32, bool) {

    // let one: U256 = U256::from(1_u8);

    let tick_spacing_i32 = pool.tick_spacing as i32;

    let mut compressed: i32 = tick / tick_spacing_i32;

    // Sus this out
    if tick < 0 && tick % tick_spacing_i32 != 0 {
        compressed -= 1;
    }

    if lte {
        let (word_pos, bit_pos) = position_tick(compressed);
        let word = pool.tick_map.entry(word_pos).or_insert(U256::from(0_u8));

        // only includes at and to the right of bit_pos
        let mask = (U256::from(1_u8) << bit_pos) - 1 + (U256::from(1_u8) << bit_pos);
        let masked = *word & mask;

        let initialized = masked != U256::from(0_u8);

        let next = if initialized {
            (compressed - (bit_pos - math_bit::most_significant_bit(masked)) as i32) * tick_spacing_i32
        } else {
            (compressed - bit_pos as i32) * tick_spacing_i32
        };

        (next, initialized)
    } else {
        let (word_pos, bit_pos) = position_tick(compressed + 1);
        let word = pool.tick_map.entry(word_pos).or_insert(U256::from(0_u8));

        // only includes to the left 
        let mask = !((U256::from(1_u8) << bit_pos) - 1);
        let masked = *word & mask;

        let initialized = masked != U256::from(0_u8);

        let next = if initialized {
            (compressed + 1 + (math_bit::least_significant_bit(masked) - bit_pos) as i32) * tick_spacing_i32
        } else {
            (compressed + 1 + (u8::MAX - bit_pos) as i32) * tick_spacing_i32
        };

        (next, initialized)
    }


}

pub fn position_tick(
    tick: i32
) -> (i32, u8) {
    let word_pos = tick >> 8;   // Arithmetic right shift (on purpose!)
    let bit_pos = mod_euclidean(tick, 256) as u8;

    (word_pos, bit_pos)
}

pub fn mod_euclidean(v: i32, n: i32) -> i32 {
    let r = v % n;
    if r < 0 {
        r + n
    } else {
        r
    }
}

pub fn cross_tick(
    pool: &mut Pool,
    tick_next_index: i32,
    fee_growth_global_a: u128,
    fee_growth_global_b: u128,
    // next_pool_reward_infos: &[u128],
    simulating: bool,  // determines whether we 
) -> i128 {

    let tick_next = pool
        .ticks
        .entry(tick_next_index)
        .or_insert(
            Tick {
                // id: UID,
                liquidity_gross: 0,
                liquidity_net: 0,
                fee_growth_outside_a: 0,
                fee_growth_outside_b: 0,
                // reward_growths_outside: vec![],
                initialized: false,
            }
        );

    if !simulating {
        tick_next.fee_growth_outside_a = (Wrapping(fee_growth_global_a) - Wrapping(tick_next.fee_growth_outside_a)).0;
        tick_next.fee_growth_outside_b = (Wrapping(fee_growth_global_b) - Wrapping(tick_next.fee_growth_outside_b)).0;

        // for i in 0..next_pool_reward_infos.len() {
        //     tick_next.reward_growths_outside[i] = (Wrapping(next_pool_reward_infos[i]) - Wrapping(tick_next.reward_growths_outside[i])).0;
        // }
    }

    // We should generally never cross into an uninitialized tick
    assert!(tick_next.initialized, "crossing uninitialized tick ({}): {:#?}", tick_next_index, tick_next);

    tick_next.liquidity_net
}

pub fn deploy_pool(
    fee: u32,
    tick_spacing: u32,
    sqrt_price: u128,
    fee_protocol: u32
) -> Pool {
    let tick_current_index = math_tick::tick_index_from_sqrt_price(sqrt_price);
    let max_liquidity_per_tick = math_tick::max_liquidity_per_tick(tick_spacing);

    Pool {
        protocol_fees_a: 0,
        protocol_fees_b: 0,
        sqrt_price,
        tick_current_index,
        tick_spacing,
        max_liquidity_per_tick,
        fee,
        fee_protocol,
        unlocked: true,
        fee_growth_global_a: 0,
        fee_growth_global_b: 0,
        liquidity: 0,
        ticks: BTreeMap::new(),
        tick_map: BTreeMap::new()
    }
}

pub fn mint(
    pool: &mut Pool,
    // owner: SuiAddress
    tick_lower: i32,
    tick_upper: i32,
    amount: u128
) -> (u64, u64) {
    assert!(pool.unlocked);
    assert!(amount > 0);

    let (amount_a, amount_b) = modify_position(
        pool,
        tick_lower,
        tick_upper,
        amount as i128 // minting adds so amount is + u128
    );

    // not exactly equivalent to univ3's amount_0 > 0
    assert!(!(amount_a < 0) && !(amount_b < 0));

    (amount_a.abs() as u64, amount_b.abs() as u64)
}

pub fn modify_position(
    pool: &mut Pool,
    // owner: SuiAddress,
    tick_lower: i32,
    tick_upper: i32,
    liquidity_delta: i128
) -> (i128, i128) {
    update_position(
        pool,
        tick_lower,
        tick_upper,
        liquidity_delta
    );
    let mut amount_a = 0;
    let mut amount_b = 0;

    if liquidity_delta != 0 {
        if pool.tick_current_index < tick_lower {
            amount_a = math_sqrt_price::get_amount_a_delta(
                math_tick::sqrt_price_from_tick_index(tick_lower), 
                math_tick::sqrt_price_from_tick_index(tick_upper), 
                liquidity_delta
            );

        } else if pool.tick_current_index < tick_upper {
            amount_a = math_sqrt_price::get_amount_a_delta(
                pool.sqrt_price, 
                math_tick::sqrt_price_from_tick_index(tick_upper), 
                liquidity_delta
            );

            amount_b = math_sqrt_price::get_amount_b_delta(
                math_tick::sqrt_price_from_tick_index(tick_lower), 
                pool.sqrt_price, 
                liquidity_delta
            );

            pool.liquidity = math_liquidity::add_delta(pool.liquidity, liquidity_delta);
        } else {
            amount_b = math_sqrt_price::get_amount_a_delta(
                math_tick::sqrt_price_from_tick_index(tick_lower), 
                math_tick::sqrt_price_from_tick_index(tick_upper), 
                liquidity_delta
            );
        }
    }

    (amount_a, amount_b)
}

pub fn update_position(
    pool: &mut Pool,
    // owner: SuiAddress,
    tick_lower: i32,
    tick_upper: i32,
    liquidity_delta: i128,
) {
    let tick_current_index = pool.tick_current_index;
    let mut flipped_lower = false;
    let mut flipped_upper = false;

    if liquidity_delta != 0 {
        flipped_lower = update_tick(
            pool, 
            tick_lower, 
            tick_current_index, 
            liquidity_delta, 
            false
        );

        flipped_upper = update_tick(
            pool, 
            tick_upper, 
            tick_current_index, 
            liquidity_delta, 
            true
        );

        if flipped_lower {
            flip_tick(pool, tick_lower);
        }

        if flipped_upper {
            flip_tick(pool, tick_upper);
        }
    }

    // TODO: fee_growth and positions updates
    // ignoring fee growth and liquidity mining stuff for now
    // don't update positions (we're only doing tick stuff for now)

    if liquidity_delta < 0 {
        if flipped_lower {
            clear_tick(pool, tick_lower);
        }
        if flipped_upper {
            clear_tick(pool, tick_upper);
        }
    }

}

pub fn update_tick(
    pool: &mut Pool,
    tick_index: i32,
    tick_current_index: i32,
    liquidity_delta: i128,
    upper: bool,
    // reward growths
) -> bool {

    let fee_growth_global_a = pool.fee_growth_global_a;
    let fee_growth_global_b = pool.fee_growth_global_b;
    let max_liquidity_per_tick = pool.max_liquidity_per_tick;

    let tick = pool
        .ticks
        .entry(tick_index)
        .or_insert(
            Tick {
                liquidity_gross: 0,
                liquidity_net: 0,
                fee_growth_outside_a: 0,
                fee_growth_outside_b: 0,
                // reward_growths_outside: vec![],
                initialized: false,
            }
        );
    
    let liquidity_gross_before = tick.liquidity_gross;
    let liquidity_gross_after = math_liquidity::add_delta(liquidity_gross_before, liquidity_delta);

    assert!(liquidity_gross_after <= max_liquidity_per_tick);

    let flipped = (liquidity_gross_after == 0) != (liquidity_gross_before == 0);

    if liquidity_gross_before == 0 {
        if tick_index < tick_current_index {
            tick.fee_growth_outside_a = fee_growth_global_a;
            tick.fee_growth_outside_b = fee_growth_global_b;
            // omitting liquidity mining information
        }
        tick.initialized = true;
    }

    tick.liquidity_gross = liquidity_gross_after;

    tick.liquidity_net = if upper {
        tick.liquidity_net - liquidity_delta
    } else {
        tick.liquidity_net + liquidity_delta
    };

    flipped
}

pub fn clear_tick(
    pool: &mut Pool,
    tick_index: i32
) {
    // Prevent redundant insertions
    pool
        .ticks
        .entry(tick_index)
        .and_modify(|tick| {
            tick.liquidity_gross = 0;
            tick.liquidity_net = 0;
            tick.fee_growth_outside_a = 0;
            tick.fee_growth_outside_b = 0;
            tick.initialized = false;
        });
}

pub fn flip_tick(
    pool: &mut Pool,
    tick_index: i32
) {
    assert!(tick_index % pool.tick_spacing as i32 == 0, "tick_index is not a multiple of tick spacing");

    let (word_pos, bit_pos) = position_tick(tick_index / pool.tick_spacing as i32);
    // println!("shl {} bits", bit_pos);

    let mask = U256::from(1_u8) << bit_pos;
    let word = pool
        .tick_map
        .entry(word_pos)
        .or_insert(U256::from(0_u8));

    *word = *word ^ mask;
}

pub fn check_ticks(
    tick_lower: i32,
    tick_upper: i32
) {
    assert!(tick_lower < tick_upper, "tick_lower > tick upper");
    assert!(tick_lower >= math_tick::MIN_TICK_INDEX, "tick_lower < MIN_TICK_INDEX");
    assert!(tick_upper <= math_tick::MAX_TICK_INDEX, "tick_lower > MAX_TICK_INDEX");
}

#[cfg(test)]
mod tests {
    // use super::{compute_swap_result, deploy_pool, mint, math_sqrt_price};
    use fixed::types::U64F64;
    use super::*;

    fn setup_test_case() -> Pool {
        let fee = 0;
        let sqrt_price_x96 = 5602277097478614198912276234240;
        let sqrt_price_x64 = sqrt_price_x96 >> 32;

        println!("initial sqrt_price: {}", U64F64::from_bits(sqrt_price_x64));
        println!("initial price: {}", U64F64::from_bits(sqrt_price_x64) * U64F64::from_bits(sqrt_price_x64));
        println!("tick from initial sqrt_price: {}", math_tick::tick_index_from_sqrt_price(sqrt_price_x64));

        let tick_spacing = 1;
        let fee_protocol= 0;

        let mut pool = deploy_pool(
            fee, 
            tick_spacing, 
            sqrt_price_x64, 
            fee_protocol
        );

        println!("initial tick: {}", pool.tick_current_index);

        let tick_lower = 84222;
        let tick_upper = 86129;
        let amount = 1517882343751509868544;

        mint(
            &mut pool, 
            tick_lower, 
            tick_upper, 
            amount
        );

        println!("deployed pool: {:#?}", pool);

        println!("initial liquidity: {}", pool.liquidity);

        pool
    }

    #[test]
    fn test_compute_swap_buy_eth() {

        println!("hello!");

        let mut pool = setup_test_case();
        println!("nooo!");

        let amount_specified = 42_000_000_000_000_000_000; // 42 UDSC

        let swap_result = compute_swap_result(
            &mut pool,
            false, 
            amount_specified,
            true,
            math_tick::MAX_SQRT_PRICE_X64 - 1,
            false
        );

        println!(
            "amount_a: {}\namount_b: {}",
            swap_result.amount_a,
            swap_result.amount_b
        );

        println!(
            "amount_calculated: {}",
            swap_result.amount_calculated
        );

        println!("post swap tick: {}", pool.tick_current_index);
        println!("post swap sqrt_price: {}", pool.sqrt_price);
        println!("post swap price: {}", U64F64::from_bits(pool.sqrt_price) * U64F64::from_bits(pool.sqrt_price));
        println!("post swap liquidity: {}", pool.liquidity);

        println!("expected amount_a: {}", 8_396_714_242_162_445);
        println!("expected amount_b: {}", 42_000_000_000_000_000_000);
        println!("expected sqrt_price: {}", 5604469350942327889444743441197_u128 >> 32);
        println!("expected tick: {}", math_tick::tick_index_from_sqrt_price(5604469350942327889444743441197_u128 >> 32));
        println!("expected price: {}", U64F64::from_bits(5604469350942327889444743441197_u128 >> 32) * U64F64::from_bits(5604469350942327889444743441197_u128 >> 32));
        println!("expected liquidity: {}", 1517882343751509868544);
    }

    #[test]
    fn test_compute_swap_buy_usdc() {

        println!("hello!");

        let mut pool = setup_test_case();
        println!("nooo!");

        let amount_specified = 13_370_000_000_000; // 42 UDSC

        let swap_result = compute_swap_result(
            &mut pool,
            true, 
            amount_specified,
            true,
            math_tick::MIN_SQRT_PRICE_X64 + 1,
            false
        );

        println!(
            "amount_a: {}\namount_b: {}",
            swap_result.amount_a,
            swap_result.amount_b
        );

        println!(
            "amount_calculated: {}",
            swap_result.amount_calculated
        );

        println!("post swap tick: {}", pool.tick_current_index);
        println!("post swap sqrt_price: {}", pool.sqrt_price);
        println!("post swap price: {}", U64F64::from_bits(pool.sqrt_price) * U64F64::from_bits(pool.sqrt_price));
        println!("post swap liquidity: {}", pool.liquidity);

        println!("expected amount_a: {}", 13_370_000_000_000_000);
        println!("expected amount_b: {}", 66_808_388_890_199_406_685);
        println!("expected sqrt_price: {}", 5598789932670288701514545755210_u128 >> 32);
        println!("expected tick: {}", math_tick::tick_index_from_sqrt_price(5598789932670288701514545755210_u128 >> 32));
        println!("expected price: {}", U64F64::from_bits(5598789932670288701514545755210_u128 >> 32) * U64F64::from_bits(5604469350942327889444743441197_u128 >> 32));
        println!("expected liquidity: {}", 1517882343751509868544);
    }

    fn setup_test_case_for_next_init_a_to_b() -> Pool {
        let fee = 0;
        // bit_pos initial is 0
        let sqrt_price_x96 = 4700277097478614198912276234240;
        let sqrt_price_x64 = sqrt_price_x96 >> 32;

        println!("initial sqrt_price: {}", U64F64::from_bits(sqrt_price_x64));
        println!("initial price: {}", U64F64::from_bits(sqrt_price_x64) * U64F64::from_bits(sqrt_price_x64));
        println!("tick from initial sqrt_price: {}", math_tick::tick_index_from_sqrt_price(sqrt_price_x64));

        let tick_spacing = 1;
        let fee_protocol= 0;

        let mut pool = deploy_pool(
            fee, 
            tick_spacing, 
            sqrt_price_x64, 
            fee_protocol
        );

        println!("initial tick: {}", pool.tick_current_index);

        let tick_lower = 84222;
        let tick_upper = 86129;
        let amount = 1517882343751509868544;

        println!("deployed pool");

        mint(
            &mut pool, 
            tick_lower, 
            tick_upper, 
            amount
        );

        println!("initial liquidity: {}", pool.liquidity);

        pool
    }

    fn setup_test_case_for_next_init_b_to_a() -> Pool {
        let fee = 0;
        // bit_pos initial is 255
        let sqrt_price_x96 = 4760557097478614198912276234240;
        let sqrt_price_x64 = sqrt_price_x96 >> 32;

        println!("initial sqrt_price: {}", U64F64::from_bits(sqrt_price_x64));
        println!("initial price: {}", U64F64::from_bits(sqrt_price_x64) * U64F64::from_bits(sqrt_price_x64));
        println!("tick from initial sqrt_price: {}", math_tick::tick_index_from_sqrt_price(sqrt_price_x64));

        let tick_spacing = 1;
        let fee_protocol= 0;

        let mut pool = deploy_pool(
            fee, 
            tick_spacing, 
            sqrt_price_x64, 
            fee_protocol
        );

        println!("initial tick: {}", pool.tick_current_index);

        let tick_lower = 84222;
        let tick_upper = 86129;
        let amount = 1517882343751509868544;

        println!("deployed pool");

        mint(
            &mut pool, 
            tick_lower, 
            tick_upper, 
            amount
        );

        println!("initial liquidity: {}", pool.liquidity);

        pool
    }

    // #[test]
    // fn test_next_initialized_tick() {

    //     // Setup
    //     // let tick_lower = 84222;
    //     // let tick_upper = 86129;

    //     // From bit_pos = 255
    //     for  i in 1..256 {
    //         let mut pool = setup_test_case_for_next_init_b_to_a();
    //         // Higher: b to a
    //         let tick_current = pool.tick_current_index;
    //         let tick_lower = pool.tick_current_index - i;
    //         let tick_upper = 86220;
    //         let amount = 1517882343751509868544u128;
    //         println!("- {}", i);
    //         println!("bit_pos current: {}", position_tick(tick_current).1);
    //         // println!("bit_pos expected next: {}", position_tick(tick_lower).1);

    //         // mint(&mut pool, tick_current -166, tick_upper, amount);
    //         mint(&mut pool, tick_lower, tick_upper, amount);

    //         println!("expected next: {}", tick_lower);
    //         println!("{:#?}", next_initialized_tick_within_one_word(&mut pool, tick_current, true));

    //         assert!((tick_lower, true) == next_initialized_tick_within_one_word(&mut pool, tick_current, true));
    //     }

    //     // From bit_pos = 0
    //     for  i in 1..256 {
    //         let mut pool = setup_test_case_for_next_init_a_to_b();
    //         // Higher: b to a
    //         let tick_current = pool.tick_current_index;
    //         let tick_lower = pool.tick_current_index + i;
    //         let tick_upper = 86220;
    //         let amount = 1517882343751509868544u128;
    //         println!("+ {}", i);
    //         println!("bit_pos current: {}", position_tick(tick_current).1);
    //         println!("bit_pos expected next: {}", position_tick(tick_lower).1);

    //         // mint(&mut pool, tick_current -166, tick_upper, amount);
    //         mint(&mut pool, tick_lower, tick_upper, amount);

    //         println!("expected next: {}", tick_lower);
    //         println!("{:#?}", next_initialized_tick_within_one_word(&mut pool, tick_current, false));

    //         assert!((tick_lower, true) == next_initialized_tick_within_one_word(&mut pool, tick_current, false));
    //     }

    // }
}

mod math_bit {
    use ethnum::U256;
    // use std::ops::Shr;

    pub fn most_significant_bit(
        mut x: U256
    ) -> u8 {
        assert!(x > 0, "x must be greater than 0.");
        let mut r = 0;

        if x >= U256::from_str_hex("0x100000000000000000000000000000000").unwrap() {
            x = x >> 128;
            r = r + 128;
        }

        if x >= U256::from_str_hex("0x10000000000000000").unwrap() {
            x = x >> 64;
            r = r + 64;
        }

        if x >= U256::from_str_hex("0x100000000").unwrap() {
            x = x >> 32;
            r = r + 32;
        }

        if x >= U256::from_str_hex("0x10000").unwrap() {
            x = x >> 16;
            r = r + 16;
        };

        if x >= U256::from_str_hex("0x100").unwrap() {
            x = x >> 8;
            r = r + 8;
        };

        if x >= U256::from_str_hex("0x10").unwrap() {
            x = x >> 4;
            r = r + 4;
        };

        if x >= U256::from_str_hex("0x4").unwrap() {
            x = x >> 2;
            r = r + 2;
        };

        if x >= U256::from_str_hex("0x2").unwrap() {
            r = r + 1;
        }

        r
    }

    pub fn least_significant_bit(
        mut x: U256
    ) -> u8 {
        assert!(x > 0, "x must be greater than 0.");

        let mut r: u8 = 255;

        if x & U256::from_str_hex("0xffffffffffffffffffffffffffffffff").unwrap() > 0 {
            r = r - 128;
        } else {
            x = x >> 128;
        };

        if x & U256::from_str_hex("0xffffffffffffffff").unwrap() > 0 {
            r = r - 64;
        } else {
            x = x >> 64;
        };

        if x & U256::from_str_hex("0xffffffff").unwrap() > 0 {
            r = r - 32;
        } else {
            x = x >> 32;
        };

        if x & U256::from_str_hex("0xffff").unwrap() > 0 {
            r = r - 16;
        } else {
            x = x >> 16;
        };

        if x & U256::from_str_hex("0xff").unwrap() > 0 {
            r = r - 8;
        } else {
            x = x >> 8;
        };

        if x & U256::from_str_hex("0xf").unwrap() > 0 {
            r = r - 4;
        } else {
            x = x >> 4;
        };

        if x & U256::from_str_hex("0x3").unwrap() > 0 {
            r = r - 2;
        } else {
            x = x >> 2;
        };

        if x & U256::from_str_hex("0x1").unwrap() > 0 {
            r = r - 1;
        }

        r
    }

    // #[cfg(test)]
    // mod tests {
    //     use super::*;

    //     #[test]
    //     fn test_most_significant_bit()
    // }
}

mod math_liquidity {

    pub const Q64: u128 = 0x10000000000000000;

    pub fn add_delta(
        x: u128,
        y: i128
    ) -> u128 {
        let mut z;
        let abs_y = y.abs() as u128;

        // println!("x: {}", x);
        // println!("y: {}", y);

        if y < 0 {
            assert!(x >= abs_y, "add_delta: x < |y|.");
            z = x - abs_y;
        } else {
            z = x + abs_y;
            assert!(z >= x, "add_delta: z < x");
        }

        z
    }
}

mod math_swap {
    use super::{
        full_math_u128,
        math_sqrt_price
    };

    const RESOLUTION: u8 = 64;
    const Q64: u128 = 0x10000000000000000;
    const MAX_U64: u128 = 0xffffffffffffffff;
    const SCALE_FACTOR: u128 = 10000;
    const DECIMAL_PLACES: u8 = 64;

    pub fn compute_swap(
        sqrt_price_current: u128,
        sqrt_price_target: u128,
        liquidity: u128,
        amount_remaining: u128,
        amount_specified_is_input: bool,
        fee_rate: u32,
    ) -> (u128, u128, u128, u128) {
        println!("current price: {}, target price: {}, liquidity: {}", sqrt_price_current, sqrt_price_target, liquidity);

        let a_to_b = sqrt_price_current >= sqrt_price_target;
        let fee_amount;

        let mut amount_fixed_delta = get_amount_fixed_delta(
            sqrt_price_current,
            sqrt_price_target,
            liquidity,
            amount_specified_is_input,
            a_to_b
        );

        let mut amount_calc = amount_remaining;
        if amount_specified_is_input {
            amount_calc = full_math_u128::mul_div_floor(
                amount_remaining,
                (1000000 - fee_rate) as u128,
                1000000
            );
        }

        println!("compute_swap(): amount_calc = {}", amount_calc);
        println!("compute_swap() amount_fixed delta = {}", amount_fixed_delta);

        let next_sqrt_price = if amount_calc >= amount_fixed_delta {
            // println!("compute_swap_step(): branch 1 next_sqrt_price = {}", sqrt_price_target);
            sqrt_price_target
        } else {
            math_sqrt_price::get_next_sqrt_price(
                sqrt_price_current,
                liquidity,
                amount_calc,
                amount_specified_is_input,
                a_to_b
            )
        };

        println!("compute_swap(): next_sqrt_price = {}", next_sqrt_price);

        let is_max_swap = next_sqrt_price == sqrt_price_target;

        let amount_unfixed_delta = get_amount_unfixed_delta(
            sqrt_price_current,
            next_sqrt_price,
            liquidity,
            amount_specified_is_input,
            a_to_b,
        );

        // println!("amount_unfixed_delta = {}", amount_unfixed_delta);

        if !is_max_swap {
            amount_fixed_delta = get_amount_fixed_delta(
                sqrt_price_current,
                next_sqrt_price,
                liquidity,
                amount_specified_is_input,
                a_to_b   
            );
        }

        let (amount_in, mut amount_out) = if amount_specified_is_input {
            (amount_fixed_delta, amount_unfixed_delta)
        } else {
            (amount_unfixed_delta, amount_fixed_delta)
        };

        if !amount_specified_is_input && amount_out > amount_remaining {
            amount_out = amount_remaining;
        }

        if amount_specified_is_input && !is_max_swap {
            fee_amount = amount_remaining - amount_in;
        } else {
            fee_amount = full_math_u128::mul_div_round(
                amount_in,
                fee_rate as u128,
                (1000000 - fee_rate) as u128,
            );
        }

        (next_sqrt_price, amount_in, amount_out, fee_amount)
    }

    pub fn get_amount_fixed_delta(
        sqrt_price_current: u128,
        sqrt_price_target: u128,
        liquidity: u128,
        amount_specified_is_input: bool,
        a_to_b: bool,
    ) -> u128 {
        if a_to_b == amount_specified_is_input {
            math_sqrt_price::get_amount_a_delta_(
                sqrt_price_current,
                sqrt_price_target,
                liquidity,
                amount_specified_is_input
            )
        } else {
            math_sqrt_price::get_amount_b_delta_(
                sqrt_price_current,
                sqrt_price_target,
                liquidity,
                amount_specified_is_input,
            )
        }
    }

    pub fn get_amount_unfixed_delta(
        sqrt_price_current: u128,
        sqrt_price_target: u128,
        liquidity: u128,
        amount_specified_is_input: bool,
        a_to_b: bool,
    ) -> u128 {
        if a_to_b == amount_specified_is_input {
            math_sqrt_price::get_amount_b_delta_(
                sqrt_price_current,
                sqrt_price_target,
                liquidity,
                !amount_specified_is_input,
            )
        } else {
            // println!("ASFFADf");
            math_sqrt_price::get_amount_a_delta_(
                sqrt_price_current,
                sqrt_price_target,
                liquidity,
                !amount_specified_is_input,
            )
        }
    }
}

mod math_sqrt_price {
    use super::{
        full_math_u128,
        math_u256,
        math_u128
    };
    use ethnum::U256;
    // use std::ops::Shl;

    const RESOLUTION: u8 = 64;
    const Q64: u128 = 0x10000000000000000;
    const MAX_U64: u128 = 0xffffffffffffffff;
    const SCALE_FACTOR: u128 = 10000;
    const DECIMAL_PLACES: u8 = 64;

    pub fn get_amount_a_delta_(
        mut sqrt_price_a: u128,
        mut sqrt_price_b: u128,
        liquidity: u128,
        round_up: bool,
    ) -> u128 {
        assert!(sqrt_price_a > 0, "Invalid sqrt price.");

        if sqrt_price_a > sqrt_price_b {
            (sqrt_price_a, sqrt_price_b) = (sqrt_price_b, sqrt_price_a);
        };

        let (sqrt_price_a_u256, sqrt_price_b_u256, liquidity_u256) = 
            (U256::from(sqrt_price_a), U256::from(sqrt_price_b), U256::from(liquidity));

        let numerator1 = liquidity_u256 << RESOLUTION;
        let numerator2 = sqrt_price_b_u256 - sqrt_price_a_u256;

        let amount_a;
        if round_up {
            amount_a = math_u256::div_round(
                numerator1 * numerator2 / sqrt_price_b_u256,
                sqrt_price_a_u256,
                true
            );
        } else {
            amount_a = numerator1 * numerator2 / sqrt_price_b_u256 / sqrt_price_a_u256;
        };

        amount_a.as_u128()
    }

    pub fn get_amount_b_delta_(
        mut sqrt_price_a: u128,
        mut sqrt_price_b: u128,
        liquidity: u128,
        round_up: bool,
    ) -> u128 {
        if sqrt_price_a > sqrt_price_b {
            (sqrt_price_a, sqrt_price_b) = (sqrt_price_b, sqrt_price_a);
        };

        let amount_b;

        if round_up {
            amount_b = full_math_u128::mul_div_round(liquidity, sqrt_price_b - sqrt_price_a, Q64);
        } else {
            amount_b = full_math_u128::mul_div_floor(liquidity, sqrt_price_b - sqrt_price_a, Q64);
        }

        amount_b
    }

    pub fn get_amount_a_delta(
        sqrt_price_a: u128,
        sqrt_price_b: u128,
        liquidity: i128,
    ) -> i128 {
        if liquidity < 0 {
            - (get_amount_a_delta_(
                sqrt_price_a, 
                sqrt_price_b, 
                liquidity.abs() as u128, 
                false
            ) as i128)
        } else {
            get_amount_a_delta_(
                sqrt_price_a, 
                sqrt_price_b, 
                liquidity.abs() as u128, 
                true
            ) as i128
        }
    }

    pub fn get_amount_b_delta(
        sqrt_price_a: u128,
        sqrt_price_b: u128,
        liquidity: i128,
    ) -> i128 {
        if liquidity < 0 {
            - (get_amount_b_delta_(
                sqrt_price_a, 
                sqrt_price_b, 
                liquidity.abs() as u128, 
                false
            ) as i128)
        } else {
            get_amount_b_delta_(
                sqrt_price_a, 
                sqrt_price_b, 
                liquidity.abs() as u128, 
                true
            ) as i128
        }
    }

    pub fn get_next_sqrt_price(
        sqrt_price: u128,
        liquidity: u128,
        amount: u128,
        amount_specified_is_input: bool,
        a_to_b: bool,
    ) -> u128 {
        if amount_specified_is_input == a_to_b {
            get_next_sqrt_price_from_amount_a_rounding_up(
                sqrt_price,
                liquidity,
                amount,
                amount_specified_is_input
            )
        } else {
            get_next_sqrt_price_from_amount_b_rounding_down(
                sqrt_price, 
                liquidity, 
                amount, 
                amount_specified_is_input
            )
        }
    }

    pub fn get_next_sqrt_price_from_amount_a_rounding_up(
        sqrt_price: u128,
        liquidity: u128,
        amount: u128,
        add: bool
    ) -> u128 {
        if amount == 0 {
            return sqrt_price;
        }

        let (sqrt_price_u256, liquidity_u256, amount_u256) =
            (U256::from(sqrt_price), U256::from(liquidity), U256::from(amount));

        let p = amount_u256 * sqrt_price_u256;
        let numerator = (liquidity_u256 * sqrt_price_u256) << RESOLUTION;

        let liquidity_shl = liquidity_u256 << RESOLUTION;
        let denominator = if add {
            liquidity_shl + p
        } else {
            liquidity_shl - p
        };

        math_u256::div_round(numerator, denominator, true).as_u128()
    }

    pub fn get_next_sqrt_price_from_amount_b_rounding_down(
        sqrt_price: u128,
        liquidity: u128,
        amount: u128,
        add: bool,
    ) -> u128 {
        if add {
            let quotient = if amount <= MAX_U64 {
                (amount << RESOLUTION) / liquidity
            } else {
                full_math_u128::mul_div_floor(amount, Q64, liquidity)
            };
            sqrt_price + quotient
        } else {
            let quotient = if amount <= MAX_U64 {
                math_u128::checked_div_round(amount << RESOLUTION, liquidity, true)
            } else {
                full_math_u128::mul_div_round(amount, Q64, liquidity)
            };

            assert!(sqrt_price > quotient, "Invalid sqrt_price.");

            sqrt_price - quotient
        }
    }
    
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn test_get_amount_b_delta_() {
            let delta = get_amount_b_delta_(
                18446743083709604748,
                18446744073709551616,
                18446744073709551616,
                false
            );
            assert!(delta == 989999946868);
        }
    }
}

pub mod math_tick {
    use super::{
        full_math_u128,
        math_u128
    };

    pub const MAX_U64: u64 = 0xffffffffffffffff;
    pub const MAX_U128: u128 = 0xffffffffffffffffffffffffffffffff;
    pub const MAX_SQRT_PRICE_X64: u128 = 79226673515401279992447579055;
    pub const MIN_SQRT_PRICE_X64: u128 = 4295048016;
    pub const MAX_TICK_INDEX: i32 = 443636;
    pub const MIN_TICK_INDEX: i32 = -443636;
    pub const BIT_PRECISION: u32 = 14;
    pub const LOG_B_2_X32: u128 = 59543866431248;
    pub const LOG_B_P_ERR_MARGIN_LOWER_X64: u128 = 184467440737095516; // 0.01
    pub const LOG_B_P_ERR_MARGIN_UPPER_X64: u128 = 15793534762490258745; // 2^-precision / log_2_b + 0.01

    pub fn tick_index_from_sqrt_price(
        sqrt_price_x64: u128
    ) -> i32 {
        let msb = 128 - math_u128::leading_zeros(sqrt_price_x64) - 1;
        let log2p_integer_x32 = ((msb as i128) - 64i128) << 32;

        let mut bit = 0x8000_0000_0000_0000_i128;
        let mut precision = 0;
        let mut log2p_fraction_x64 = 0i128;
        let mut r = if msb >= 64 {
            sqrt_price_x64 >> (msb - 63)
        } else {
            sqrt_price_x64 << (63 - msb)
        };

        while bit > 0 && precision < BIT_PRECISION {
            r = r * r;
            let is_r_more_than_two = r >> 127 as u32;
            r = r >> (63 + is_r_more_than_two as u8);
            log2p_fraction_x64 = log2p_fraction_x64 + (bit * is_r_more_than_two as i128);
            bit = bit >> 1;
            precision = precision + 1;
        }

        let log2p_fraction_x32 = log2p_fraction_x64 >> 32;
        let log2p_x32 = log2p_integer_x32 + log2p_fraction_x32;

        // Transform from base 2 to base b
        let logbp_x64 = log2p_x32 * LOG_B_2_X32 as i128;

        let tick_low = ((logbp_x64 - LOG_B_P_ERR_MARGIN_LOWER_X64 as i128) >> 64) as i32;
        let tick_high = ((logbp_x64 + LOG_B_P_ERR_MARGIN_UPPER_X64 as i128) >> 64) as i32;

        let result_tick;
        if tick_low == tick_high {
            result_tick = tick_low;
        } else {
            let actual_tick_high_sqrt_price_x64 = sqrt_price_from_tick_index(tick_high);
            if actual_tick_high_sqrt_price_x64 <= sqrt_price_x64 {
                result_tick = tick_high;
            } else {
                result_tick = tick_low;
            }
        }
        
        result_tick
    }

    pub fn get_min_tick(
        tick_spacing: u32
    ) -> i32 {
        let tick_spacing = tick_spacing as i32;
        MIN_TICK_INDEX / tick_spacing * tick_spacing
    }

    pub fn get_max_tick(
        tick_spacing: u32
    ) -> i32 {
        let tick_spacing = tick_spacing as i32;
        MAX_TICK_INDEX / tick_spacing * tick_spacing
    }

    pub fn max_liquidity_per_tick(
        tick_spacing: u32
    ) -> u128 {
        let min_tick_index = get_min_tick(tick_spacing);
        let max_tick_index = get_max_tick(tick_spacing);

        let num_ticks = ((max_tick_index - min_tick_index) / tick_spacing as i32).abs() as u32 + 1;
        let liquidity = MAX_U128 / (num_ticks as u128);

        liquidity
    }

    pub fn sqrt_price_from_tick_index(
        tick: i32
    ) -> u128 {
        if tick >= 0 {
            get_sqrt_price_positive_tick(tick)
        } else {
            get_sqrt_price_negative_tick(tick)
        }
    }

    pub fn get_sqrt_price_positive_tick(
        tick: i32
    ) -> u128 {
        let mut ratio;
        if tick & 1i32 != 0 {
            ratio = 79232123823359799118286999567
        } else {
            ratio = 79228162514264337593543950336
        };
        if tick & 2i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 79236085330515764027303304731, 96u8);
        };
        if tick & 4i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 79244008939048815603706035061, 96u8);
        };
        if tick & 8i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 79259858533276714757314932305, 96u8);
        };
        if tick & 16i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 79291567232598584799939703904, 96u8);
        };
        if tick & 32i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 79355022692464371645785046466, 96u8);
        };
        if tick & 64i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 79482085999252804386437311141, 96u8);
        };
        if tick & 128i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 79736823300114093921829183326, 96u8);
        };
        if tick & 256i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 80248749790819932309965073892, 96u8);
        };
        if tick & 512i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 81282483887344747381513967011, 96u8);
        };
        if tick & 1024i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 83390072131320151908154831281, 96u8);
        };
        if tick & 2048i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 87770609709833776024991924138, 96u8);
        };
        if tick & 4096i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 97234110755111693312479820773, 96u8);
        };
        if tick & 8192i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 119332217159966728226237229890, 96u8);
        };
        if tick & 16384i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 179736315981702064433883588727, 96u8);
        };
        if tick & 32768i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 407748233172238350107850275304, 96u8);
        };
        if tick & 65536i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 2098478828474011932436660412517, 96u8);
        };
        if tick & 131072i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 55581415166113811149459800483533, 96u8);
        };
        if tick & 262144i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 38992368544603139932233054999993551, 96u8);
        };

        ratio >> 32
    }

    pub fn get_sqrt_price_negative_tick(
        tick: i32
    ) -> u128 {
        let abs_tick = tick.abs();
        let mut ratio;
        if abs_tick & 1i32 != 0 {
            ratio = 18445821805675392311
        } else {
            ratio = 18446744073709551616
        };

        if abs_tick & 2i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 18444899583751176498, 64u8);
        };
        if abs_tick & 4i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 18443055278223354162, 64u8);
        };
        if abs_tick & 8i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 18439367220385604838, 64u8);
        };
        if abs_tick & 16i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 18431993317065449817, 64u8);
        };
        if abs_tick & 32i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 18417254355718160513, 64u8);
        };
        if abs_tick & 64i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 18387811781193591352, 64u8);
        };
        if abs_tick & 128i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 18329067761203520168, 64u8);
        };
        if abs_tick & 256i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 18212142134806087854, 64u8);
        };
        if abs_tick & 512i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 17980523815641551639, 64u8);
        };
        if abs_tick & 1024i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 17526086738831147013, 64u8);
        };
        if abs_tick & 2048i32 != 0{
            ratio = full_math_u128::mul_shr(ratio, 16651378430235024244, 64u8);
        };
        if abs_tick & 4096i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 15030750278693429944, 64u8);
        };
        if abs_tick & 8192i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 12247334978882834399, 64u8);
        };
        if abs_tick & 16384i32 != 0{
            ratio = full_math_u128::mul_shr(ratio, 8131365268884726200, 64u8);
        };
        if abs_tick & 32768i32 != 0  {
            ratio = full_math_u128::mul_shr(ratio, 3584323654723342297, 64u8);
        };
        if abs_tick & 65536i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 696457651847595233, 64u8);
        };
        if abs_tick & 131072i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 26294789957452057, 64u8);
        };
        if abs_tick & 262144i32 != 0 {
            ratio = full_math_u128::mul_shr(ratio, 37481735321082, 64u8);
        };

        ratio
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        // use super::math_tick::MIN_TICK_INDEX;

        #[test]
        fn test_sqrt_price_from_tick_index_at_max() {
            let r = tick_index_from_sqrt_price(MAX_SQRT_PRICE_X64);
            assert!(r == MAX_TICK_INDEX);
        }
    
        #[test]
        fn test_sqrt_price_from_tick_index_at_min() {
            let r = tick_index_from_sqrt_price(MIN_SQRT_PRICE_X64);
            assert!(r == MIN_TICK_INDEX);
        }
    
        #[test]
        fn test_sqrt_price_from_tick_index_at_max_add_one() {
            let sqrt_price_x64_max_add_one = MAX_SQRT_PRICE_X64 + 1;
            let tick_from_max_add_one = tick_index_from_sqrt_price(sqrt_price_x64_max_add_one);
            let sqrt_price_x64_max = MAX_SQRT_PRICE_X64;
            let tick_from_max = tick_index_from_sqrt_price(sqrt_price_x64_max);
    
            // We don't care about accuracy over the limit. We just care about it's equality properties.
            assert!(tick_from_max_add_one == tick_from_max);
        }

        #[test]
        #[should_panic]
        fn test_tick_exceed_max() {
            let sqrt_price_from_max_tick_add_one = sqrt_price_from_tick_index(MAX_TICK_INDEX + 1);
            let sqrt_price_from_max_tick = sqrt_price_from_tick_index(MAX_TICK_INDEX);
            assert!(sqrt_price_from_max_tick_add_one > sqrt_price_from_max_tick);
        }
    
        #[test]
        fn test_tick_below_min() {
            let sqrt_price_from_min_tick_sub_one = sqrt_price_from_tick_index(MIN_TICK_INDEX - 1);
            let sqrt_price_from_min_tick = sqrt_price_from_tick_index(MIN_TICK_INDEX);

            assert!(sqrt_price_from_min_tick_sub_one < sqrt_price_from_min_tick);
        }
    
        #[test]
        fn test_tick_at_max() {
            let r = sqrt_price_from_tick_index(MAX_TICK_INDEX);
            assert!(r == MAX_SQRT_PRICE_X64);
        }
    
        #[test]
        fn test_tick_at_min() {
            let r = sqrt_price_from_tick_index(MIN_TICK_INDEX);
            assert!(r == MIN_SQRT_PRICE_X64);
        }
    
        #[test]
        fn test_get_min_tick_10() {
            let min_tick = get_min_tick(10);
            let max_tick = get_max_tick(10);
            assert!(min_tick == -443630, "min_tick");
            assert!(max_tick == 443630, "max_tick");
        }

        #[test]
        fn test_get_min_tick_300() {
            let min_tick = get_min_tick(200);
            let max_tick = get_max_tick(200);
            assert!(min_tick == -443600, "min_tick");
            assert!(max_tick == 443600, "max_tick");
        }
    
        #[test]
        fn test_get_min_tick_max() {
            let min_tick = get_min_tick(16383);
            let max_tick = get_max_tick(16383);
            assert!(min_tick == -442341, "min_tick");
            assert!(max_tick == 442341, "max_tick");
        }
    }

}

mod math_u256 {
    use ethnum::U256;
    pub fn div_round(num: U256, denom: U256, round_up: bool) -> U256  {
        let p = num / denom;
        if round_up && (p * denom) != num {
            p + 1
        } else {
            p
        }
    }
}

mod full_math_u128 {
    // use std::ops::Shr;
    use ethnum::U256;

    pub fn mul_div_round(a: u128, b: u128, denom: u128) -> u128 {
        let r: U256 = (full_mul(a, b) + (U256::from(denom) >> 1)) / U256::from(denom);
        r.as_u128()
    }

    pub fn mul_div_floor(a: u128, b: u128, denom: u128) -> u128 {
        let r = full_mul(a, b) / U256::from(denom);
        r.as_u128()
    }

    pub fn mul_div_ceil(a: u128, b: u128, denom: u128) -> u128 {
        let r = (full_mul(a, b) + (U256::from(denom) - U256::from(1_u8))) / U256::from(denom);
        r.as_u128()
    }

    pub fn mul_shr(a: u128, b: u128, shift: u8) -> u128 {
        let product = full_mul(a, b) >> shift;
        product.as_u128()
    }

    pub fn mul_shl(a: u128, b: u128, shift: u8) -> u128 {
        let product = full_mul(a, b) << shift;
        product.as_u128()
    }

    pub fn full_mul(a: u128, b: u128) -> U256 {
        U256::from(a) * U256::from(b)
    }
}

mod math_u128 {
    pub fn checked_div_round(
        num: u128,
        denom: u128,
        round_up: bool
    ) -> u128 {
        assert!(denom != 0, "Divide by zero.");

        let quotient = num / denom;
        let remainder = num % denom;
        if round_up && (remainder > 0) {
            return quotient + 1
        };

        quotient
    }

    pub fn leading_zeros(a: u128) -> u8 {
        if a == 0 {
            return 128
        }

        let a1 = a & 0xFFFFFFFFFFFFFFFF;
        let a2 = a >> 64;

        if a2 == 0 {
            let mut bit = 64;

            while bit >= 1 {
                let b = (a1 >> (bit - 1)) & 1;
                if b != 0 {
                    break
                };

                bit = bit - 1;
            };

            return (64 - bit) + 64
        } else {
            let mut bit = 128;
            while bit >= 1 {
                let b = (a >> (bit - 1)) & 1;
                if b != 0 {
                    break
                };
                bit = bit - 1;
            };

            return 128 - bit
        }
    }
}