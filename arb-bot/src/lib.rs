// #![feature(async_fn_in_trait)]

use anyhow::{anyhow, Context, Result};
use sui_sdk::rpc_types::EventFilter;

use clap::Parser;

use custom_sui_sdk::SuiClient;
// use sui_sdk::wallet_context::WalletContext;

use ethnum::I256;

use futures::{StreamExt, future};

use move_core_types::language_storage::TypeTag;

use rayon::prelude::*;

use serde_json::Value;
use sui_sdk::rpc_types::SuiObjectResponse;

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Instant, Duration};
use std::collections::HashSet;

use sui_keys::keystore::{Keystore, AccountKeystore};
use sui_sdk::types::object::Object;
use sui_sdk::sui_client_config;
use sui_sdk::rpc_types::SuiObjectDataOptions;
use sui_sdk::types::base_types::ObjectID;

pub mod markets;
pub mod market_graph;
pub mod cetus;
pub mod turbos;
pub mod constants;
pub mod sui_sdk_utils;
pub mod sui_json_utils;
pub mod turbos_pool;
pub mod cetus_pool; 
pub mod arbitrage;
pub mod fast_v3_pool;
pub use crate::markets::*;
pub use crate::market_graph::*;
pub use crate::cetus::*;
pub use crate::turbos::*;

#[derive(Parser)]
#[clap(
    name = "arb-bot",
    about = "hopefully he makes money",
    rename_all = "kebab-case"
)]
pub struct RunDataOpts {
    #[clap(long, default_value = "wss://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")]
    pub wss_url: String,
    #[clap(long, default_value = "https://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")]
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
        .map(|exchange| {
            exchange.event_filters()
        })
        .collect::<Result<Vec<Vec<EventFilter>>, anyhow::Error>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<EventFilter>>();

    let mut subscribe_pool_state_changing_events = run_data
        .sui_client
        .event_api()
        .subscribe_event(
            EventFilter::Any(
                pool_state_changing_event_filters
            )
        )
        .await?;

    // let pool_to_cycles = market_graph.pool_to_cycles(source_coin)?.clone();

    // Very hacky skip
    let cetus_sui_pool_id = ObjectID::from_str("0x2e041f3fd93646dcc877f783c1f2b7fa62d30271bdef1f21ef002cebf857bded")?;

    // let mut pool_set: HashSet<ObjectID> = HashSet::new();

    let mut skip_event_pools = HashSet::new();
    skip_event_pools.insert(cetus_sui_pool_id);

    // Equivalent to .is_some() except we can print events
    while let Some(event_result) = subscribe_pool_state_changing_events.next().await {
        
        if let Ok(event) = event_result {
            // // println!("Event parsed_json: {:#?}", event.parsed_json);
            println!("New event pool id: {:#?}", event.parsed_json.get("pool").context("missing pool field")?);
            println!("Event package id: {}", event.package_id);

            let pool_id = if let Value::String(pool_id_str) = 
                event.parsed_json.get("pool").context("missing pool field")? {
                    ObjectID::from_str(pool_id_str)?
                } else {
                    return Err(anyhow!("Pool field should match the Value::String variant."));
                };

            if skip_event_pools.contains(&pool_id) {
                continue;
            }

            // pool_set.insert(pool_id);
            // println!("[{:?}]", pool_set);

            // let pool_id = ObjectID::from_str("0xcf994611fd4c48e277ce3ffd4d4364c914af2c3cbb05f7bf6facd371de688630")?;

            // if pool_id == cetus_sui_pool_id {
            //     continue;
            // }

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

            println!("Measuring...");
            let now = Instant::now();

            let pool_id_to_object_response = sui_sdk_utils::get_object_id_to_object_response(
                &run_data.sui_client, 
                &pool_ids_to_update_vec
            ).await?;

            market_graph.update_markets_with_object_responses(
                &run_data.sui_client, 
                &pool_id_to_object_response
            ).await?;

            println!("elapsed: {:#?}", now.elapsed());

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

                    if optimized_result.profit < I256::from(10_000_000u128 * optimized_result.path.len() as u128) {
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
        
                if optimized_result.amount_in > start_source_coin_balance.total_balance / 2 {
                    println!("profitable optimized result amount_in: {}", optimized_result.amount_in);
                    // Skip so that we don't fail
                    // optimized_result.amount_in = start_source_coin_balance.total_balance / 2;
                    // panic!();
                    continue;
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
