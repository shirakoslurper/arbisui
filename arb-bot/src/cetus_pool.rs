use std::thread::current;
use std::cmp;
use std::num::Wrapping;

use std::time::Instant;

use self::tick::next_index_for_swap;

#[derive(Clone, Debug)]
pub struct Pool {
    pub tick_spacing: u32,
    pub fee_rate: u64,
    pub liquidity: u128,
    pub current_sqrt_price: u128,
    pub current_tick_index: i32,
    pub fee_growth_global_a: u128,
    pub fee_growth_global_b: u128,
    pub fee_protocol_coin_a: u64,
    pub fee_protocol_coin_b: u64,
    pub tick_manager: tick::TickManager,
    pub is_pause: bool
}

#[derive(Clone, Debug)]
pub struct SwapResult {
    pub before_sqrt_price: u128,
    pub after_sqrt_price: u128,
    pub amount_in: u64,
    pub amount_out: u64,
    pub fee_amount: u64,
    pub ref_fee_amount: u64,
    pub steps: u64
}

// First test then modify this function so it does NOT mutate the pool
// This will make running repeat calculations easier 

struct ComputeSwapState {
    current_sqrt_price: u128,
    current_tick_index: i32,
    liquidity: u128,
    fee_growth_global_a: u128,
    fee_growth_global_b: u128,
    fee_protocol_coin_a: u64,
    fee_protocol_coin_b: u64
}

