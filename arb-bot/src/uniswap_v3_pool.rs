use anyhow;
use move_core_types::language_storage::TypeTag;
use std::collections:: BTreeMap;
use fixed::types::{U64F64, I64F64};

const ONE_POINT_ZERO_ZERO_ONE: f64 = 1.001;

// // As of current we're not working with deltas
// // We're grabbing end of last block state
// // and loading new information entirely.
// // The functions we need only need to operate on data
// // and track state during computation.
// pub struct Tick {
//     // Gross tally of liquidity referencing the tick
//     // Ensures that even if net liquidity is 0, we 
//     // can know if a tick is referenced by >= 1 position,
//     // which lets us know whether to update the bitmap
//     pub liquidity_gross: u128,
//     pub liquidity_net: i128,
//     pub initialized: bool, // We can simply remove from the BTreeMap
// }

// pub struct PoolInfo {
//     pub coin_x: TypeTag,
//     pub coin_y: TypeTag,
//     pub coin_x_sqrt_price: U64F64,      // U64F64
//     pub coin_y_sqrt_price: U64F64,      // U64F64
//     pub fee_rate: u64,
//     pub liquidity: u128,                // U64F64 ?
//     pub tick: i128,                     // Turbos: i32, Cetus: u64
//     pub initialized_ticks: BTreeMap<i128, Tick>,
//     pub tick_spacing: u32, 
// }

// struct SwapState {
//     amount_specified_remaining: u128,
//     amount_calculated: u128,
//     coin_x_sqrt_price: U64F64,
//     coin_y_sqrt_prict: U64F64,
//     tick: i128
// }

// struct StepState {
//     coin_x_sqrt_price_start: U64F64,
//     coin_x_sqrt_price_next: Option<U64F64>,
//     coin_y_sqrt_price_start: U64F64,
//     coin_y_sqrt_price_next: Option<U64F64>,
//     tick_next: Option<i128>,
//     amount_in: u128,
//     amount_out: u128,
//     fee_amount: u128,
// }

// impl PoolInfo {
//     fn swap(&self, amount_specified: u128, x_for_y: bool) {
//         // Iterate over initialized ticks in direction chosen by user
//         let mut state = SwapState {
//             amount_specified_remaining: amount_specified,
//             amount_calculated: 0,
//             coin_x_sqrt_price: self.coin_x_sqrt_price.clone(),
//             coin_y_sqrt_prict: self.coin_y_sqrt_price.clone(),
//             tick: self.tick,
//         };

//         let mut step = StepState {
//             coin_x_sqrt_price_start: U64F64::from_num(0),
//             coin_x_sqrt_price_next: None,
//             coin_y_sqrt_price_start: U64F64::from_num(0),
//             coin_y_sqrt_price_next: None,
//             tick_next: None,
//             amount_in: 0,
//             amount_out: 0,
//             fee_amount: 0,
//         };

//         while state.amount_specified_remaining > 0 {
//             step.coin_x_sqrt_price_start = state.coin_x_sqrt_price.clone();
//             step.tick_next = self.next_initialized_tick(state.tick, true);
//             step.coin_x_sqrt_price_next = Some(get_sqrt_ratio_at_tick(step.tick_next));

//             ()
//         }
//     }

//     // We're not using bitmaps here
//     // Though tick spacing is used by both turbos and cetus
//     // we don't need to if we're using BTreeMap.
//     // We can simply insert the tick IF it is initialized.
//     // If we find the BTreeMap to be too slow we can switch to an array.
//     fn next_initialized_tick(&self, tick: i128, x_for_y: bool) -> Option<i128> {
//         match x_for_y {
//             true => {
//                 self.initialized_ticks.range(tick..).map(|(index, _)| index.clone()).next()
//             },
//             false => {
//                 self.initialized_ticks.range(..tick).map(|(index, _)| index.clone()).next_back()
//             },
//         }
//     }
// }

// fn compute_swap_step(
//     sqrt_ratio_current: U64F64,
//     sqrt_ratio_target: U64F64,
//     liquidity: u128,
//     amount_remaining: u128,
//     fee_pips: u32,
// ) -> (
//     U64F64,     // sqrt_ratio_next 
//     u128,       // amount_in
//     u128,       // amount_out
//     u128        // fee_amount
// ) {
//     let x_for_y = sqrt_ratio_current >= sqrt_ratio_target;
//     let exact_in = amount_remaining >= 0;
//     let mut sqrt_ratio_next = U64F64::from_num(0);

//     if exact_in {
//         let amount_remaining_less_fee = (amount_remaining * (1_000_000_u128 - fee_pips as u128)) / 1_000_000;
//         let amount_in = if x_for_y {
//             get_amount_x_delta(sqrt_ratio_target, sqrt_ratio_current, liquidity)
//         } else {
//             get_amount_y_delta(sqrt_ratio_current, sqrt_ratio_target, liquidity)
//         };

