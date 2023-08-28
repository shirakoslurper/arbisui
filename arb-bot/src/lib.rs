// #![feature(async_fn_in_trait)]

use anyhow::{anyhow, Context, Result};
use fixed::consts::E;
use sui_sdk::rpc_types::EventFilter;

use clap::Parser;

use custom_sui_sdk::SuiClient;
// use sui_sdk::wallet_context::WalletContext;

use ethnum::I256;

use futures::{StreamExt, future, FutureExt};

use move_core_types::language_storage::{TypeTag, StructTag};

use rayon::prelude::*;

use serde_json::Value;
use sui_sdk::rpc_types::SuiObjectResponse;

// use std::task::{Context, Poll};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Instant, Duration};
use std::collections::HashSet;
use std::sync::mpsc;
use std::thread;

use sui_keys::keystore::{Keystore, AccountKeystore};
use sui_sdk::types::object::Object;
use sui_sdk::sui_client_config;
use sui_sdk::rpc_types::SuiObjectDataOptions;
use sui_sdk::types::base_types::ObjectID;

pub mod markets;
pub mod market_graph;
pub mod cetus;
pub mod kriyadex;
pub mod turbos;
pub mod constants;
pub mod sui_sdk_utils;
pub mod sui_json_utils;
pub mod turbos_pool;
// pub mod cetus_pool; 
pub mod arbitrage;
pub mod fast_v2_pool;
pub mod fast_v3_pool;
pub mod fast_cronje_pool;
pub use crate::markets::*;
pub use crate::market_graph::*;
pub use crate::cetus::*;
pub use crate::turbos::*;
pub use crate::kriyadex::*;

#[derive(Parser)]
#[clap(
    name = "arb-bot",
    about = "hopefully he makes money",
    rename_all = "kebab-case"
)]
pub struct RunDataOpts {
    #[clap(long, default_value = "wss://sui-mainnet.blastapi.io:443/338f3a96-cd39-41f8-88d9-4faefe2eee21")]
    pub wss_url: String,
    #[clap(long, default_value = "https://sui-mainnet.blastapi.io:443/338f3a96-cd39-41f8-88d9-4faefe2eee21")]
    pub rpc_url: String,
    #[clap(long)]
    pub keystore_path: PathBuf,
    #[clap(long)]
    pub key_index: usize, 
    #[clap(long)]
    pub max_intermediate_nodes: usize, 
}

pub struct RunData {
    pub sui_client: SuiClient,
    pub keystore: Keystore,
    pub key_index: usize,
}