pub fn swap_in_pool(
    pool: &mut Pool,
    a_to_b: bool,
    amount_specified_is_input: bool,
    sqrt_price_limit: u128,
    amount_specified: u64,
    protocol_fee_rate: u64,
    fee_protocol: u64,   // hard coded to 0 at highest level flash_swap()
    simulating: bool
) -> SwapResult {

    // // Sanity check: adding all the liquidity
    // let all_tick_net_liquidity = pool.tick_manager.ticks
    //     .iter()
    //     .fold(0i128, |net, tick| {
    //         net + tick.1.liquidity_net
    //     });

    // if all_tick_net_liquidity != 0 {
    //     println!("all_tick_net_liquidity: {}, pool.liquidity: {}", all_tick_net_liquidity, pool.liquidity);
    //     panic!();
    // }

    let mut swap_result = SwapResult {
        before_sqrt_price: pool.current_sqrt_price,
        after_sqrt_price: 0,
        amount_in: 0, 
        amount_out: 0, 
        fee_amount: 0, 
        ref_fee_amount: 0, 
        steps: 0
    };

    let mut compute_swap_state = ComputeSwapState {
        current_sqrt_price: pool.current_sqrt_price,
        current_tick_index: pool.current_tick_index,
        liquidity: pool.liquidity,
        fee_growth_global_a: pool.fee_growth_global_a,
        fee_growth_global_b: pool.fee_growth_global_b,
        fee_protocol_coin_a: pool.fee_protocol_coin_a,
        fee_protocol_coin_b: pool.fee_protocol_coin_b
    };

    let mut amount_remaining = amount_specified;

    let mut next_index = tick::next_index_for_swap(
        &pool.tick_manager,
        compute_swap_state.current_tick_index,
        a_to_b
    );

    let mut amount_calculated = 0;
    // let mut loop_count = 0;

    while amount_remaining > 0 && compute_swap_state.current_sqrt_price != sqrt_price_limit {
        // println!("loop # {}:", loop_count);
        // loop_count += 1;

        // println!("a_to_b: {}", true);
        // println!("liquidity: {}", compute_swap_state.liquidity);

        if compute_swap_state.liquidity == 0 {  // Maybe add as another condiiton
            break;
        }

        // assert!(!current_score_and_tick.is_none());
        // println!("amount_remaining = {}", amount_remaining);
        // println!("compute_swap_state.current_sqrt_price = {}\nsqrt_price_limit = {}", compute_swap_state.current_sqrt_price, sqrt_price_limit);

        // println!("next_index_and_tick pre advance: {:#?}", next_index_and_tick);

        // println!("next_index (advanced in prev loop): {}", next_index.unwrap());
        let next_tick = pool.tick_manager.ticks
            .get(&next_index.unwrap())
            .unwrap();


        // println!("next_index = {}, next_tick = {:#?}", next_index, next_tick);

        // In place of borrow tick for swap
        // We've already gotten the tick, we just need the score of the next tick
        // Advance for next iteration
        // next_index = if a_to_b {
        //     // Selling a
        //     // println!("a_to_b: true");
        //     // println!("{:#?}", pool.tick_manager.ticks.range(..next_index.unwrap()));
        //     pool.tick_manager.ticks.range(..next_index.unwrap()).map(|(score, _)| score.clone()).next_back()
        // } else {
        //     // Selling b
        //     // println!("a_to_b: false");
        //     // println!("{:#?}", pool.tick_manager.ticks.range(next_index.unwrap()+1..));
        //     pool.tick_manager.ticks.range(next_index.unwrap()+1..).map(|(score, _)| score.clone()).next()
        // };
        next_index = tick::next_index_for_swap(
            &pool.tick_manager,
            next_index.unwrap(),
            a_to_b
        );

        let next_tick_index = next_tick.index;
        // loc10
        let next_tick_sqrt_price = next_tick.sqrt_price;

        // println!("next_index_and_tick post advance: {:#?}", next_index_and_tick);
        // panic!("HALT");


        // loc7
        let sqrt_price_next_tick_w_limit = if a_to_b {
            // println!("a_to_b");
            // println!("a_to_b: {}, current sqrt_price: {}, sqrt_price_limit: {}, next_tick_sqrt_price: {}", a_to_b, compute_swap_state.current_sqrt_price, sqrt_price_limit, next_tick_sqrt_price);
            cmp::max(sqrt_price_limit, next_tick_sqrt_price)
        } else {
            // println!("a_to_b: {}, current sqrt_price: {}, sqrt_price_limit: {}, next_tick_sqrt_price: {}", a_to_b, compute_swap_state.current_sqrt_price, sqrt_price_limit, next_tick_sqrt_price);
            // panic!("ASSAS");
            cmp::min(sqrt_price_limit, next_tick_sqrt_price)
        };

        // println!("current tick: {}, target tick: {}", compute_swap_state.current_tick_index, next_tick_index);
        // println!("liquidity: {}", compute_swap_state.liquidity);

        // loc18 (sqrt_price_next_computed)
        let (amount_in, amount_out, sqrt_price_next_computed, fee_amount) = clmm_math::compute_swap_step(
            compute_swap_state.current_sqrt_price,
            sqrt_price_next_tick_w_limit,
            compute_swap_state.liquidity,
            amount_remaining,
            pool.fee_rate,
            a_to_b,
            amount_specified_is_input
        );

        if amount_in != 0 || fee_amount != 0 {
            if amount_specified_is_input {
                amount_remaining = amount_remaining - amount_in - fee_amount;
            } else {
                amount_remaining = amount_remaining - amount_out;
            }
        }

        swap_result.amount_in += amount_in;
        swap_result.amount_out += amount_out;
        swap_result.fee_amount += fee_amount;
        swap_result.steps += 1;

        // println!("amount_in = {}, amount_out = {}", swap_result.amount_in, swap_result.amount_out);

        amount_calculated += update_pool_fee(
            pool,
            fee_amount,
            protocol_fee_rate,
            a_to_b,
            simulating
        );

        // if sqrt_price_next_tick_w_limit == next_tick_sqrt_price
        if sqrt_price_next_computed == next_tick_sqrt_price {
            // println!("YES");
            compute_swap_state.current_sqrt_price = sqrt_price_next_tick_w_limit;
            
            compute_swap_state.current_tick_index = if a_to_b {
                next_tick_index - 1
            } else {
                next_tick_index
            };

            compute_swap_state.liquidity = tick::cross_by_swap(
                &mut pool.tick_manager,
                next_tick_index, // loc9! fixed from current_swap_state.current tick index which would fail in the a to b direction
                a_to_b,
                compute_swap_state.liquidity,
                compute_swap_state.fee_growth_global_a,
                compute_swap_state.fee_growth_global_b,
                simulating
            );
        } else if compute_swap_state.current_sqrt_price != next_tick_sqrt_price {
            compute_swap_state.current_sqrt_price = sqrt_price_next_computed;
            compute_swap_state.current_tick_index = tick_math::tick_index_from_sqrt_price(compute_swap_state.current_sqrt_price);
        }

        // println!("amount_remaining: {}", amount_remaining);        
        // println!("sqrt_price post loop: {}", compute_swap_state.current_sqrt_price);
        // println!("end of loop 1: amount_remaining = {}", amount_remaining);
    }

    swap_result.after_sqrt_price = compute_swap_state.current_sqrt_price;
    swap_result.ref_fee_amount = full_math_u64::mul_div_floor(amount_calculated, fee_protocol, 10000);

    // println!("post swap sqrt_price: {}", compute_swap_state.current_sqrt_price);
    // println!("post swap tick: {}", compute_swap_state.current_tick_index);

    if !simulating {
        pool.current_sqrt_price = compute_swap_state.current_sqrt_price;
        pool.current_tick_index = compute_swap_state.current_tick_index;
        pool.liquidity = compute_swap_state.liquidity;
        pool.fee_growth_global_a = compute_swap_state.fee_growth_global_a;
        pool.fee_growth_global_b = compute_swap_state.fee_growth_global_b;

        if a_to_b {
            pool.fee_protocol_coin_a += amount_calculated - swap_result.ref_fee_amount;
        } else {
            pool.fee_protocol_coin_b += amount_calculated - swap_result.ref_fee_amount;
        }
    }

    // println!("swap_result: {:#?}", swap_result);
    swap_result
}

