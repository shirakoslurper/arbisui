use sui_sdk::types::base_types::ObjectID;

#[derive(Clone, Debug)]
pub struct Pool {
    pub id: ObjectID,
    pub reserve_x: u64,
    pub reserve_y: u64,
    pub protocol_fee: u64,
    pub lp_fee: u64,
    pub unlocked: bool,
}


// Mutate
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
        let amount_in_after_fee = (amount_in * (1_000_000 - (self.lp_fee + self.protocol_fee))) / 1_000_000;
        if x_to_y {
            self.reserve_x += amount_in_after_fee;
            self.reserve_y -= amount_out;
        } else {
            self.reserve_x -= amount_out;
            self.reserve_y += amount_in_after_fee;
        }
    }

    pub fn apply_swap(
        &mut self,
        x_to_y: bool,
        amount_in: u64
    ) {
        let (amount_x_delta, amount_y_delta) = self.calc_swap_exact_amount_in(amount_in, x_to_y);
        if x_to_y {
            self.reserve_x += amount_x_delta;
            self.reserve_y -= amount_y_delta
        } else {
            self.reserve_x -= amount_x_delta;
            self.reserve_y += amount_y_delta
        }
    }

    // Calculate
    pub fn calc_swap_exact_amount_in(
        &self,
        amount_in: u64,
        x_to_y: bool,
    ) -> (u64, u64) { // amount_x, amount_y
        let total_fee = self.protocol_fee + self.lp_fee;
    
        let (reserve_in, reserve_out) = if x_to_y {
            (self.reserve_x, self.reserve_y)
        } else {
            (self.reserve_y, self.reserve_x)
        };
    
        let (amount_in_after_fee, amount_out) = get_amount_out(amount_in, reserve_in, reserve_out, total_fee);

        if x_to_y {
            (amount_in_after_fee, amount_out)
        } else {
            (amount_out, amount_in_after_fee)
        }
    }

}

// Calculate

pub fn get_amount_out(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    fee: u64,
) -> (u64, u64) {
    let amount_in_u128 = amount_in as u128;
    let reserve_in_u128 = reserve_in as u128;
    let reserve_out_u128 = reserve_out as u128;
    let fee_u128 = fee as u128;

    let amount_in_after_fee_num = amount_in_u128 * (1_000_000 - fee_u128);
    let numerator = amount_in_after_fee_num * reserve_out_u128;
    let denominator = (reserve_in_u128 * 1_000_000) + amount_in_after_fee_num;
    let amount_in_after_fee= (amount_in_after_fee_num / 1_000_000) as u64;

    let amount_out = (numerator / denominator) as u64;

    (amount_in_after_fee, amount_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_swap() {
        let mut pool = Pool {
            id: ObjectID::from_str("0x0").unwrap(),
            reserve_x: 123456789000000,
            reserve_y: 987654321000000,
            protocol_fee: 1000,
            lp_fee: 2000,
            unlocked: true,
        };

        let k = pool.reserve_x as u128 * pool.reserve_y as u128;
        
        let amount_in = 1000000000;
        // y in. x out.
        pool.apply_swap(true, amount_in);

        let new_k = pool.reserve_x as u128 * pool.reserve_y as u128;

        assert!(k == new_k, "k = {}, new_k = {}", k, new_k);
    }
}