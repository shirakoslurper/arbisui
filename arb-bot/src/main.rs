use custom_sui_sdk::SuiClientBuilder;
use sui_sdk::SUI_COIN_TYPE;

use arb_bot::*;

use anyhow::Context;

use ethnum::I256;

use futures::future;
use sui_sdk::types::object::{Object, self};

use std::cmp;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;
use std::time::Instant;
use std::sync::Arc;

use sui_sdk::rpc_types::{SuiMoveValue, SuiCoinMetadata, SuiObjectResponse, SuiTypeTag};
use sui_sdk::types::base_types::ObjectID;
use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;


use custom_sui_sdk::transaction_builder::{TransactionBuilder, ProgrammableMergeCoinsArg};
use custom_sui_sdk::programmable_transaction_sui_json::ProgrammableTransactionArg;

use move_core_types::language_storage::TypeTag;

use fixed::types::U64F64;

use petgraph::algo::all_simple_paths;

use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;
use nonzero_ext::*;

use rayon::prelude::*;

use crate::sui_sdk_utils;

const CETUS_EXCHANGE_ADDRESS: &str = "0x1eabed72c53feb3805120a081dc15963c204dc8d091542592abaf7a35689b2fb";
const TURBOS_EXCHANGE_ADDRESS: &str = "0x91bfbc386a41afcfd9b2533058d7e915a1d3829089cc268ff4333d54d6339ca1";
const TURBOS_TICK_MAP: &str = "0xd836ea2a159743a568fe29e8f42672a1b88414ab21be5411f8f6331e66b218d3";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let cetus = Cetus::from_str(CETUS_EXCHANGE_ADDRESS)?;
    let turbos = Turbos::from_str(TURBOS_EXCHANGE_ADDRESS)?;

    // 50 Requests / Sec
    let rate_limiter = Arc::new(RateLimiter::direct(Quota::per_second(nonzero!(50u32))));

    let run_data = RunData {
        sui_client: SuiClientBuilder::default()
        .ws_url("wss://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .build(
            "https://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f",
            &rate_limiter
        )
        .await?
    };

    // let exchanges = vec![cetus];
    let base_coin = TypeTag::from_str(SUI_COIN_TYPE)?;
    
    let cetus_markets = cetus.get_all_markets(&run_data.sui_client).await?;
    let turbos_markets = turbos.get_all_markets(&run_data.sui_client).await?;

    let mut markets = vec![];
    markets.extend(turbos_markets.clone());
    markets.extend(cetus_markets.clone());
    // markets.extend(cetus_markets.clone());

    // // filter for viabl
    // markets = markets.into_iter().filter(|market| {


    //     market.viable()
    // {}).collect::<Vec<_>>();

    println!("markets.len(): {}", markets.len());

    // /// TEST
    // // let pool_ids = markets.iter().map(|market| market.pool_id().clone()).collect::<Vec<ObjectID>>();
    // let pool_id_to_object_response = turbos.get_pool_id_to_object_response(&run_data.sui_client, &markets).await?;
    // for (pool_id, object_response) in pool_id_to_object_response.iter() {
    //     println!("{:#?}", turbos.computing_pool_from_object_response(&run_data.sui_client, object_response).await?);
    // }
    // // END TEST


    // TODO: Weigh the costs of duplicate data in markets
    // OR storing coin data in nodes
    // But its for human reading only rly
    let coin_to_metadata = future::try_join_all(
        markets
            .iter()
            .map(|market| {
                async {
                    let mut coin_to_metadata = HashMap::new();

                    if let Some(coin_x_metadata) = run_data.sui_client
                        .coin_read_api()
                        .get_coin_metadata(market.coin_x().to_string()).await? {
                            coin_to_metadata.insert(market.coin_x().clone(), coin_x_metadata);
                        }

                    if let Some(coin_y_metadata) = run_data.sui_client
                        .coin_read_api()
                        .get_coin_metadata(market.coin_y().to_string()).await? {
                            coin_to_metadata.insert(market.coin_y().clone(), coin_y_metadata);
                        }

                    // println!("coin_x_metadata: {:#?}", coin_x_metadata);
                    // println!("coin_y_metadata: {:#?}\n", coin_y_metadata);

                    Ok::<HashMap<TypeTag, SuiCoinMetadata>, anyhow::Error>(coin_to_metadata)
                }
            })
        ).await?
        .into_iter()
        .flatten()
        .collect::<HashMap<TypeTag, SuiCoinMetadata>>();

    let mut market_graph = MarketGraph::new(&markets)?;

    let cetus_pool_id_to_object_response = cetus
        .get_pool_id_to_object_response(&run_data.sui_client, &cetus_markets)
        .await?;

    let turbos_pool_id_to_object_response = turbos
        .get_pool_id_to_object_response(&run_data.sui_client, &turbos_markets)
        .await?;

    let mut pool_id_to_object_response = HashMap::new();
    pool_id_to_object_response.extend(turbos_pool_id_to_object_response);
    pool_id_to_object_response.extend(cetus_pool_id_to_object_response);

    println!("pool_id_to_fields.keys().len(): {}", pool_id_to_object_response.keys().len());

    // let liquidity_filtered = petgraph::visit::EdgeFiltered::from_fn(
    //     &market_graph.graph,
    //     |(_, _, market)| {
    //         market.viable()
    //     }
    // );

    let paths = all_simple_paths(&market_graph.graph, &base_coin, &base_coin, 1, Some(7)).collect::<Vec<Vec<&TypeTag>>>().clone();

    println!("Num cycles paths: {}", paths.len());

    // let cross_exchange_paths = paths
    //     .into_iter()
    //     .filter(|path| {
    //         let mut market_set = HashSet::new();

    //         for pair in path[..].windows(2) {
    //             for market_info in market_graph.graph.edge_weight(pair[0], pair[1]).unwrap() {
    //                 market_set.insert(market_info.market.package_id().clone());
    //             }
    //         }

    //         market_set.len() > 1
    //     })
    //     .collect::<Vec<_>>();

    // println!("Num cycles cross_exchange_paths: {}", cross_exchange_paths.len());

    market_graph.update_markets_with_object_responses(&run_data.sui_client, &pool_id_to_object_response).await?;

    // let mut total_profit = I256::from(0_u8);

    let now = Instant::now();

    let mut optimized_results = paths
        .par_iter()
        .map(|path| {
            arbitrage::optimize_starting_amount_in(path, &market_graph)
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;

    optimized_results = optimized_results
        .into_iter()
        .filter(|optimized_result| optimized_result.profit > 0)
        .collect::<Vec<_>>();
    
    // println!("{:#?}", optimized_results[0]);

    let total_profit = optimized_results
        .iter()
        .fold(I256::from(0u8), |tp, optimized_result| {
            tp + optimized_result.profit
        });

    println!("total_profit: {}", total_profit);

    let most_profitable = optimized_results
        .iter()
        .fold(optimized_results[0].clone(), |max_result, optimized_result| {
            if max_result.profit > optimized_result.profit {
                max_result
            } else {
                optimized_result.clone()
            }
        });

    println!("{:#?}", most_profitable);

    // let transaction_builder = TransactionBuilder::new()
    // let mut pt_builder = ProgrammableTransactionBuilder::new();

    // if most_profitable.amount_in < 10_000_000_000 {
    //     for leg in most_profitable.path {
    //         // Yields SuiRpcResult<Vec<Coin>>
    //         let coins = run_data
    //             .sui_client
    //             .coin_api(
    //             )
    //             .select_coins(
    //                 owner_sui_address,
    //                 Some(format!("0x{}", leg.coin_x)),
    //                 most_profitable.amount_in,
    //                 vec![]
    //             )
    //             .await()?;

    //         let primary_coin = coins[0];

    //         if coins.len() > 1 {
    //             for coin in coins[1..] {
    //                 transaction_builder::programmable_merge_coins(
    //                     &mut pt_builder,
    //                     ProgrammableMergeCoinsArg::CoinObjectID(
    //                         primary_coin.coin_object_id.clone()
    //                     ),
    //                     ProgrammableMergeCoinsArg::CoinObjectID(
    //                         coin.coin_object_id.clone()
    //                     ),
    //                     SuiTypeTag::new(coin[0])
    //                 ).await?;
    //             }
    //         }

    //         // programmable turbos move call
    //         // for now lets make it async so that the interface function 
    //         // gets the clock time for us and we don't have to feed it anything?

    //         // programmable
    //     }
    // }

    let elapsed = now.elapsed();
    println!("Elasped: {:.2?}", elapsed);

    // println!("{:#?}", optimized_results);

    // // The following is all synchronous
    // paths
    //     .iter()
    //     // .filter(|path| {
    //     //     *path[0] == base_coin
    //     // })
    //     .for_each(|path| {
    //         // println!("\nSIMPLE CYCLE ({} HOP) ", path.len() - 1);

    //         let mut best_path_rate = U64F64::from_num(1);

    //         let orig_decimals = coin_to_metadata.get(path[0]).unwrap().decimals as u32;
    //         let orig_amount = 1 * 10_u128.pow(orig_decimals);
    //         let mut amount_in = orig_amount;

    //         for pair in path[..].windows(2) {
    //             let orig = pair[0];
    //             let dest = pair[1];

    //             // Decimals for human readability (rates we would see on exchanges)
    //             let orig_decimals = coin_to_metadata.get(orig).unwrap().decimals as i32;
    //             let dest_decimals = coin_to_metadata.get(dest).unwrap().decimals as i32;

    //             // let ten =  U64F64::from_num(10);
    //             let adj = U64F64::from_num(10_f64.powi(dest_decimals - orig_decimals));

    //             let markets = market_graph
    //                 .graph
    //                 .edge_weight_mut(orig, dest)
    //                 .context("Missing edge weight")
    //                 .unwrap();

    //             // println!("AAAAA markets.len() = {}", markets.);

    //             let directional_rates = markets
    //                 .iter_mut()
    //                 .map(|market_info| {
    //                     let coin_x = market_info.market.coin_x();
    //                     let coin_y = market_info.market.coin_y();

    //                     // println!("coin_x: {:#?}\n coin_y: {:#?}", coin_x, coin_y);
    //                     // println!("orig: {:#?}\n dest: {:#?}", orig, dest);

    //                     if (coin_x, coin_y) == (orig, dest) {
    //                         // println!("AASS {}", market_info.market.coin_x_price().unwrap());
    //                         if market_info.market.viable() {
    //                             // let (_, amount_y) = market_info.market.compute_swap_x_to_y(amount_in);
    //                             // amount_in = amount_y;
    //                             // amount_out = amount_x;
    //                             market_info.market.coin_x_price().unwrap()
    //                         } else {
    //                             // println!("NO LIQUIDITY!!");
    //                             amount_in = 0;
    //                             U64F64::from_num(0)
    //                         }

    //                     } else if (coin_y, coin_x) == (orig, dest){
    //                         // println!("AERE {}", market_info.market.coin_y_price().unwrap());
    //                         if market_info.market.viable() {
    //                             // let (amount_x, _) = market_info.market.compute_swap_y_to_x(amount_in);
    //                             // amount_in = amount_x;
    //                             // amount_out = amount_y;
    //                             market_info.market.coin_y_price().unwrap()
    //                         } else {
    //                             // println!("NO LIQUIDITY!!");
    //                             amount_in = 0;
    //                             U64F64::from_num(0)
    //                         }
    //                     } else {
    //                         // println!("AADFFS");
    //                         panic!("coin pair does not match");
    //                     }
    //                 });

    //             // println!("BBBBB");

    //             // println!("directional_rates: {:#?}", directional_rates);

    //             let best_leg_rate = directional_rates
    //                 .fold(U64F64::from_num(0), |max, current| {
    //                     cmp::max(max, current)
    //                 });

    //             // println!("    {}: {} decimals", orig, orig_decimals);
    //             // println!("    -> {}: {} decimals", dest, dest_decimals);
    //             // // Using decimals for human readability
    //             // println!("        leg rate: {}", best_leg_rate / adj);
    //             best_path_rate = best_path_rate * best_leg_rate;
    //             // println!("        current path_rate: {}", best_path_rate);
    //         }

    //         // println!("PROFIT: {}", I256::from(amount_in) - I256::from(orig_amount));

    //         // println!("{} HOP CYCLE RATE: {}", path.len() - 1, best_path_rate);

    //         // let orig_decimals = coin_to_metadata.get(path[0]).unwrap().decimals as u32;

    //         if best_path_rate > 1 {
    //             let mut left_orig_amount = 10_u128;
    //             let mut right_orig_amount = u64::MAX as u128 - 10;   // amount in for cetus is u64

    //             let mut mid_orig_amount = 0;
    //             let mut profit_mid = I256::from(0);

    //             let max_profit ;
    //             let maximizing_orig_amount;

    //             while left_orig_amount <= right_orig_amount {

    //                 mid_orig_amount = left_orig_amount + ((right_orig_amount - left_orig_amount) / 2);
                    
    //                 let mid_lo_orig_amount = mid_orig_amount - 10;
    //                 let mid_hi_orig_amount = mid_orig_amount + 10;
                    
    //                 let mut mid_amount_in = mid_orig_amount;
    //                 let mut mid_lo_amount_in = mid_lo_orig_amount;
    //                 let mut mid_hi_amount_in = mid_hi_orig_amount;
    //                 // let mut best_path_rate = U64F64::from_num(1);

    //                 // if mid_amount_in == 0 || mid_lo_amount_in == 0 || mid_hi_amount_in == 0 {
    //                 //     break;
    //                 // }

    //                 mid_amount_in = amount_out(&mut market_graph, path, mid_amount_in);
    //                 mid_lo_amount_in = amount_out(&mut market_graph, path, mid_lo_amount_in);
    //                 mid_hi_amount_in = amount_out(&mut market_graph, path, mid_hi_amount_in);
                    
    //                 // Even if convex theres some rounding so similar input amounts can result in the same outputs
    //                 // Not too helpful to have differences of 1 in the orig amounts since the outputs barely change

    //                 // AHHHH we wanna maximize profit NOT amount out

    //                 profit_mid = I256::from(mid_amount_in) - I256::from(mid_orig_amount);
    //                 let profit_mid_lo = I256::from(mid_lo_amount_in) - I256::from(mid_lo_orig_amount);
    //                 let profit_mid_hi = I256::from(mid_hi_amount_in) - I256::from(mid_hi_orig_amount);

    //                 // println!("profit_mid: {}, mid_orig_amount: {}, mid_amount_in: {}", profit_mid, mid_orig_amount, mid_amount_in);
    //                 // println!("profit_mid_hi: {}, mid_hi_orig_amount: {}, mid_hi_amount_in: {}", profit_mid_hi, mid_hi_orig_amount, mid_hi_amount_in);
    //                 // println!("profit_mid_lo: {}, mid_lo_orig_amount: {}, mid_lo_amount_in: {}\n", profit_mid_lo, mid_lo_orig_amount, mid_lo_amount_in);

    //                 if profit_mid > profit_mid_hi && profit_mid > profit_mid_lo {
    //                     // max_profit = profit_mid;
    //                     // maximizing_orig_amount = mid_orig_amount;
    //                     // println!("AAAAHHHH");
    //                     // println!("max_amount_out: {}", mid_amount_in);
    //                     break;
    //                 } else if profit_mid < profit_mid_hi {
    //                     left_orig_amount = mid_orig_amount + 1;
    //                 } else {
    //                     right_orig_amount = mid_orig_amount - 1;
    //                 }

    //             }

    //             max_profit = profit_mid;
    //             maximizing_orig_amount = mid_orig_amount;

    //             if max_profit > 0 {
    //                 println!("\nSIMPLE CYCLE ({} HOP) ", path.len() - 1);
    //                 println!("{} HOP CYCLE RATE: {}", path.len() - 1, best_path_rate);
    //                 println!("max_profit: {}, maximizing_orig_amount: {}", max_profit, maximizing_orig_amount);

    //                 println!("MAX_PROFIT: {}", max_profit);
    //                 println!("\n");

    //                 total_profit += max_profit;
    //             }


    //             // println!("{} HOP CYCLE RATE: {}", path.len() - 1, best_path_rate);

    //             // println!("\n");

    //             // println!("\n");

    //         }
    //     });

    //     paths
    //     .iter()
    //     // .filter(|path| {
    //     //     *path[0] == base_coin
    //     // })
    //     .for_each(|path| {
    //         let path = &path.into_iter().cloned().rev().collect::<Vec<_>>();

    //         // println!("\nSIMPLE CYCLE ({} HOP) ", path.len() - 1);

    //         let mut best_path_rate = U64F64::from_num(1);

    //         let orig_decimals = coin_to_metadata.get(path[0]).unwrap().decimals as u32;
    //         let orig_amount = 1 * 10_u128.pow(orig_decimals);
    //         let mut amount_in = orig_amount;

    //         for pair in path[..].windows(2) {
    //             let orig = pair[0];
    //             let dest = pair[1];

    //             // Decimals for human readability (rates we would see on exchanges)
    //             let orig_decimals = coin_to_metadata.get(orig).unwrap().decimals as i32;
    //             let dest_decimals = coin_to_metadata.get(dest).unwrap().decimals as i32;

    //             // let ten =  U64F64::from_num(10);
    //             let adj = U64F64::from_num(10_f64.powi(dest_decimals - orig_decimals));

    //             let markets = market_graph
    //                 .graph
    //                 .edge_weight_mut(orig, dest)
    //                 .context("Missing edge weight")
    //                 .unwrap();

    //             // println!("AAAAA markets.len() = {}", markets.);

    //             let directional_rates = markets
    //                 .iter_mut()
    //                 .map(|market_info| {
    //                     let coin_x = market_info.market.coin_x();
    //                     let coin_y = market_info.market.coin_y();

    //                     // println!("coin_x: {:#?}\n coin_y: {:#?}", coin_x, coin_y);
    //                     // println!("orig: {:#?}\n dest: {:#?}", orig, dest);

    //                     if (coin_x, coin_y) == (orig, dest) {
    //                         // println!("AASS {}", market_info.market.coin_x_price().unwrap());
    //                         if market_info.market.viable() {
    //                             // let (_, amount_y) = market_info.market.compute_swap_x_to_y(amount_in);
    //                             // amount_in = amount_y;
    //                             // amount_out = amount_x;
    //                             market_info.market.coin_x_price().unwrap()
    //                         } else {
    //                             // println!("NO LIQUIDITY!!");
    //                             amount_in = 0;
    //                             U64F64::from_num(0)
    //                         }

    //                     } else if (coin_y, coin_x) == (orig, dest){
    //                         // println!("AERE {}", market_info.market.coin_y_price().unwrap());
    //                         if market_info.market.viable() {
    //                             // let (amount_x, _) = market_info.market.compute_swap_y_to_x(amount_in);
    //                             // amount_in = amount_x;
    //                             // amount_out = amount_y;
    //                             market_info.market.coin_y_price().unwrap()
    //                         } else {
    //                             // println!("NO LIQUIDITY!!");
    //                             amount_in = 0;
    //                             U64F64::from_num(0)
    //                         }
    //                     } else {
    //                         // println!("AADFFS");
    //                         panic!("coin pair does not match");
    //                     }
    //                 });

    //             // println!("BBBBB");

    //             // println!("directional_rates: {:#?}", directional_rates);

    //             let best_leg_rate = directional_rates
    //                 .fold(U64F64::from_num(0), |max, current| {
    //                     cmp::max(max, current)
    //                 });

    //             // println!("    {}: {} decimals", orig, orig_decimals);
    //             // println!("    -> {}: {} decimals", dest, dest_decimals);
    //             // // Using decimals for human readability
    //             // println!("        leg rate: {}", best_leg_rate / adj);
    //             best_path_rate = best_path_rate * best_leg_rate;
    //             // println!("        current path_rate: {}", best_path_rate);
    //         }

    //         // println!("PROFIT: {}", I256::from(amount_in) - I256::from(orig_amount));

    //         // println!("{} HOP CYCLE RATE: {}", path.len() - 1, best_path_rate);

    //         // let orig_decimals = coin_to_metadata.get(path[0]).unwrap().decimals as u32;

    //         if best_path_rate > 1 {
    //             let mut left_orig_amount = 10_u128;
    //             let mut right_orig_amount = u64::MAX as u128 - 10;   // amount in for cetus is u64

    //             let mut mid_orig_amount = 0;
    //             let mut profit_mid = I256::from(0);

    //             let max_profit ;
    //             let maximizing_orig_amount;

    //             while left_orig_amount <= right_orig_amount {

    //                 mid_orig_amount = left_orig_amount + ((right_orig_amount - left_orig_amount) / 2);
                    
    //                 let mid_lo_orig_amount = mid_orig_amount - 10;
    //                 let mid_hi_orig_amount = mid_orig_amount + 10;
                    
    //                 let mut mid_amount_in = mid_orig_amount;
    //                 let mut mid_lo_amount_in = mid_lo_orig_amount;
    //                 let mut mid_hi_amount_in = mid_hi_orig_amount;
    //                 // let mut best_path_rate = U64F64::from_num(1);

    //                 // if mid_amount_in == 0 || mid_lo_amount_in == 0 || mid_hi_amount_in == 0 {
    //                 //     break;
    //                 // }

    //                 mid_amount_in = amount_out(&mut market_graph, path, mid_amount_in);
    //                 mid_lo_amount_in = amount_out(&mut market_graph, path, mid_lo_amount_in);
    //                 mid_hi_amount_in = amount_out(&mut market_graph, path, mid_hi_amount_in);
                    
    //                 // Even if convex theres some rounding so similar input amounts can result in the same outputs
    //                 // Not too helpful to have differences of 1 in the orig amounts since the outputs barely change

    //                 // AHHHH we wanna maximize profit NOT amount out

    //                 profit_mid = I256::from(mid_amount_in) - I256::from(mid_orig_amount);
    //                 let profit_mid_lo = I256::from(mid_lo_amount_in) - I256::from(mid_lo_orig_amount);
    //                 let profit_mid_hi = I256::from(mid_hi_amount_in) - I256::from(mid_hi_orig_amount);

    //                 // println!("profit_mid: {}, mid_orig_amount: {}, mid_amount_in: {}", profit_mid, mid_orig_amount, mid_amount_in);
    //                 // println!("profit_mid_hi: {}, mid_hi_orig_amount: {}, mid_hi_amount_in: {}", profit_mid_hi, mid_hi_orig_amount, mid_hi_amount_in);
    //                 // println!("profit_mid_lo: {}, mid_lo_orig_amount: {}, mid_lo_amount_in: {}\n", profit_mid_lo, mid_lo_orig_amount, mid_lo_amount_in);

    //                 if profit_mid > profit_mid_hi && profit_mid > profit_mid_lo {
    //                     // max_profit = profit_mid;
    //                     // maximizing_orig_amount = mid_orig_amount;
    //                     // println!("AAAAHHHH");
    //                     // println!("max_amount_out: {}", mid_amount_sin);
    //                     break;
    //                 } else if profit_mid < profit_mid_hi {
    //                     left_orig_amount = mid_orig_amount + 1;
    //                 } else {
    //                     right_orig_amount = mid_orig_amount - 1;
    //                 }

    //             }

    //             max_profit = profit_mid;
    //             maximizing_orig_amount = mid_orig_amount;

    //             if max_profit > 0 {
    //                 println!("\nSIMPLE CYCLE ({} HOP) ", path.len() - 1);
    //                 println!("{} HOP CYCLE RATE: {}", path.len() - 1, best_path_rate);
    //                 println!("max_profit: {}, maximizing_orig_amount: {}", max_profit, maximizing_orig_amount);

    //                 println!("MAX_PROFIT: {}", max_profit);

    //                 // println!("Path")
    //                 println!("\n");

    //                 total_profit += max_profit;
    //             }

    //             // println!("{} HOP CYCLE RATE: {}", path.len() - 1, best_path_rate);

    //             // println!("\n");
    //         }
    //     });

    //     // let elapsed = now.elapsed();
    //     // println!("Elasped: {:.2?}", elapsed);
    
    // // loop_blocks(run_data, vec![&flameswap]).await?;

    // println!("Total Profit: {}", total_profit);

    Ok(())
}