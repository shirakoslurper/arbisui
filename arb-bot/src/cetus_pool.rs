


// pub fn swap_in_pool(

// )

mod clmm_math {
    use super::{
        full_math_u64,
        full_math_u128,
        math_u256,
        tick_math
    };
    use ethnum::U256;

    // pub fn compute_swap_step(
    //     sqrt_price_current: u128,
    //     sqrt_price_target: u128,
    //     liquidity: u128,
    //     amount_remaining: u64,
    //     fee_rate: u64,
    //     a_to_b: bool,
    //     amount_specified_is_input: bool
    // ) -> (u64, u64, u128, u64) {
    //     let next_sqrt_price = sqrt_price_target;
    //     let amount_in = 0;
    //     let amount_out = 0;
    //     let fee_amount = 0;

    //     if liquidity == 0 {
    //         return (amount_in, amount_out, next_sqrt_price, fee_amount);
    //     }

    //     if a_to_b {
    //         assert!(sqrt_price_current >= sqrt_price_target);
    //     } else {
    //         assert!(sqrt_price_current < sqrt_price_target);
    //     }

    //     if amount_specified_is_input {
    //         let amount_calc = full_math_u64::mul_div_floor(
    //             amount_remaining,
    //             1000000 - fee_rate,
    //             1000000
    //         );

    //         // let delta_up = s
    //     } else {

    //     }

    //     (0, 0, 0, 0)

    // }

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
            return U256::from(0);
        }

        if a_to_b {
            let denominator = full_math_u128::full_mul(liquidity, sqrt_price_delta).checked_shl(64).expect("Checked shl failed.");
            let numerator = full_math_u128::full_mul(sqrt_price_current, sqrt_price_target);
            let delta_x = math_u256::div_round(numerator, denominator, true);
            
            delta_x
        } else {
            let delta_y_pre_shift = full_math_u128::full_mul(liquidity, sqrt_price_delta);
            if delta_y_pre_shift & U256::from(18446744073709551615) > U256::from(0) {
                (delta_y_pre_shift >> 64) + 1
            } else {
                delta_y_pre_shift >> 64
            }
        }
    }

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
            return U256::from(0);
        }

        if a_to_b {
            let delta_y = full_math_u128::full_mul(liquidity, sqrt_price_delta) >> 64;
            delta_y
        } else {
            let denominator = full_math_u128::full_mul(liquidity, sqrt_price_delta).checked_shl(64).expect("Checked shl failed.");
            let numerator = full_math_u128::full_mul(sqrt_price_current, sqrt_price_target);
            let delta_x = math_u256::div_round(numerator, denominator, false);
            delta_x
        }
    }

    // TODO: CHECK SHIFTS FOR DIRECTION

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

        let numerator = full_math_u128::full_mul(sqrt_price, liquidity).checked_shl(64).unwrap("Checked shl failed.");

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



    #[cfg(test)]
    mod tests {
        #[test]
        fn test_get_delta_up_from_input() {

        }

        #[test]
        fn get_delta_down_from_input() {

        }

    }

}

// RECHECK MIN AND MAX
mod tick_math {
    pub const MAX_SQRT_PRICE_X64: u128 = 79226673515401279992447579055;
    pub const MIN_SQRT_PRICE_X64: u128 = 4295048016;
    pub const MAX_TICK_INDEX: i32 = 443636;
    pub const MIN_TICK_INDEX: i32 = -443636;
    pub const TICK_BOUND: i32 = 443636;

    // pub fn max_sqrt_price() -> u128 {
    //     MAX_SQRT_PRICE_X64
    // }

    // pub fn min_sqrt_price() -> u128 {
    //     4295048016u128
    // }
    
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

mod full_math_u64 {

    pub fn mul_div_round(a: u64, b: u64, denom: u64) -> u64 {
        let r = (full_mul(a, b) + (denom as u128 >> 1)) / denom as u128;
        r as u64
    }

    pub fn mul_div_floor(a: u64, b: u64, denm: u64) -> u64 {
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