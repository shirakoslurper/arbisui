// #![feature(async_fn_in_trait)]

use anyhow::{anyhow, Context, Result};
use sui_sdk::rpc_types::EventFilter;
use custom_sui_sdk::SuiClient;
// use sui_sdk::wallet_context::WalletContext;
use futures::StreamExt;

use move_core_types::language_storage::TypeTag;

use rayon::prelude::*;

use serde_json::Value;

use std::collections::HashMap;
use std::str::FromStr;

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
pub use crate::markets::*;
pub use crate::market_graph::*;
pub use crate::cetus::*;
pub use crate::turbos::*;

pub struct RunData {
    pub sui_client: SuiClient,
    // pub wallet_context: WalletContext,
}

pub async fn loop_blocks<'a>(
    run_data: &RunData, 
    exchanges: &Vec<Box<dyn Exchange>>, 
    market_graph: &mut MarketGraph<'a>,
    source_coin: &TypeTag
    // paths: Vec<Vec<&TypeTag>>
) -> Result<()> {

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

    // Equivalent to .is_some() except we can print events
    while let Some(event_result) = subscribe_pool_state_changing_events.next().await {
        
        if let Ok(event) = event_result {
            // println!("Event parsed_json: {:#?}", event.parsed_json);
            // println!("New event pool id: {:#?}", event.parsed_json.get("pool").context("missing pool field")?);

            println!("Event package id: {}", event.package_id);

            let pool_id = if let Value::String(pool_id_str) = 
                event.parsed_json.get("pool").context("missing pool field")? {
                    ObjectID::from_str(pool_id_str)?
                } else {
                    return Err(anyhow!("Pool field should match the Value::String variant."));
                };

            let pool_response = run_data
                .sui_client
                .read_api()
                .get_object_with_options(
                    pool_id, 
                    SuiObjectDataOptions::full_content()
                ).await?;

            market_graph.update_market_with_object_response(
                &run_data.sui_client,
                &pool_id,
                &pool_response
            ).await?;

            println!("Updated pool: {}", pool_id);

            // All these events were chosen because they have a pool id
            // To be honest its probably best to come up with a way to have a per 
            // exchange parsing of the pool id field but here they are both "pool"
            // We grab the cycles associate with a pool id and run our max profit calcs on every leg of the cycle.
            // We can filter by exchange per leg later but for now we're trimming off a lot of time.
            let cycles = market_graph
                .pool_id_and_source_coin_to_cycles
                .get(&(pool_id, source_coin.clone()))
                .context(format!("No cycles for (pool_id, source_coin): ({}, {})", pool_id, source_coin))?;

            let mut optimized_results = cycles
                .par_iter()
                .map(|cycle| {
                    arbitrage::optimize_starting_amount_in(cycle, &market_graph)
                })
                .collect::<Result<Vec<_>, anyhow::Error>>()?;
    
            optimized_results = optimized_results
                .into_iter()
                .filter(|optimized_result| {
                    optimized_result.profit > 0
                })
                .collect::<Vec<_>>();

            optimized_results
                .iter()
                .for_each(|or| {
                    println!("{}", or.profit);
                });

        }



    }
    
    Ok(())
}


// Take 

// pub fn initalize_loop() {

// }



// pub struct Config {
//     pub rpc: ,
//     pub ws: ,
// }

// pub async fn run() -> Result<()> {
//     let mut run_data = RunData
// }

// Search only 