//         if amount_remaining_less_fee >= amount_in {
//             sqrt_ratio_next = sqrt_ratio_target;
//         } else {
//             // sqrt_ratio_next = get_next_sqrt_price_from_input(

//             // );
//         }

//     }

//     ()
// }




//  // Calculates sqrt(1.0001 * tick)
// // We're making some compromises here
// // - Using a floating point base
// // - Using a converting tick to i32 even though Cetus uses U64
// fn get_sqrt_ratio_at_tick(tick: i128) -> U64F64 {
//     U64F64::from_num(ONE_POINT_ZERO_ZERO_ONE.powi(tick as i32))
// }

// fn get_next_sqrt_price_from_amount_x_rounding_up(
//     sqrt_price: U64F64,
//     liquidity: u128,
//     amount_in: u128
// ) -> U64F64 {
//     let liquidity = U64F64::from_num(liquidity);
//     let amount_in = U64F64::from_num(amount_in);

//     if (sqrt_price * amount_in) / amount_in == sqrt_price {
//         let denominator = liquidity + (amount_in * sqrt_price);
//         if denominator >= liquidity {
//             return (sqrt_price * liquidity) / (liquidity);
//         }
//     }

//     liquidity / (amount_in + (liquidity / sqrt_price))
// }

// fn get_amount_x_delta(
//     sqrt_ratio_a: U64F64, 
//     sqrt_ratio_b: U64F64,
//     liquidity: u128,
//     round_up: bool
// ) -> u128 {
//     // Order so that lesser is a and greater is b (to get absolute value later)
//     let (sqrt_ratio_a, sqrt_ratio_b) = if sqrt_ratio_a > sqrt_ratio_b {
//         (sqrt_ratio_b, sqrt_ratio_a)
//     } else {
//         (sqrt_ratio_a, sqrt_ratio_b)
//     };

//     let liquidity = U64F64::from_num(liquidity);

//     assert!(sqrt_ratio_a > 0);

//     ((liquidity * (sqrt_ratio_b - sqrt_ratio_a)) / (sqrt_ratio_a * sqrt_ratio_b)).to_num::<u128>()
// }

// fn get_amount_y_delta(
//     sqrt_ratio_a: U64F64, 
//     sqrt_ratio_b: U64F64,
//     liquidity: u128,
//     // round_up: blah blah precision
// ) -> u128 {
//     // Order so that lesser is a and greater is b (to get absolute value later)
//     let (sqrt_ratio_a, sqrt_ratio_b) = if sqrt_ratio_a > sqrt_ratio_b {
//         (sqrt_ratio_b, sqrt_ratio_a)
//     } else {
//         (sqrt_ratio_a, sqrt_ratio_b)
//     };

//     let liquidity = U64F64::from_num(liquidity);

//     assert!(sqrt_ratio_a > 0);

//     (liquidity * (sqrt_ratio_b - sqrt_ratio_a)).to_num::<u128>()
// }

mod pool {
    use super::math_swap;
    use std::collections::BTreeMap;
    use ethnum::U256;

    struct Pool {
        protocol_fees_a: u64,
        protocol_fees_b: u64,
        sqrt_price: u128,
        tick_current_index: i32,
        tick_spacing: u32,
        max_liquidity_per_tick: u128,
        fee: u32,
        fee_protocol: u32,
        unlocked: bool,
        fee_growth_global_a: u128,
        fee_growth_global_b: u128,
        liquidity: u128,
		tick_map: BTreeMap<i32, U256>,
    }

    struct ComputeSwapState {
        amount_a: u128,
        amount_b: u128, 
        amount_specified_remaining: u128,
        amount_calculated: u128,
        sqrt_price: u128,
        tick_current_index: i32,
        fee_growth_global: u128,
        protocol_fee: u128,
        liquidity: u128,
        fee_amount: u128,
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
        let a_to_b = sqrt_price_current >= sqrt_price_target;
        let mut fee_amount = 0;

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

        let next_sqrt_price = if amount_calc >= amount_fixed_delta {
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

        let is_max_swap = next_sqrt_price == sqrt_price_target;

        let amount_unfixed_delta = get_amount_unfixed_delta(
            sqrt_price_current,
            next_sqrt_price,
            liquidity,
            amount_specified_is_input,
            a_to_b,
        );

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
    use std::ops::Shl;

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

        let numerator1 = liquidity_u256.shl(RESOLUTION);
        let numerator2 = sqrt_price_b_u256;

        let mut amount_a;
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
        let numerator = (liquidity_u256 * sqrt_price_u256).shl(RESOLUTION);

        let liquidity_shl = liquidity_u256.shl(RESOLUTION);
        let denominator = if add {
            liquidity_shl
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
                amount.shl(RESOLUTION) / liquidity
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
    use std::ops::Shr;
    use ethnum::U256;

    pub fn mul_div_round(a: u128, b: u128, denom: u128) -> u128 {
        let r: U256 = (full_mul(a, b) + U256::from(denom).shr(1)) / U256::from(denom);
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
}
