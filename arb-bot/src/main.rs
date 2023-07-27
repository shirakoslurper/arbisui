use custom_sui_sdk::SuiClientBuilder;
use sui_sdk::SUI_COIN_TYPE;

use arb_bot::*;

use anyhow::Context;

use clap::Parser;

use ethnum::I256;

use futures::future;

use std::cmp;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;
use std::time::Instant;
use std::sync::Arc;

use sui_keys::keystore::{Keystore, FileBasedKeystore, AccountKeystore};

use sui_sdk::rpc_types::{SuiMoveValue, SuiCoinMetadata, SuiObjectResponse, SuiObjectDataOptions, SuiTypeTag};
use sui_sdk::types::object::{Object, self};
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
const TURBOS_CURRENT_PACKAGE_ADDRESS: &str = "0x84d1ad43e95e9833670fcdb2f2d9fb7618fe1827e3908f2c2bb842f3dccb80af";
const TURBOS_VERSIONED_ID: &str = "0xf1cf0e81048df168ebeb1b8030fad24b3e0b53ae827c25053fff0779c1445b6f";
// temp
// const MY_SUI_ADDRESS: &str = "0x02a212de6a9dfa3a69e22387acfbafbb1a9e591bd9d636e7895dcfc8de05f331";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let run_data_opts = RunDataOpts::parse();
    let keystore_path = run_data_opts.keystore_path;
    let keystore = Keystore::File(FileBasedKeystore::new(&keystore_path)?);
    let key_index = run_data_opts.key_index;

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
    let rate_limiter = Arc::new(RateLimiter::direct(Quota::per_second(nonzero!(35u32))));

    let run_data = RunData {
        sui_client: SuiClientBuilder::default()
        .ws_url(
            &run_data_opts.wss_url
            // "wss://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f"
            // "wss://sui-mainnet.blastapi.io:443/25957d97-3d27-4236-8056-6b3f4eff7f0b"
        )
        .build(
            &run_data_opts.rpc_url,
            // "https://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f",
            // "https://sui-mainnet.blastapi.io:443/25957d97-3d27-4236-8056-6b3f4eff7f0b",
            &rate_limiter
        )
        .await?,
        keystore,
        key_index
    };

    // // END ARB

    let owner_address = run_data
        .keystore
        .addresses()
        .get(run_data.key_index)
        .context(format!("No address for key index {} in keystore", run_data.key_index))?
        .clone();

    let mut exec = Box::new(CetusMarket {
        parent_exchange: cetus.clone(),
        coin_x: TypeTag::from_str("0x06864a6f921804860930db6ddbe2e16acdf8504495ea7481637a1c8b9a8fe54b::cetus::CETUS")?, //0xf0fe2210b4f0c4e3aff7ed147f14980cf14f1114c6ad8fd531ab748ccf33373b::bswt::BSWT")?
        coin_y: TypeTag::from_str("0x2::sui::SUI")?,
        pool_id: ObjectID::from_str("0x498e57c0f7a67436177348afb1e43fe15c2c572f49f056202823d8a47aefcbd1")?, // 0x25ccb77dc4de57879e12ac7f8458860a0456a0a46a84b9f4a8903b5498b96665
        coin_x_sqrt_price: None, // In terms of y. x / y
        coin_y_sqrt_price: None, // In terms of x. y / x
        computing_pool: None
    }) as Box<dyn Market>;

    let exec_response = run_data
        .sui_client
        .read_api()
        .get_object_with_options(
            exec.pool_id().clone(), 
            SuiObjectDataOptions::full_content()
        ).await?;

    exec.update_with_object_response(&run_data.sui_client, &exec_response).await?;

    
    // println!("{:?}", exec.compute_swap_x_to_y(974641586360));
    println!("{:?}", exec.compute_swap_x_to_y(10_000_000_000));
    // println!("{:?}", exec.compute_swap_y_to_x(8_000_000_000));
    println!("{:?}", exec.coin_x_price());
    // panic!();

    let exec_result = arbitrage::OptimizedResult {
        path: vec![
            arbitrage::DirectedLeg {
                x_to_y: false,
                market: &exec
            },
        ],
        amount_in: 8_000_000_000, // 974641586360
        amount_out: 100000000000,
        profit: I256::from(0)
    };

    arbitrage::execute_arb(
        &run_data.sui_client, 
        exec_result, 
        &owner_address, 
        &run_data.keystore
    ).await?;

    // panic!();

    let source_coin = TypeTag::from_str(SUI_COIN_TYPE)?;
    
    let cetus_markets = cetus.get_all_markets(&run_data.sui_client).await?;
    let turbos_markets = turbos.get_all_markets(&run_data.sui_client).await?;

    let mut markets = vec![];
    markets.extend(turbos_markets.clone());
    markets.extend(cetus_markets.clone());

    println!("markets.len(): {}", markets.len());

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

    let max_intermediate_nodes = run_data_opts.max_intermediate_nodes;

    market_graph.update_markets_with_object_responses(&run_data.sui_client, &pool_id_to_object_response).await?;
    market_graph.add_cycles(
        &source_coin,
        max_intermediate_nodes
    )?;

    let coin_x = TypeTag::from_str("0xf0fe2210b4f0c4e3aff7ed147f14980cf14f1114c6ad8fd531ab748ccf33373b::bswt::BSWT")?;
    let coin_y = TypeTag::from_str("0x2::sui::SUI")?;

    let weight = market_graph.graph.edge_weight(
        &coin_x,
        &coin_y,
    ).unwrap();

    for x in weight {
        println!("pool: {}", x.0);
    }

    panic!();

    loop_blocks(
        &run_data,
        &vec![Box::new(cetus), Box::new(turbos)],
        &mut market_graph,
        &source_coin
    ).await?;

    // // let mut total_profit = I256::from(0_u8);

    // let now = Instant::now();

    // let mut optimized_results = paths
    //     .par_iter()
    //     .map(|path| {
    //         arbitrage::optimize_starting_amount_in(path, &market_graph)
    //     })
    //     .collect::<Result<Vec<_>, anyhow::Error>>()?;

    // optimized_results = optimized_results
    //     .into_iter()
    //     .filter(|optimized_result| optimized_result.profit > 0)
    //     .collect::<Vec<_>>();
    
    // let elapsed = now.elapsed();
    // println!("Elasped: {:.2?}", elapsed);
    // // println!("{:#?}", optimized_results[0]);

    // let total_profit = optimized_results
    //     .iter()
    //     .fold(I256::from(0u8), |tp, optimized_result| {
    //         tp + optimized_result.profit
    //     });

    // println!("total_profit: {}", total_profit);

    // let most_profitable = optimized_results
    //     .iter()
    //     .fold(optimized_results[0].clone(), |max_result, optimized_result| {
    //         if max_result.profit > optimized_result.profit {
    //             max_result
    //         } else {
    //             optimized_result.clone()
    //         }
    //     });

    // optimized_results.iter().for_each(|or| {
    //     println!("profit: {}", or.profit);
    // });

    // println!("{:#?}", most_profitable);

    // // let transaction_builder = TransactionBuilder::new();

    // // if most_profitable.amount_in < 10_000_000_000 {
    // //     for leg in most_profitable.path {
    // //         let mut pt_builder = ProgrammableTransactionBuilder::new();

    // //         // println!("coin x metadata: {:#?}", coin_to_metadata.get(leg.market.coin_x()).unwrap());
    // //         // println!("coin y metadata: {:#?}", coin_to_metadata.get(leg.market.coin_y()).unwrap());
            
    // //         let orig_coin_string = if leg.x_to_y {
    // //             Some(format!("{}", leg.market.coin_x()))
    // //         } else {
    // //             Some(format!("{}", leg.market.coin_y()))
    // //         };

    // //         println!("coin_x string: {}", format!("0x{}", leg.market.coin_x()));
    // //         println!("coin_y string: {}", format!("0x{}", leg.market.coin_y()));

    // //         // Yields SuiRpcResult<Vec<Coin>>
    // //         let coins = run_data
    // //             .sui_client
    // //             .coin_read_api(
    // //             )
    // //             .select_coins(
    // //                 SuiAddress::from_str(MY_SUI_ADDRESS)?,
    // //                 orig_coin_string,
    // //                 most_profitable.amount_in,
    // //                 vec![]
    // //             )
    // //             .await?;

    // //         let coin_object_ids = coins
    // //             .into_iter()
    // //             .map(|coin| {
    // //                 coin.coin_object_id
    // //             })
    // //             .collect::<Vec<ObjectID>>();

    // //         // let coin_args = run_data.sui_client.transaction_builder()
    // //         //     .programmable_make_object_vec(
    // //         //         &mut pt_builder,
    // //         //         coin_object_ids
    // //         //     ).await?;

    // //         // programmable turbos move call
    // //         // for now lets make it async so that the interface function 
    // //         // gets the clock time for us and we don't have to feed it anything?
            
    // //         println!("AAAAAAA");

    // //         let predicted_amount_out = if leg.x_to_y {
    // //             leg.market
    // //                 .compute_swap_x_to_y(most_profitable.amount_in).1
    // //         } else {
    // //             leg.market
    // //                 .compute_swap_y_to_x(most_profitable.amount_in).0
    // //         };

    // //         println!("predicted amount out: {}", predicted_amount_out);

    // //         leg.market
    // //             .add_swap_to_programmable_transaction(
    // //                 run_data.sui_client.transaction_builder(),
    // //                 & mut pt_builder,
    // //                 coin_object_ids,
    // //                 leg.x_to_y,
    // //                 most_profitable.amount_in,
    // //                 predicted_amount_out,
    // //                 SuiAddress::from_str(MY_SUI_ADDRESS)?
    // //             )
    // //             .await?;

    // //         let transaction = run_data
    // //             .sui_client
    // //             .transaction_builder()
    // //             .finish_building_programmable_transaction(
    // //                 pt_builder,
    // //                 SuiAddress::from_str(MY_SUI_ADDRESS)?,
    // //                 None,
    // //                 9000000
    // //             )
    // //             .await?;

    // //         let result = run_data
    // //             .sui_client
    // //             .read_api()
    // //             .dry_run_transaction_block(
    // //                 transaction
    // //             )
    // //             .await?;

    // //         println!("RESULT: {:#?}", result);
                

    // //         // programmable
    // //     }
    // // }

    Ok(())
}