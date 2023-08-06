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
            self.reserve_x -= amount_in_after_fee;
            self.reserve_y += amount_out;
        } else {
            self.reserve_x += amount_out;
            self.reserve_y -= amount_in_after_fee;
        }
    }

    // Calculate
    pub fn calc_swap_exact_amount_in(
        &self,
        amount_in: u64,
        x_to_y: bool,
    ) -> u64 {
        let total_fee = self.protocol_fee + self.lp_fee;
    
        let (reserve_in, reserve_out) = if x_to_y {
            (self.reserve_x, self.reserve_y)
        } else {
            (self.reserve_y, self.reserve_x)
        };
    
        get_amount_out(amount_in, reserve_in, reserve_out, total_fee)
    }

}

// Calculate

pub fn get_amount_out(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    fee: u64,
) -> u64 {
    let amount_in_u128 = amount_in as u128;
    let reserve_in_u128 = reserve_in as u128;
    let reserve_out_u128 = reserve_out as u128;
    let fee_u128 = fee as u128;

    let amount_in_with_fee = amount_in_u128 * (1_000_000 - fee_u128);
    let numerator = amount_in_with_fee * reserve_out_u128;
    let denominator = (reserve_in_u128 * 1_000_000) + amount_in_with_fee;

    (numerator / denominator) as u64
}