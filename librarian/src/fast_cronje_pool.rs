use std::iter;

use ethnum::U256;
use sui_sdk::types::base_types::ObjectID;

// Hmm we can use a generic for the ID field lmwo

#[derive(Clone, Debug)]
pub struct Pool {
    pub id: ObjectID,
    pub reserve_x: u64,
    pub reserve_y: u64,
    pub protocol_fee: u64,
    pub lp_fee: u64,
    pub scale_x: u64,
    pub scale_y: u64,
    pub unlocked: bool,
}

impl Pool {
    pub fn update_lp_fee(
        &mut self,
        lp_fee: u64
    ) {
        self.lp_fee = lp_fee;
    }

    pub fn update_protocol_fee(
        &mut self,
        protocol_fee: u64
    ) {
        self.protocol_fee = protocol_fee;
    }
    
    pub fn update_unlocked(
        &mut self,
        unlocked: bool
    ) {
        self.unlocked = unlocked
    }

    pub fn apply_add_liquidity_effects(
        &mut self,
        amount_x: u64,
        amount_y: u64
    ) {
        self.reserve_x += amount_x;
        self.reserve_y += amount_y;
    }

    pub fn apply_remove_liquidity_effects(
        &mut self,
        amount_x: u64,
        amount_y: u64
    ) {
        self.reserve_x -= amount_x;
        self.reserve_y -= amount_y;
    }

    // The fee must be taken out of the amount in
    pub fn apply_swap_effects(
        &mut self,
        x_to_y: bool,
        amount_in: u64,
        amount_out: u64,
    ) {
        let factor_fee = 1_000_000_u128;

        let amount_in_u128 = amount_in as u128;
        let protocol_fee_u128 = self.protocol_fee as u128;
        let lp_fee_u128 = self.lp_fee as u128;

        let amount_in_after_protocol_fee = (amount_in_u128 * (factor_fee - protocol_fee_u128)) / factor_fee;
        // let total_fees = self.protocol_fee + self.lp_fee;
        let amount_in_after_fees = ((amount_in_after_protocol_fee * (factor_fee - lp_fee_u128)) / factor_fee) as u64;

        if x_to_y {
            self.reserve_x += amount_in_after_fees;
            self.reserve_y -= amount_out;
        } else {
            self.reserve_x -= amount_out;
            self.reserve_y += amount_in_after_fees;
        }
    }

    pub fn apply_swap(
        &mut self,
        amount_in: u64,
        x_to_y: bool
    ) {
        let (delta_x, delta_y) = self.calc_swap_exact_amount_in(amount_in, x_to_y);

        if x_to_y {
            self.reserve_x += delta_x;
            assert!(self.reserve_x >= delta_x, "self.reserve_y >= delta_y: self.reserve_y = {}, delta_y = {}", self.reserve_y, delta_y);
            self.reserve_y -= delta_y;
        } else {
            assert!(self.reserve_x >= delta_x, "self.reserve_x >= delta_x: self.reserve_x = {}, delta_x = {}", self.reserve_x, delta_x);
            self.reserve_x -= delta_x;
            self.reserve_y += delta_y;
        }
    }

    pub fn calc_swap_exact_amount_in(
        &self,
        amount_in: u64,
        x_to_y: bool
    ) -> (u64, u64) {   // returns delta_x and delta_y (as applied to the pool reserves)
        let factor_fee = 1_000_000_u128;

        let (reserve_in, reserve_out) = if x_to_y {
            (self.reserve_x, self.reserve_y)
        } else {
            (self.reserve_y, self.reserve_x)
        };

        let (scale_in, scale_out) = if x_to_y {
            (self.scale_x, self.scale_y)
        } else {
            (self.scale_y, self.scale_x)
        };

        let amount_in_u128 = amount_in as u128;
        let protocol_fee_u128 = self.protocol_fee as u128;
        let lp_fee_u128 = self.lp_fee as u128;

        // In kriya stableswap protocol fees are taken first out of the amount in, then 
        // Lp fees are taken out of the remaining amount
        let amount_in_after_protocol_fee = (amount_in_u128 * (factor_fee - protocol_fee_u128)) / factor_fee;
        // let total_fees = self.protocol_fee + self.lp_fee;
        let amount_in_after_fees = ((amount_in_after_protocol_fee * (factor_fee - lp_fee_u128)) / factor_fee) as u64;

        let amount_out = get_amount_out(
            amount_in_after_protocol_fee as u64, 
            reserve_in, 
            reserve_out, 
            self.lp_fee, 
            scale_in, 
            scale_out
        );

        if x_to_y {
            (amount_in_after_fees, amount_out)
        } else {
            (amount_out, amount_in_after_fees)
        }
    }
}