fn update_pool_fee(
    pool: &mut Pool,
    fee_amount: u64,
    protocol_fee_rate: u64,
    a_to_b: bool,
    simulating: bool
) -> u64 {
    let delta = full_math_u64::mul_div_ceil(fee_amount, protocol_fee_rate, 10000);

    let new_fee_amount = fee_amount - delta;

    if new_fee_amount == 0 || pool.liquidity == 0 {
        return delta;
    }

    let temp = ((new_fee_amount as u128) << 64) / pool.liquidity;

    if !simulating {
        if a_to_b {
            pool.fee_growth_global_a += (Wrapping(pool.fee_growth_global_a) + Wrapping(temp)).0;
        } else {
            pool.fee_growth_global_b += (Wrapping(pool.fee_growth_global_b) + Wrapping(temp)).0;
        }
    }

    delta
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use super::*;

    #[test]
    fn test_compute_swap_buy_eth() {
        let mut ticks = BTreeMap::new();
        ticks.insert(
            84222,
            tick::Tick {
                index: 84222,
                sqrt_price: tick_math::sqrt_price_from_tick_index(84222),
                liquidity_net: 1517882343751509868544,
                liquidity_gross: 1517882343751509868544,
                fee_growth_outside_a: 0,
                fee_growth_outside_b: 0
            }
        );
        ticks.insert(
            86129,
            tick::Tick {
                index: 86129,
                sqrt_price: tick_math::sqrt_price_from_tick_index(86129),
                liquidity_net: -1517882343751509868544,
                liquidity_gross: 1517882343751509868544,
                fee_growth_outside_a: 0,
                fee_growth_outside_b: 0
            }
        );

        println!("ticks: {:#?}", ticks);

        let tick_manager = tick::TickManager {
            tick_spacing: 1,
            ticks
        };

        let mut pool = Pool {
            tick_spacing: 1,
            fee_rate: 0,
            liquidity: 1517882343751509868544,
            current_sqrt_price: 1304381782533278269440,
            current_tick_index: 85176,
            fee_growth_global_a: 0,
            fee_growth_global_b: 0,
            fee_protocol_coin_a: 0,
            fee_protocol_coin_b: 0,
            tick_manager,
            is_pause: false
        };

        let swap_result = swap_in_pool(
            &mut pool,
            false,
            true,
            tick_math::MAX_SQRT_PRICE_X64 + 1,
            42_000_000_000_000_000,
            0,
            0,
            false
        );

        println!("swap result: {:#?}", swap_result);
        println!("expected amt out: 8399996712957");

    }

    #[test]
    fn test_compute_swap_buy_usdc() {
        let mut ticks = BTreeMap::new();
        ticks.insert(
            84222,
            tick::Tick {
                index: 84222,
                sqrt_price: tick_math::sqrt_price_from_tick_index(84222),
                liquidity_net: 1517882343751509868544,
                liquidity_gross: 1517882343751509868544,
                fee_growth_outside_a: 0,
                fee_growth_outside_b: 0
            }
        );
        ticks.insert(
            86129,
            tick::Tick {
                index: 86129,
                sqrt_price: tick_math::sqrt_price_from_tick_index(86129),
                liquidity_net: -1517882343751509868544,
                liquidity_gross: 1517882343751509868544,
                fee_growth_outside_a: 0,
                fee_growth_outside_b: 0
            }
        );

        println!("ticks: {:#?}", ticks);

        let tick_manager = tick::TickManager {
            tick_spacing: 1,
            ticks
        };

        let mut pool = Pool {
            tick_spacing: 1,
            fee_rate: 0,
            liquidity: 1517882343751509868544,
            current_sqrt_price: 1304381782533278269440,
            current_tick_index: 85176,
            fee_growth_global_a: 0,
            fee_growth_global_b: 0,
            fee_protocol_coin_a: 0,
            fee_protocol_coin_b: 0,
            tick_manager,
            is_pause: false
        };

        let swap_result = swap_in_pool(
            &mut pool,
            true,
            true,
            tick_math::MIN_SQRT_PRICE_X64 + 1,
            13_370_000_000_000,
            0,
            0,
            false
        );

        println!("swap result: {:#?}", swap_result);
        println!("expected amt out: 66849958362998925");

    }

}

