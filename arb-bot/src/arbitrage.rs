//hello

use anyhow::{anyhow, Context};

use ethnum::I256;

use move_core_types::language_storage::TypeTag;

use std::fmt::{Debug, Error, Formatter};

use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;

use crate::markets::Market;
use crate::market_graph::MarketGraph;

#[derive(Debug, Clone)]
pub struct OptimizedResult<'a> {
    pub path: Vec<DirectedLeg<'a>>,
    pub amount_in: u128,
    pub amount_out: u128,
    pub profit: I256
}

#[derive(Clone)]
pub struct DirectedLeg<'a> {
    orig: &'a TypeTag,
    dest: &'a TypeTag,
    market: &'a Box<dyn Market>
}

impl<'a> Debug for DirectedLeg<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
         f
        .debug_struct("DirectedLeg")
        .field("orig", &self.orig)
        .field("dest", &self.dest)
        .field("market.coin_x()", &self.market.coin_x())
        .field("market.coin_y()", &self.market.coin_y())
        .field("market.coin_x_price()", &self.market.coin_x_price())
        .field("market.coin_y_price()", &self.market.coin_y_price())
        .field("market.pool_id()", &self.market.pool_id())
        .finish()
    }
}

// pub fn search() {

// } -> Result<Optimized, Result >

// For a single path
// Objective is maximizing profit
pub fn optimize_starting_amount_in<'a>(
    path: &[&'a TypeTag], 
    market_graph: &'a MarketGraph<'a>
) -> Result<OptimizedResult<'a>, anyhow::Error> {

    // let expanded_paths = 
    let mut starting_amount_in = 0;
    let mut delta = 0;

    let mut expanded_paths = Vec::<Vec::<DirectedLeg>>::new();
    // println!("Expanded paths: {:#?}", expanded_paths);
    expanded_paths.push(vec![]);
    // println!("Expanded paths: {:#?}", expanded_paths);

    for pair in path[..].windows(2) {
        let orig = pair[0];
        let dest = pair[1];

        let orig_to_dest_markets = market_graph
            .graph
            .edge_weight(orig, dest)
            .unwrap();

        let mut expanded_paths_extended = Vec::<Vec::<DirectedLeg>>::new();

        for expanded_path in expanded_paths {
            for market_info in orig_to_dest_markets.iter() {
                let mut expanded_path_extended = expanded_path
                    .clone();

                expanded_path_extended.push(
                        DirectedLeg {
                            orig,
                            dest,
                            market: &market_info.market
                        }
                    );

                expanded_paths_extended.push(expanded_path_extended);
            }
        }

        expanded_paths = expanded_paths_extended;
    }

    // println!("Expanded paths: {:#?}", expanded_paths);

    // Golden section search:
    // - for unimodal functions
    // - does not get caught in local extrema

    let mut optimized_results = Vec::new();

    let gr_num = 121393u128;
    let gr_den = 75025u128;

    for expanded_path in expanded_paths {
        let mut a = 0u128;
        let mut b = u64::MAX as u128;

        let mut c = b - (((b - a) * gr_den) / gr_num);
        let mut d = a + (((b - a) * gr_den) / gr_num);

        if b < a {
            println!("b: {}, a: {}", b , a);
        }

        while (I256::from(b) - I256::from(a)).abs() > 1 {
            let amount_out_c = amount_out(&expanded_path, c)?;
            let amount_out_d = amount_out(&expanded_path, d)?;
            let profit_c = I256::from(amount_out_c) - I256::from(c);
            let profit_d = I256::from(amount_out_d) - I256::from(d);

            if profit_c > profit_d {
                b = d;
            } else {
                a = c;
            }

            c = b - (((b - a) * gr_den) / gr_num);
            d = a + (((b - a) * gr_den) / gr_num);
        }

        let optimized_amount_in = (b + a) / 2;
        let optimized_amount_out = amount_out(&expanded_path, optimized_amount_in)?;
        let optimized_profit = I256::from(optimized_amount_out) - I256::from(optimized_amount_in);

        optimized_results.push(
            OptimizedResult{
                path: expanded_path,
                amount_in: optimized_amount_in,
                amount_out: optimized_amount_out,
                profit: optimized_profit
            }
        )
    }
    
    // println!("optimized_results: {:#?}", optimized_results);

    let first_optimized_result = optimized_results.pop().context("optimized_results is empty")?;

    let profit_maximized_result = optimized_results
        .into_iter()
        .fold(
            first_optimized_result,
            |pmr, optimized_result| {
                if pmr.profit > optimized_result.profit {
                    pmr
                } else {
                    optimized_result
                }
            }
        );

    // if profit_maximized_result.profit > 0 {
        // println!("{} HOP: max_profit = {}", profit_maximized_result.path.len(), profit_maximized_result.profit);
    // }

    Ok(profit_maximized_result)
}

pub fn amount_out(path: &[DirectedLeg], mut amount_in: u128) -> Result<u128, anyhow::Error> {

    for leg in path {
        let coin_x = leg.market.coin_x();
        let coin_y = leg.market.coin_y();

        if (coin_x, coin_y) == (leg.orig, leg.dest) {
            if leg.market.viable() {
                if amount_in == 0 {
                    return Ok(0);
                }

                let (amount_x, amount_y) = leg.market.compute_swap_x_to_y(amount_in);
                amount_in = amount_y;
            } else {
                amount_in = 0;
            }

        } else if (coin_y, coin_x) == (leg.orig, leg.dest){
            if leg.market.viable() {
                if amount_in == 0 {
                    return Ok(0);
                }

                let (amount_x, amount_y) = leg.market.compute_swap_y_to_x(amount_in);
                amount_in = amount_x;
            } else {
                amount_in = 0;
            }
        } else {
            return Err(anyhow!("amount_out(): coin pair does not match"));
        }
    }

    Ok(amount_in)
}

// pub fn build_programmable_transaction_block(path: &[DirectedLeg]) {
//     let mut pt_builder = ProgrammableTransactionBuilder::new();

//     // for leg in path {
//     //     pt_builder.programmable_move_call(
//     //         leg.market.package_id().clone(),
//     //         module_identifier,
//     //         function_identifier,
//     //         type_arguments
//     //     )
//     // }

// }

// Selecting which markets 