fn get_amount_out(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    lp_fee_pct: u64,
    scale_in: u64,
    scale_out: u64
) -> u64 {
    let factor_scale = U256::from(100_000_000_u128);
    let factor_fee = 1_000_000_u128;

    let target_k = lp_value(
        reserve_in as u128,
        scale_in,
        reserve_out as u128,
        scale_out
    );

    let amount_in_u256 = U256::from(amount_in);
    let reserve_in_u256 = U256::from(reserve_in);
    let scale_in_u256 = U256::from(scale_in);
    let reserve_out_u256 = U256::from(reserve_out);
    let scale_out_u256 = U256::from(scale_out);

    let scaled_reserve_in_initial = (reserve_in_u256 * factor_scale) / scale_in_u256;
    let scaled_reserve_out_initial = (reserve_out_u256 * factor_scale) / scale_out_u256;

    let fee_num = U256::from(factor_fee - lp_fee_pct as u128);

    let scaled_amount_in = (amount_in_u256 * factor_scale) / scale_in_u256;

    let scaled_amount_in_after_fee = (scaled_amount_in * (fee_num)) / factor_fee;

    let scaled_reserve_in_final = scaled_reserve_in_initial + scaled_amount_in_after_fee;

    let scaled_reserve_out_final = get_y(
        scaled_reserve_in_final,    // x_f
        target_k,                   // k_target
        scaled_reserve_out_initial  // y_0
    );

    // Solved reserve out final should be smaller than the initial reserve out
    assert!(scaled_reserve_out_initial >= scaled_reserve_out_final);
    let scaled_reserve_out_delta = scaled_reserve_out_initial - scaled_reserve_out_final;
    let descaled_reserve_out_delta = (scaled_reserve_out_delta * scale_out_u256) / factor_scale;

    descaled_reserve_out_delta.as_u64()
}

fn lp_value(
    reserve_in: u128,
    scale_in: u64,
    reserve_out: u128,
    scale_out: u64
) -> U256 {
    
    let factor_scale = U256::from(100_000_000_u128);

    let reserve_in_u256 = U256::from(reserve_in);
    let reserve_out_u256 = U256::from(reserve_out);
    let scale_in_u256 = U256::from(scale_in);
    let scale_out_u256 = U256::from(scale_out);

    let scaled_reserve_in = (reserve_in_u256 * factor_scale) / scale_in_u256;
    let scaled_reserve_out = (reserve_out_u256 * factor_scale) / scale_out_u256;

    let first_term = scaled_reserve_in * scaled_reserve_out;
    let second_term = (scaled_reserve_in * scaled_reserve_in) + (scaled_reserve_out * scaled_reserve_out);

    first_term * second_term
}

// Iterative search for a y_f

fn get_y(
    x_f: U256,
    target_k: U256,// Fixed
    y_0: U256, 
) -> U256 {
    let mut y = y_0;

    let mut i = 0;
    let one_u256 = U256::from(1_u8);

    let mut steps = 0;

    while i < 255 {
        steps += 1;

        let iter_k = f(x_f, y);

        let step;
        
        if target_k > iter_k {
            assert!(target_k >= iter_k);
            step = ((target_k - iter_k) / d(x_f, y)) + one_u256;
            y += step;
        } else {
            assert!(iter_k >= target_k);
            step = ((iter_k - target_k) / d(x_f, y)) + one_u256;
            assert!(y >= step, "y >= step: y = {}, step = {}, steps: {}", y, step, steps);
            y -= step;
        }

        if step <= one_u256 {
            return y;
        }

        i += 1;
    }

    y
}

fn f(
    x: U256,
    y: U256
) -> U256 {
    (x * x * x * y) + (x * y * y * y)
}

// 3xy^2 + x^3 - the derivative
fn d(
    x: U256,
    y: U256
) -> U256 {
    (3 * x * y * y) + (x * x * x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_swap() {
        let mut pool = Pool {
            id: ObjectID::from_str("0x0").unwrap(),
            reserve_x: 100087381436,
            reserve_y: 100087381436,
            scale_x: 1000000,
            scale_y: 1000000,
            protocol_fee: 667,
            lp_fee: 333,
            unlocked: true,
        };
        
        let lp_value_initial = lp_value(
            pool.reserve_x as u128, 
            pool.scale_x, 
            pool.reserve_y as u128, 
            pool.scale_y
        );

        // let amount_in = 100000000u64;
        let amount_in = u64::MAX;
        // let amount_in = u32::MAX as u64;
        // y in. x out.

        let (real_amount_in, amount_out) = pool.calc_swap_exact_amount_in(amount_in, true);

        pool.apply_swap(amount_in, true);

        let lp_value_final = lp_value(
            pool.reserve_x as u128, 
            pool.scale_x, 
            pool.reserve_y as u128, 
            pool.scale_y
        );

        assert!(lp_value_initial == lp_value_final, "lp_value_initial = {}, lp_value_final = {}, real_amount_in = {}, amount_out = {}", lp_value_initial, lp_value_final, real_amount_in, amount_out);
    }
}