pub mod tick {
    use super::tick_math;
    use std::collections::BTreeMap;
    use std::num::Wrapping;

    #[derive(Debug, Clone)]
    pub struct TickManager {
        pub tick_spacing: u32,
        pub ticks: BTreeMap<i32, Tick> // Tick score to Tick
    }

    #[derive(Debug, Clone)]
    pub struct Tick {
        pub index: i32,
        pub sqrt_price: u128,
        pub liquidity_net: i128,
        pub liquidity_gross: u128,
        pub fee_growth_outside_a: u128,
        pub fee_growth_outside_b: u128,
        // points_growth_outside: u128,
        // rewards_growth_outside: Vec<u128>
    }

    pub fn first_index_and_tick_for_swap(
        tick_manager: &TickManager,
        tick_index: i32,
        a_to_b: bool
    ) -> Option<(i32, Tick)> {
        // let score = tick_score(tick_index);

        if a_to_b {
            tick_manager.ticks.range(..tick_index).map(|(index, tick)| (index.clone(), tick.clone())).next_back()
        } else {
            tick_manager.ticks.range(tick_index+1..).map(|(index, tick)| (index.clone(), tick.clone())).next()
        }
    }

    pub fn first_index_for_swap(
        tick_manager: &TickManager,
        tick_index: i32,
        a_to_b: bool
    ) -> Option<i32> {
        // let score = tick_score(tick_index);

        // When moving down, the current tick is included?, when moving up it is excluded
        if a_to_b {
            tick_manager.ticks.range(..tick_index+1).map(|(index, _)| index.clone()).next_back()
        } else {
            tick_manager.ticks.range(tick_index+1..).map(|(index, _)| index.clone()).next()
        }
    }

    pub fn next_index_for_swap(
        tick_manager: &TickManager,
        tick_index: i32,
        a_to_b: bool
    ) -> Option<i32> {
        // let score = tick_score(tick_index);

        // When moving down, the current tick is included?, when moving up it is excluded
        if a_to_b {
            tick_manager.ticks.range(..tick_index).map(|(index, _)| index.clone()).next_back()
        } else {
            tick_manager.ticks.range(tick_index+1..).map(|(index, _)| index.clone()).next()
        }
    }

