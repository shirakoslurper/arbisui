use ethnum::U256;

pub struct Pool {
    pub id: ObjectID,
    pub reserve_x: u128,
    pub reserve_y: u128,
    pub protocol_fee: u64,
    pub lp_fee: u64,
    pub scale_x: u64,
    pub scale_y: u64,
    pub unlocked: bool,
}

fn solve_y(
    x_0: U256,
    y_0: U256,
    w: U256,
    x_in: U256,
) -> U256 {
    let x_f = x_0 + x_in;
    let y_f = iterative_search()
}

fn iterative_search(
    x_0: U256,
    x_f: U256,
    y_0: U256,
    w: U256,
    err_tolerance: U256
)

fn f() {
    
}
