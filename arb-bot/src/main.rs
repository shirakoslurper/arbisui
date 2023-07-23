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
use sui_sdk::types::base_types::{ObjectID, ObjectIDParseError, SuiAddress};
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

const CETUS_PACKAGE_ADDRESS: &str = "0x1eabed72c53feb3805120a081dc15963c204dc8d091542592abaf7a35689b2fb";
// more liek periphery address but we can change names later
const CETUS_ROUTER_ADDRESS: &str = "0x2eeaab737b37137b94bfa8f841f92e36a153641119da3456dec1926b9960d9be";
const CETUS_GLOBAL_CONFIG_ADDRESS: &str = "0xdaa46292632c3c4d8f31f23ea0f9b36a28ff3677e9684980e4438403a67a3d8f";

const TURBOS_ORIGINAL_PACKAGE_ADDRESS: &str = "0x91bfbc386a41afcfd9b2533058d7e915a1d3829089cc268ff4333d54d6339ca1";
// const TURBOS_ROUTER_ADDRESS: &str = "0x91bfbc386a41afcfd9b2533058d7e915a1d3829089cc268ff4333d54d6339ca1";
const TURBOS_VERSIONED_ID: &str = "0xf1cf0e81048df168ebeb1b8030fad24b3e0b53ae827c25053fff0779c1445b6f";

// const TURBOS_PACKAGE_ADDRESS: &str = "0x84d1ad43e95e9833670fcdb2f2d9fb7618fe1827e3908f2c2bb842f3dccb80af";
const TURBOS_CURRENT_PACKAGE_ADDRESS: &str = "0x84d1ad43e95e9833670fcdb2f2d9fb7618fe1827e3908f2c2bb842f3dccb80af";
// const TURBOS_VERSIONED_ID: &str = "0xf1cf0e81048df168ebeb1b8030fad24b3e0b53ae827c25053fff0779c1445b6f";

// const TURBOS_TICK_MAP: &str = "0xd836ea2a159743a568fe29e8f42672a1b88414ab21be5411f8f6331e66b218d3";

// temp
const MY_SUI_ADDRESS: &str = "0x02a212de6a9dfa3a69e22387acfbafbb1a9e591bd9d636e7895dcfc8de05f331";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let cetus = Cetus::new(
        ObjectID::from_str(CETUS_PACKAGE_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?, 
        ObjectID::from_str(CETUS_ROUTER_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
        ObjectID::from_str(CETUS_GLOBAL_CONFIG_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?
    );
    let turbos = Turbos::new(
        ObjectID::from_str(TURBOS_ORIGINAL_PACKAGE_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?, 
        ObjectID::from_str(TURBOS_CURRENT_PACKAGE_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
        ObjectID::from_str(TURBOS_VERSIONED_ID).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
    );

    // 50 Requests / Sec
    let rate_limiter = Arc::new(RateLimiter::direct(Quota::per_second(nonzero!(50u32))));

    let run_data = RunData {
        sui_client: SuiClientBuilder::default()
        .ws_url(
            "wss://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f"
            // "wss://sui-mainnet.blastapi.io:443/25957d97-3d27-4236-8056-6b3f4eff7f0b"
        )
        .build(
            "https://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f",
            // "https://sui-mainnet.blastapi.io:443/25957d97-3d27-4236-8056-6b3f4eff7f0b",
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

    let max_intermediate_nodes = 2;

    let paths = all_simple_paths(
        &market_graph.graph, 
        &base_coin, 
        &base_coin, 
        1, 
        Some(max_intermediate_nodes)
    ).collect::<Vec<Vec<&TypeTag>>>()
    .clone();

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
    
    let elapsed = now.elapsed();
    println!("Elasped: {:.2?}", elapsed);
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

    optimized_results.iter().for_each(|or| {
        println!("profit: {}", or.profit);
    });

    println!("{:#?}", most_profitable);

    // let transaction_builder = TransactionBuilder::new();

    if most_profitable.amount_in < 10_000_000_000 {
        for leg in most_profitable.path {
            let mut pt_builder = ProgrammableTransactionBuilder::new();

            // println!("coin x metadata: {:#?}", coin_to_metadata.get(leg.market.coin_x()).unwrap());
            // println!("coin y metadata: {:#?}", coin_to_metadata.get(leg.market.coin_y()).unwrap());
            
            let orig_coin_string = if leg.x_to_y {
                Some(format!("{}", leg.market.coin_x()))
            } else {
                Some(format!("{}", leg.market.coin_y()))
            };

            println!("coin_x string: {}", format!("0x{}", leg.market.coin_x()));
            println!("coin_y string: {}", format!("0x{}", leg.market.coin_y()));

            // Yields SuiRpcResult<Vec<Coin>>
            let coins = run_data
                .sui_client
                .coin_read_api(
                )
                .select_coins(
                    SuiAddress::from_str(MY_SUI_ADDRESS)?,
                    orig_coin_string,
                    most_profitable.amount_in,
                    vec![]
                )
                .await?;

            let coin_object_ids = coins
                .into_iter()
                .map(|coin| {
                    coin.coin_object_id
                })
                .collect::<Vec<ObjectID>>();

            // let coin_args = run_data.sui_client.transaction_builder()
            //     .programmable_make_object_vec(
            //         &mut pt_builder,
            //         coin_object_ids
            //     ).await?;

            // programmable turbos move call
            // for now lets make it async so that the interface function 
            // gets the clock time for us and we don't have to feed it anything?
            
            println!("AAAAAAA");

            let predicted_amount_out = if leg.x_to_y {
                leg.market
                    .compute_swap_x_to_y(most_profitable.amount_in).1
            } else {
                leg.market
                    .compute_swap_y_to_x(most_profitable.amount_in).0
            };

            println!("predicted amount out: {}", predicted_amount_out);

            leg.market
                .add_swap_to_programmable_transaction(
                    run_data.sui_client.transaction_builder(),
                    & mut pt_builder,
                    coin_object_ids,
                    leg.x_to_y,
                    most_profitable.amount_in,
                    predicted_amount_out,
                    SuiAddress::from_str(MY_SUI_ADDRESS)?
                )
                .await?;

            let transaction = run_data
                .sui_client
                .transaction_builder()
                .finish_building_programmable_transaction(
                    pt_builder,
                    SuiAddress::from_str(MY_SUI_ADDRESS)?,
                    None,
                    9000000
                )
                .await?;

            let result = run_data
                .sui_client
                .read_api()
                .dry_run_transaction_block(
                    transaction
                )
                .await?;

            println!("RESULT: {:#?}", result);
                

            // programmable
        }
    }

    Ok(())
}