    pub fn tick_score(tick_index: i32) -> u64 {
        // equivalent to checking that -TICK_BOUND <= score <= TICK_BOUND 
        let score = (tick_index + tick_math::TICK_BOUND) as u32;
        assert!(score <= tick_math::TICK_BOUND as u32 * 2);

        score as u64
    }

    pub fn cross_by_swap(
        tick_manager: &mut TickManager,
        tick_index: i32,
        a_to_b: bool,
        liquidity: u128,
        fee_growth_global_a: u128,
        fee_growth_global_b: u128,
        simulating: bool
    ) -> u128 {

        let all_ticks_liquidity_net = tick_manager.ticks
            .iter()
            .fold(0i128, |net, tick| {
                net + tick.1.liquidity_net
            });

        // let tick = tick_manager.ticks.get_mut(&tick_score(tick_index)).unwrap();
        // let tick = tick_manager.ticks.get_mut(&tick_index).unwrap();

        let tick = tick_manager.ticks.entry(tick_index).or_insert(
            Tick {
                index: tick_index,
                sqrt_price: tick_math::sqrt_price_from_tick_index(tick_index),
                liquidity_net: 0,
                liquidity_gross: 0,
                fee_growth_outside_a: 0,
                fee_growth_outside_b: 0
            }
        );

        let directional_liquidity_net = if a_to_b {
            -tick.liquidity_net
        } else {
            tick.liquidity_net
        };

        // equivalent of math_liquidity::add_delta in turbos_pool
        let abs_directional_liquidity_net = directional_liquidity_net.abs() as u128;

        let liquidity = if directional_liquidity_net >= 0 {
            assert!(
                u128::MAX - abs_directional_liquidity_net >= liquidity,
                "u128::MAX - abs_directional_liquidity_net >= liquidity: liquidity: {}, abs_directional_liquidity_net: {}, all_ticks_liquidity_net: {}",
                liquidity, abs_directional_liquidity_net, all_ticks_liquidity_net
            );
            liquidity + abs_directional_liquidity_net
        } else {
            assert!(
                liquidity >= abs_directional_liquidity_net,
                "liquidity >= abs_directional_liquidity_net: liquidity: {}, abs_directional_liquidity_net: {}, all_ticks_liquidity_net: {}",
                liquidity, abs_directional_liquidity_net, all_ticks_liquidity_net
            );
            // println!("liquidity: {}, abs_directional_liquidity_net: {}", liquidity, abs_directional_liquidity_net);
            liquidity - abs_directional_liquidity_net
        };

        // let liquidity_net = if a_to_b {
        //     -tick.liquidity_net
        // } else {
        //     tick.liquidity_net
        // };

        // let liquidity = liquidity + liquidity_net


        if !simulating {
            tick.fee_growth_outside_a = (Wrapping(liquidity) - Wrapping(fee_growth_global_a)).0;
            tick.fee_growth_outside_b = (Wrapping(liquidity) - Wrapping(fee_growth_global_b)).0;
        }

        liquidity
    }
}

mod clmm_math {
    use super::{
        full_math_u64,
        full_math_u128,
        math_u256,
        tick_math,
        math_u128
    };
    use ethnum::U256;