pub async fn loop_blocks<'a>(
    run_data: &RunData, 
    exchanges: &Vec<Box<dyn Exchange>>, 
    market_graph: &mut MarketGraph<'a>,
    source_coin: &TypeTag
    // paths: Vec<Vec<&TypeTag>>
) -> Result<()> {

    println!("Addresses: {:#?}", run_data.keystore.addresses());
    // panic!();

    let owner_address = run_data
        .keystore
        .addresses()
        .get(run_data.key_index)
        .context(format!("No address for key index {} in keystore", run_data.key_index))?
        .clone();
        // [run_data.key_index];
        // .context(format!("No address for key index {} in keystore", run_data.key_index))?;

    let run_starting_balance = run_data
        .sui_client
        .coin_read_api()
        .get_balance(
            owner_address.clone(),
            Some(format!("{}", source_coin))
        )
        .await?;

    println!("RUN STARTING BALANCE: {}", run_starting_balance.total_balance);

    let pool_state_changing_event_filters = exchanges
        .iter()
        .flat_map(|exchange| {
            exchange.event_filters()
        })
        .collect::<Vec<EventFilter>>();

    // println!("pool_state_changing_event_filters: {:#?}", pool_state_changing_event_filters);

    let mut subscribe_pool_state_changing_events = run_data
        .sui_client
        .event_api()
        .subscribe_event(
            EventFilter::Any(
                pool_state_changing_event_filters
            )
        )
        .await?;

    let event_struct_tag_to_pool_field = exchanges
        .iter()
        .flat_map(|exchange| {
            exchange.event_struct_tag_to_pool_field()
        })
        .collect::<HashMap<_, _>>();

    // Very hacky skip
    let cetus_sui_pool_id = ObjectID::from_str("0x2e041f3fd93646dcc877f783c1f2b7fa62d30271bdef1f21ef002cebf857bded")?;
    let cetus_usdc_pool_id = ObjectID::from_str("0x5eb2dfcdd1b15d2021328258f6d5ec081e9a0cdcfa9e13a0eaeb9b5f7505ca78")?;
    let another_pool = ObjectID::from_str("0xcf994611fd4c48e277ce3ffd4d4364c914af2c3cbb05f7bf6facd371de688630")?;
    let otra_pool = ObjectID::from_str("0x5eb2dfcdd1b15d2021328258f6d5ec081e9a0cdcfa9e13a0eaeb9b5f7505ca78")?;
    // let mut pool_set: HashSet<ObjectID> = HashSet::new();
    let ignor_too = ObjectID::from_str("0x238f7e4648e62751de29c982cbf639b4225547c31db7bd866982d7d56fc2c7a8")?;
    let delulu = ObjectID::from_str("0x79d2d20005eb8f8ad6f18008a757db50ca75d57d1aa7bcef1052e55a20b37b28")?;
    let poro = ObjectID::from_str("0x9ddb0d269d1049caf7c872846cc6d9152618d1d3ce994fae84c1c051ee23b179")?;

    let mut skip_event_pools: HashSet<ObjectID> = HashSet::new();
    // skip_event_pools.insert(cetus_sui_pool_id);
    // skip_event_pools.insert(another_pool);
    // skip_event_pools.insert(otra_pool);
    // skip_event_pools.insert(ignor_too);
    // skip_event_pools.insert(delulu);
    // skip_event_pools.insert(poro);
    // skip_event_pools.insert(cetus_usdc_pool_id);
    // skip_event_pools.insert(ObjectID::from_str("0x2c6fc12bf0d093b5391e7c0fed7e044d52bc14eb29f6352a3fb358e33e80729e")?);
    skip_event_pools.insert(ObjectID::from_str("0xd0086b7713e0487bbf5bb4a1e30a000794e570590a6041155cdbebee3cb1cb77")?);
    skip_event_pools.insert(ObjectID::from_str("0x5af4976b871fa1813362f352fa4cada3883a96191bb7212db1bd5d13685ae305")?); 
    // skip_event_pools.insert(ObjectID::from_str("0x517ee525c34fdfb2240342bd43fc07e1ec253c2442a7edd2482e6973700c6ef5")?);
    // skip_event_pools.insert(ObjectID::from_str("0x238f7e4648e62751de29c982cbf639b4225547c31db7bd866982d7d56fc2c7a8")?);
    // skip_event_pools.insert(ObjectID::from_str("0x06d8af9e6afd27262db436f0d37b304a041f710c3ea1fa4c3a9bab36b3569ad3")?);
    // skip_event_pools.insert(ObjectID::from_str("0x238f7e4648e62751de29c982cbf639b4225547c31db7bd866982d7d56fc2c7a8")?);

    // let poot = ObjectID::from_str("0x86ed41e9b4c6cce36de4970cfd4ae3e98d6281f13a1b16aa31fc73ec90079c3d")?;

    let mut last_seen_pool = cetus_sui_pool_id;
    // let mut focus_pool: Option<ObjectID>;

    // let excute_pool = ;

    // let cycles_opt = market_graph
    //     .pool_id_and_source_coin_to_cycles
    //     .get(&(execute_pool, source_coin.clone()));
    //     // .context(format!("No cycles for (pool_id, source_coin): ({}, {})", pool_id, source_coin))?

    // let cycles = cycles_opt
    //     .unwrap()
    //     .iter()
    //     .map(|vec_coins|{
    //         vec_coins
    //             .iter()
    //             .map(|coin| {
    //                 (*coin).clone()
    //             })
    //             .collect::<Vec<TypeTag>>()
    //     })
    //     .collect::<Vec<Vec<TypeTag>>>();

    // println!("num cycles: {}", cycles.len());

    // let mut pool_ids_to_update = HashSet::new();
    
    // // Update pool involved in the cycle
    // // PREVIOUS ARBS WILL EMIT EVENTS
    // // BUT WILL HAVE AFFECTED MULTIPLE POOLS
    // // So we can't just update the pool for which the vent was emitted
    // // We have to update all pools involved in the cycle
    // // TODO: We can make this more efficient by only updating pools that
    // // were involved in ther previous trade
    // for cycle in cycles.iter() {
    //     for pair in cycle[..].windows(2) {
    //         let coin_a = &pair[0];
    //         let coin_b = &pair[1];

    //         let pool_ids = market_graph
    //             .graph
    //             .edge_weight(&coin_a, &coin_b)
    //             .context(format!("Missing markets for pair ({}, {})", coin_a, coin_b))?
    //             .iter()
    //             .map(|(pool_id, _)| {
    //                 pool_id.clone()
    //             })
    //             .collect::<Vec<ObjectID>>();

    //         for pool_id in pool_ids {
    //             pool_ids_to_update.insert(pool_id);
    //         }
    //     }
    // }

    // let pool_ids_to_update_vec = pool_ids_to_update.into_iter().collect::<Vec<ObjectID>>();

    // println!("Measuring... (updating {} markets)", pool_ids_to_update_vec.len());
    // let now = Instant::now();

    // let pool_id_to_object_response = sui_sdk_utils::get_object_id_to_object_response(
    //     &run_data.sui_client, 
    //     &pool_ids_to_update_vec
    // ).await?;
    // println!("pool_id_to_object_response elapsed: {:#?} for {} object responeses", now.elapsed(), pool_id_to_object_response.len());

    // market_graph.update_markets_with_object_responses(
    //     &run_data.sui_client, 
    //     &pool_id_to_object_response
    // ).await?;

    // println!("update_markets_with_object_responses elapsed: {:#?}", now.elapsed());

    // let mut optimized_results = cycles
    //     .par_iter()
    //     .map(|cycle| {
    //         arbitrage::optimize_starting_amount_in(cycle, &market_graph)
    //     })
    //     .collect::<Result<Vec<_>, anyhow::Error>>()?;

    // let mut used_legs_set = HashSet::new();

    // // Good future pattern may be an execute all trades func
    // // That takes an ordering and filtering function as args

    // // Sort by most profitable so that the later excluded trades
    // // are the less profitable ones. Sort in descending order.
    // optimized_results.sort_by(|a, b| {
    //     b.profit.cmp(&a.profit)
    // });

    // // Exclude less profitable trades whose legs
    // // were already seen before. Don't want any strange effects.
    // // Later we can account for the effects of our trades and whatnot.
    // // We should also not swap back through the same pool we came
    // // Since we haven't accounted for the effects of our trade on
    // // the pool.
    // // RESULTS DEPEND ON ORDER

    // let filtered_optimized_results_iter = optimized_results
    //     .into_iter()
    //     .filter(|optimized_result| {
    //         println!("profit: {}", optimized_result.profit);

    //         if optimized_result.profit < I256::from(7_000_000u128 * optimized_result.path.len() as u128) {
    //             return false
    //         }  // Should do some gas threshold instead
    //         for leg in &optimized_result.path {
    //             if used_legs_set.contains(leg.market.pool_id()) {
    //                 return false;
    //             } else {
    //                 used_legs_set.insert(leg.market.pool_id());
    //             }
    //         }

    //         true
    //     });

    // for mut optimized_result in filtered_optimized_results_iter {

    //     let start_source_coin_balance = run_data
    //         .sui_client
    //         .coin_read_api()
    //         .get_balance(
    //             owner_address.clone(),
    //             Some(format!("{}", source_coin))
    //         )
    //         .await?;

    //     let allowance = (start_source_coin_balance.total_balance * 4) / 5;

    //     // Adjust and check profitibility or skip
    //     if optimized_result.amount_in > allowance  {
    //         println!("profitable optimized result amount_in: {}", optimized_result.amount_in);
    //         // Skip so that we don't fail
    //         // optimized_result.amount_in = start_source_coin_balance.total_balance / 2;
    //         // panic!();
    //         // continue;

    //         let amount_in = allowance;
    //         let amount_out = arbitrage::amount_out(&optimized_result.path, allowance)?;
    //         let profit = I256::from(amount_out) - I256::from(amount_in);

    //         if profit > I256::from(10_000_000u128 * optimized_result.path.len() as u128) {
    //             optimized_result.amount_in = amount_in;
    //             optimized_result.amount_out = amount_out;
    //             optimized_result.profit = profit;

    //             // focus_pool = Some(pool_id);
    //         } else {
    //             // focus_pool = None;
    //             continue;
    //         }
    //     }

    //     println!("+-----------------------------------------------------");
    //     println!("| START BALANCE: {}", start_source_coin_balance.total_balance);
    //     println!("| AMOUNT IN: {} {}", optimized_result.amount_in, source_coin);
    //     println!("| AMOUNT OUT: {} {}", optimized_result.amount_out, source_coin);
    //     println!("| RAW PROFIT: {} {}", optimized_result.profit, source_coin);
    //     optimized_result.path
    //         .iter()
    //         .try_for_each(|leg| {
    //             if leg.x_to_y {
    //                 println!("|    +----[POOL: {}, X_TO_Y: {}]-------------", leg.market.pool_id(), leg.x_to_y);
    //                 println!("|    | {}", leg.market.coin_x());
    //                 println!("|    |   ----[RATE: {}]---->", leg.market.coin_y_price().context("Missing coin_y price.")?);
    //                 println!("|    | {}", leg.market.coin_y());
    //                 // println!("|    +------------------------------------------------");
    //             } else {
    //                 println!("|    +----[POOL: {}, X_TO_Y: {}]-------------", leg.market.pool_id(), leg.x_to_y);
    //                 println!("|    | {}", leg.market.coin_y());
    //                 println!("|    |   ----[RATE: {}]---->", leg.market.coin_x_price().context("Missing coin_x price.")?);
    //                 println!("|    | {}", leg.market.coin_x());
    //                 // println!("|    +------------------------------------------------");
    //             }

    //             Ok::<(), anyhow::Error>(())
    //         })?;

    //     println!("|    +------------------------------------------------");

    //     // panic!();

    //     arbitrage::execute_arb(
    //         &run_data.sui_client,
    //         optimized_result,
    //         run_data
    //             .keystore
    //             .addresses()
    //             .get(run_data.key_index)
    //             .context(format!("No address for key index {} in keystore", run_data.key_index))?,
    //         &run_data.keystore,
    //     )
    //     .await?;

    //     let end_source_coin_balance = run_data
    //         .sui_client
    //         .coin_read_api()
    //         .get_balance(
    //             owner_address.clone(),
    //             Some(format!("{}", source_coin))
    //         )
    //         .await?;

    //     let realized_profit = end_source_coin_balance.total_balance as i128 - start_source_coin_balance.total_balance as i128;

    //     println!("| END BALANCE: {}", end_source_coin_balance.total_balance);
    //     println!("| REALIZED PROFIT: {}", realized_profit);
    //     println!("+-----------------------------------------------------");

    //     if realized_profit < 0 {
    //         println!("UR DOWN IN MONEY LOSER");
    //         // return Err(anyhow!("Arb failed"));
    //     }
    // }





    // Equivalent to .is_some() except we can print events
    while let Some(mut event_result) = subscribe_pool_state_changing_events.next().await {

        // Pushes to latest event. Makes sure we dont fall behind even if our inner loop takes a while.
        // Likely to choose the events that are most often emmitted lmaooo
        let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());
        while let std::task::Poll::Ready(Some(i)) = subscribe_pool_state_changing_events.next().poll_unpin(&mut cx) {
            event_result = i;
        }

        if let Ok(event) = event_result {
            // // println!("Event parsed_json: {:#?}", event.parsed_json);
            // println!("New event pool id: {:#?}", event.parsed_json.get("pool").context("missing pool field")?);
            // println!("Event package id: {}", event.package_id);

            let event_pool_field = event_struct_tag_to_pool_field
                .get(&event.type_)
                .context(
                    format!(
                        "Missing event_pool_field for StructTag {} in map",
                        &event.type_
                    )
                )?;

            // println!("{:#?}", &event.parsed_json);

            let pool_id = if let Value::String(pool_id_str) = 
                event.parsed_json.get(
                    event_pool_field
                ).context("loop_blocks: missing pool field")? {
                    ObjectID::from_str(pool_id_str)?
                } else {
                    return Err(anyhow!("Pool field should match the Value::String variant."));
                };

            if pool_id == last_seen_pool {
                // last_seen_pool = pool_id;
                continue;
            }

            last_seen_pool = pool_id;



            if skip_event_pools.contains(&pool_id) {
                continue;
            }

            // if pool_id != poot {
            //     continue;
            // }


            // Only print events we are not skipping
            println!("!NEW EVENT!\n    POOL: {}\n    PACKAGE: {}\n    EVENT TYPE: {}", pool_id, event.package_id, event.type_);

            // pool_set.insert(pool_id);
            // println!("[{:?}]", pool_set);

            // All these events were chosen because they have a pool id
            // To be honest its probably best to come up with a way to have a per 
            // exchange parsing of the pool id field but here they are both "pool"
            // We grab the cycles associate with a pool id and run our max profit calcs on every leg of the cycle.
            // We can filter by exchange per leg later but for now we're trimming off a lot of time.
            let cycles_opt = market_graph
                .pool_id_and_source_coin_to_cycles
                .get(&(pool_id, source_coin.clone()));
                // .context(format!("No cycles for (pool_id, source_coin): ({}, {})", pool_id, source_coin))?
                
            // Not every coin pool_id, source_coin combo is going to have cycles
            // So skip if there are no cycles
            if cycles_opt.is_none() {
                continue;
            }

            let cycles = cycles_opt
                .unwrap()
                .iter()
                .map(|vec_coins|{
                    vec_coins
                        .iter()
                        .map(|coin| {
                            (*coin).clone()
                        })
                        .collect::<Vec<TypeTag>>()
                })
                .collect::<Vec<Vec<TypeTag>>>();

            println!("num cycles: {}", cycles.len());

            // if cycles.len() > 45 {
            //     continue;
            // }

            let mut pool_ids_to_update = HashSet::new();
            
            // Update pool involved in the cycle
            // PREVIOUS ARBS WILL EMIT EVENTS
            // BUT WILL HAVE AFFECTED MULTIPLE POOLS
            // So we can't just update the pool for which the vent was emitted
            // We have to update all pools involved in the cycle
            // TODO: We can make this more efficient by only updating pools that
            // were involved in ther previous trade
            for cycle in cycles.iter() {
                for pair in cycle[..].windows(2) {
                    let coin_a = &pair[0];
                    let coin_b = &pair[1];
    
                    let pool_ids = market_graph
                        .graph
                        .edge_weight(&coin_a, &coin_b)
                        .context(format!("Missing markets for pair ({}, {})", coin_a, coin_b))?
                        .iter()
                        .map(|(pool_id, _)| {
                            pool_id.clone()
                        })
                        .collect::<Vec<ObjectID>>();
    
                    for pool_id in pool_ids {
                        pool_ids_to_update.insert(pool_id);
                    }
                }
            }

            let pool_ids_to_update_vec = pool_ids_to_update.into_iter().collect::<Vec<ObjectID>>();

            println!("Measuring... (updating {} markets)", pool_ids_to_update_vec.len());
            let now = Instant::now();

            let pool_id_to_object_response = sui_sdk_utils::get_object_id_to_object_response(
                &run_data.sui_client, 
                &pool_ids_to_update_vec
            ).await?;
            println!("pool_id_to_object_response elapsed: {:#?} for {} object responeses", now.elapsed(), pool_id_to_object_response.len());

            market_graph.update_markets_with_object_responses(
                &run_data.sui_client, 
                &pool_id_to_object_response
            ).await?;

            println!("update_markets_with_object_responses elapsed: {:#?}", now.elapsed());

            let mut optimized_results = cycles
                .par_iter()
                .map(|cycle| {
                    arbitrage::optimize_starting_amount_in(cycle, &market_graph)
                })
                .collect::<Result<Vec<_>, anyhow::Error>>()?;
    
            let mut used_legs_set = HashSet::new();

            // Good future pattern may be an execute all trades func
            // That takes an ordering and filtering function as args

            // Sort by most profitable so that the later excluded trades
            // are the less profitable ones. Sort in descending order.
            optimized_results.sort_by(|a, b| {
                b.profit.cmp(&a.profit)
            });

            // Exclude less profitable trades whose legs
            // were already seen before. Don't want any strange effects.
            // Later we can account for the effects of our trades and whatnot.
            // We should also not swap back through the same pool we came
            // Since we haven't accounted for the effects of our trade on
            // the pool.
            // RESULTS DEPEND ON ORDER

            let filtered_optimized_results_iter = optimized_results
                .into_iter()
                .filter(|optimized_result| {
                    println!("profit: {}", optimized_result.profit);

                    if optimized_result.profit < I256::from(7_000_000u128 * optimized_result.path.len() as u128) {
                        return false
                    }  // Should do some gas threshold instead
                    for leg in &optimized_result.path {
                        if used_legs_set.contains(leg.market.pool_id()) {
                            return false;
                        } else {
                            used_legs_set.insert(leg.market.pool_id());
                        }
                    }

                    true
                });

            for mut optimized_result in filtered_optimized_results_iter {

                let start_source_coin_balance = run_data
                    .sui_client
                    .coin_read_api()
                    .get_balance(
                        owner_address.clone(),
                        Some(format!("{}", source_coin))
                    )
                    .await?;
        
                let allowance = (start_source_coin_balance.total_balance * 4) / 5;

                // Adjust and check profitibility or skip
                if optimized_result.amount_in > allowance  {
                    println!("profitable optimized result amount_in: {}", optimized_result.amount_in);
                    // Skip so that we don't fail
                    // optimized_result.amount_in = start_source_coin_balance.total_balance / 2;
                    // panic!();
                    // continue;

                    let amount_in = allowance;
                    let amount_out = arbitrage::amount_out(&optimized_result.path, allowance)?;
                    let profit = I256::from(amount_out) - I256::from(amount_in);

                    if profit > I256::from(10_000_000u128 * optimized_result.path.len() as u128) {
                        optimized_result.amount_in = amount_in;
                        optimized_result.amount_out = amount_out;
                        optimized_result.profit = profit;

                        // focus_pool = Some(pool_id);
                    } else {
                        // focus_pool = None;
                        continue;
                    }
                }

                println!("+-----------------------------------------------------");
                println!("| START BALANCE: {}", start_source_coin_balance.total_balance);
                println!("| AMOUNT IN: {} {}", optimized_result.amount_in, source_coin);
                println!("| AMOUNT OUT: {} {}", optimized_result.amount_out, source_coin);
                println!("| RAW PROFIT: {} {}", optimized_result.profit, source_coin);
                optimized_result.path
                    .iter()
                    .try_for_each(|leg| {
                        if leg.x_to_y {
                            println!("|    +----[POOL: {}, X_TO_Y: {}]-------------", leg.market.pool_id(), leg.x_to_y);
                            println!("|    | {}", leg.market.coin_x());
                            println!("|    |   ----[RATE: {}]---->", leg.market.coin_y_price().context("Missing coin_y price.")?);
                            println!("|    | {}", leg.market.coin_y());
                            // println!("|    +------------------------------------------------");
                        } else {
                            println!("|    +----[POOL: {}, X_TO_Y: {}]-------------", leg.market.pool_id(), leg.x_to_y);
                            println!("|    | {}", leg.market.coin_y());
                            println!("|    |   ----[RATE: {}]---->", leg.market.coin_x_price().context("Missing coin_x price.")?);
                            println!("|    | {}", leg.market.coin_x());
                            // println!("|    +------------------------------------------------");
                        }

                        Ok::<(), anyhow::Error>(())
                    })?;

                println!("|    +------------------------------------------------");

                // panic!();

                arbitrage::execute_arb(
                    &run_data.sui_client,
                    optimized_result,
                    run_data
                        .keystore
                        .addresses()
                        .get(run_data.key_index)
                        .context(format!("No address for key index {} in keystore", run_data.key_index))?,
                    &run_data.keystore,
                )
                .await?;

                let end_source_coin_balance = run_data
                    .sui_client
                    .coin_read_api()
                    .get_balance(
                        owner_address.clone(),
                        Some(format!("{}", source_coin))
                    )
                    .await?;

                let realized_profit = end_source_coin_balance.total_balance as i128 - start_source_coin_balance.total_balance as i128;

                println!("| END BALANCE: {}", end_source_coin_balance.total_balance);
                println!("| REALIZED PROFIT: {}", realized_profit);
                println!("+-----------------------------------------------------");

                if realized_profit < 0 {
                    println!("UR DOWN IN MONEY LOSER");
                    // return Err(anyhow!("Arb failed"));
                }
            }
        }
    }
    
    Ok(())
}