    pub fn compute_swap_step(
        sqrt_price_current: u128,
        sqrt_price_target: u128,
        liquidity: u128,
        amount_remaining: u64,
        fee_rate: u64,
        a_to_b: bool,
        amount_specified_is_input: bool
    ) -> (u64, u64, u128, u64) {
        // let next_sqrt_price = sqrt_price_target;
        // let amount_in = 0;
        // let amount_out = 0;
        // let fee_amount = 0;

        // println!("liquidity: {}", liquidity);

        // if liquidity == 0 {
        //     return (amount_in, amount_out, sqrt_price_target, fee_amount);
        // }

        // println!("current price: {}, target price: {}, liquidity: {}", sqrt_price_current, sqrt_price_target, liquidity);

        if a_to_b {
            assert!(sqrt_price_current >= sqrt_price_target);
        } else {
            assert!(sqrt_price_current < sqrt_price_target);
        }

        if amount_specified_is_input {
            // We specified amount in

            // This is the amount we're actually goin to be swapping post fees
            let amount_calc = full_math_u64::mul_div_floor(
                amount_remaining,
                1_000_000 - fee_rate,
                1_000_000
            );

            // println!("compute_swap_step(): amount_calc = {}", amount_calc);

            // How much we get out of the out token if we move price from the 
            // current sqrt_price to the target sqrt price
            let delta_up_from_input = get_delta_up_from_input(
                sqrt_price_current,
                sqrt_price_target,
                liquidity,
                a_to_b
            );

            // println!("compute_swap_step(): delta_up_from_input = {}", delta_up_from_input);

            let (amount_in, fee_amount, next_sqrt_price) = if delta_up_from_input > U256::from(amount_calc) {
                // Case: The amount of the "in" token required to move the
                // current sqrt_price to the target sqrt_price is greater
                // than the amount we are passing in (post fees)

                let amount_in = amount_calc;

                let fee_amount = amount_remaining - amount_calc;
    
                let next_sqrt_price = get_next_sqrt_price_from_input(
                    sqrt_price_current,
                    liquidity,
                    amount_calc,
                    a_to_b
                );

                // println!("compute_swap_step(): branch 1 next_sqrt_price = {}, amount_in = {}", next_sqrt_price, amount_in);

                (amount_in, fee_amount, next_sqrt_price)
            } else {
                // Case: The amount of the "in" token required to move the
                // current sqrt_price to the target sqrt_price is less than 
                // or equal to the amount we are passing in (pre fees)

                let amount_in = delta_up_from_input.as_u64();

                // The fee is taken out of what is actually traded
                let fee_amount = full_math_u64::mul_div_ceil(
                    amount_in,
                    fee_rate,
                    1_000_000 - fee_rate
                );

                let next_sqrt_price = sqrt_price_target;

                // println!("compute_swap_step(): branch 2 next_sqrt_price = {}, amount_in = {}", next_sqrt_price, amount_in);

                (amount_in, fee_amount, next_sqrt_price)
            };

            // println!("amount_in:", amount )

            let amount_out = get_delta_down_from_output(
                sqrt_price_current,
                next_sqrt_price,
                liquidity,
                a_to_b
            );

            // println!("compute_swap_step() amount_specified_is_input == true .. amount_out (U256) = {}", amount_out);
            // println!("compute_swap_step() amount_specified_is_input == true .. amount_out (U128) = {}", amount_out.as_u128());
            // println!("compute_swap_step() amount_specified_is_input == true .. amount_out (U256) <= u64::MAX = {}", amount_out <= U256::from(u64::MAX));
            // println!("compute_swap_step() amount_specified_is_input == true .. amount_out (U64) = {}", amount_out.as_u64());

            (amount_in, amount_out.as_u64(), next_sqrt_price, fee_amount)
        } else {
            // We specified amount out

            // How much we have to reduce how much of the starting coin we have
            let delta_down_from_output = get_delta_down_from_output(
                sqrt_price_current,
                sqrt_price_target,
                liquidity,
                a_to_b
            );

            let (amount_out, next_sqrt_price) = if delta_down_from_output > U256::from(amount_remaining) {
                // If we have to reduce our starting amount by more than the amount we 
                // have left to get to the target price: 
                // - we can set the amount out to how much we have left
                // - derive the new sqrt price from that

                let amount_out = amount_remaining;
                let next_sqrt_price = get_next_sqrt_price_from_output(
                    sqrt_price_current,
                    liquidity,
                    amount_remaining,
                    a_to_b
                );

                (amount_out, next_sqrt_price)
            } else {
                // If we have to reduce our starting amount by less that or equal to
                // how much we have left, we can:
                // - set amount out to the amount we derived above to get to the target sqrt price
                // - set the new sqrt price to the target sqrt price

                let amount_out = delta_down_from_output.as_u64();
                let next_sqrt_price = sqrt_price_target;

                (amount_out, next_sqrt_price)
            };

            let amount_in = get_delta_up_from_input(
                sqrt_price_current,
            next_sqrt_price,
                liquidity,
                a_to_b
            );

            // println!("compute_swap_step() amount_specified_is_input == true .. amount_in (U256) = {}", amount_in);

            let fee_amount = full_math_u64::mul_div_ceil(
                amount_in.as_u64(),
                fee_rate,
                1000000 - fee_rate
            );

            (amount_in.as_u64(), amount_out, next_sqrt_price, fee_amount)
        }
    }

    // how much of the 'in' token we need to move price
    // from current price to target price given liquidity
    fn get_delta_up_from_input(
        sqrt_price_current: u128,
        sqrt_price_target: u128,
        liquidity: u128,
        a_to_b: bool
    ) -> U256 {
        let sqrt_price_delta = if sqrt_price_current > sqrt_price_target {
            sqrt_price_current - sqrt_price_target
        } else {
            sqrt_price_target - sqrt_price_current
        };

        if sqrt_price_delta == 0 || liquidity == 0 {
            return U256::from(0u8);
        }

        // println!("get_delta_up_from_input(): sqrt_price_delta = {}", sqrt_price_delta);

        if a_to_b {
            let numerator = full_math_u128::full_mul(liquidity, sqrt_price_delta).checked_shl(64).expect("Checked shl failed.");
            let denominator = full_math_u128::full_mul(sqrt_price_current, sqrt_price_target);
            let delta_x = math_u256::div_round(numerator, denominator, true);
            
            // println!("get_delta_up_from_input(): numerator = {}", numerator);
            // println!("get_delta_up_from_input(): denominator = {}", denominator);
            // println!("get_delta_up_from_input(): delta_x = {}", delta_x);

            delta_x
        } else {
            let delta_y_pre_shift = full_math_u128::full_mul(liquidity, sqrt_price_delta);
            if delta_y_pre_shift & U256::from(18446744073709551615u128) > U256::from(0u8) {
                (delta_y_pre_shift >> 64) + U256::from(1_u8)
            } else {
                delta_y_pre_shift >> 64
            }
        }
    }

    // how much of the 'out' token we get when we move price 
    // from current price to target price given liquidity
    fn get_delta_down_from_output(
        sqrt_price_current: u128,
        sqrt_price_target: u128,
        liquidity: u128,
        a_to_b: bool
    ) -> U256 {
        let sqrt_price_delta = if sqrt_price_current > sqrt_price_target {
            sqrt_price_current - sqrt_price_target
        } else {
            sqrt_price_target - sqrt_price_current
        };

        if sqrt_price_delta == 0 || liquidity == 0 {
            return U256::from(0_u8);
        }

        if a_to_b {
            let delta_y = full_math_u128::full_mul(liquidity, sqrt_price_delta) >> 64;
            
            // println!("get_delta_down_from_output(): branch 1 delta y = {}", delta_y);

            delta_y
        } else {
            let numerator = full_math_u128::full_mul(liquidity, sqrt_price_delta).checked_shl(64).expect("Checked shl failed.");
            let denominator = full_math_u128::full_mul(sqrt_price_current, sqrt_price_target);
            let delta_x = math_u256::div_round(numerator, denominator, false);

            delta_x
        }
    }

    // TODO: CHECK SHIFTS FOR DIRECTION
    pub fn get_next_sqrt_price_from_input(
        sqrt_price_current: u128,
        liquidity: u128,
        amount: u64,
        a_to_b: bool
    ) -> u128 {
        if a_to_b {
            get_next_sqrt_price_a_up(
                sqrt_price_current,
                liquidity,
                amount,
                true
            )
        } else {
            get_next_sqrt_price_b_down(
                sqrt_price_current,
                liquidity,
                amount,
                true
            )
        }
    }

    pub fn get_next_sqrt_price_from_output(
        sqrt_price_current: u128,
        liquidity: u128,
        amount: u64,
        a_to_b: bool
    ) -> u128 {
        if a_to_b {
            get_next_sqrt_price_b_down(
                sqrt_price_current,
                liquidity,
                amount,
                false
            )
        } else {
            get_next_sqrt_price_a_up(
                sqrt_price_current,
                liquidity,
                amount,
                false
            )
        }
    }

    // rounding up
    fn get_next_sqrt_price_a_up(
        sqrt_price: u128,
        liquidity: u128,
        amount: u64,
        add: bool
    ) -> u128 {
        if amount == 0 {
            return sqrt_price;
        }

        let numerator = full_math_u128::full_mul(sqrt_price, liquidity).checked_shl(64).expect("Checked shl failed.");

        let liquidity_shl = U256::from(liquidity) << 64;
        let p = full_math_u128::full_mul(sqrt_price, amount as u128);

        let next_sqrt_price = if add {
            math_u256::div_round(
                numerator,
                liquidity_shl + p,
                true
            ).as_u128()
        } else {
            math_u256::div_round(
                numerator,
                liquidity_shl - p,
                true
            ).as_u128()
        };

        assert!(next_sqrt_price <= tick_math::MAX_SQRT_PRICE_X64 && next_sqrt_price >= tick_math::MIN_SQRT_PRICE_X64);

        next_sqrt_price
    }

    fn get_next_sqrt_price_b_down(
        sqrt_price: u128,
        liquidity: u128,
        amount: u64,
        add: bool
    ) -> u128 {
        let quotient = math_u128::checked_div_round(
            (amount as u128) << 64,
            liquidity,
            !add
        );

        let next_sqrt_price = if add {
            sqrt_price + quotient
        } else {
            sqrt_price - quotient
        };

        assert!(next_sqrt_price <= tick_math::MAX_SQRT_PRICE_X64 && next_sqrt_price >= tick_math::MIN_SQRT_PRICE_X64);

        next_sqrt_price
    }


    // #[cfg(test)]
    // mod tests {
    //     use super::*;
    //     #[test]
    //     fn test_get_amount_b_delta_() {
    //         let delta = get_amount_b_delta_(
    //             18446743083709604748,
    //             18446744073709551616,
    //             18446744073709551616,
    //             false
    //         );
    //         assert!(delta == 989999946868);
    //     }
    // }

}

// RECHECK MIN AND MAX
pub mod tick_math {
    use super::{
        full_math_u128,
        math_u128
    };

    pub const MAX_U128: u128 = 0xffffffffffffffffffffffffffffffff;
    pub const MAX_SQRT_PRICE_X64: u128 = 79226673515401279992447579055;
    pub const MIN_SQRT_PRICE_X64: u128 = 4295048016;
    pub const MAX_TICK_INDEX: i32 = 443636;
    pub const MIN_TICK_INDEX: i32 = -443636;
    pub const TICK_BOUND: i32 = 443636;

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

// mod math_u128 {
//     pub fn checked_div_rou
// }

mod full_math_u64 {

    pub fn mul_div_round(a: u64, b: u64, denom: u64) -> u64 {
        let r = (full_mul(a, b) + (denom as u128 >> 1)) / denom as u128;
        r as u64
    }

    pub fn mul_div_floor(a: u64, b: u64, denom: u64) -> u64 {
        let r = full_mul(a, b) / denom as u128;
        r as u64
    }

    pub fn mul_div_ceil(a: u64, b: u64, denom: u64) -> u64 {
        let r = (full_mul(a, b) + (denom as u128 - 1u128)) / denom as u128;
        r as u64
    }

    pub fn mul_shr(a: u64, b: u64, shift: u8) -> u64 {
        (full_mul(a, b) >> shift) as u64
    }

    pub fn mul_shl(a: u64, b: u64, shift: u8) -> u64 {
        (full_mul(a, b) << shift) as u64
    }

    pub fn full_mul(a: u64, b: u64) -> u128 {
        a as u128 * b as u128
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