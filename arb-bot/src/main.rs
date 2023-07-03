use custom_sui_sdk::SuiClientBuilder;
use sui_sdk::SUI_COIN_TYPE;

use arb_bot::*;

use anyhow::Context;

use ethnum::I256;

use futures::future;
use sui_sdk::types::object::{Object, self};

use std::cmp;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::time::Instant;

use sui_sdk::rpc_types::{SuiMoveValue, SuiCoinMetadata, SuiObjectResponse};
use sui_sdk::types::base_types::ObjectID;

use move_core_types::language_storage::TypeTag;

use fixed::types::U64F64;

use petgraph::algo::all_simple_paths;

use crate::sui_sdk_utils;

const CETUS_EXCHANGE_ADDRESS: &str = "0x1eabed72c53feb3805120a081dc15963c204dc8d091542592abaf7a35689b2fb";
const TURBOS_EXCHANGE_ADDRESS: &str = "0x91bfbc386a41afcfd9b2533058d7e915a1d3829089cc268ff4333d54d6339ca1";
const TURBOS_TICK_MAP: &str = "0xd836ea2a159743a568fe29e8f42672a1b88414ab21be5411f8f6331e66b218d3";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let cetus = Cetus::from_str(CETUS_EXCHANGE_ADDRESS)?;
    let turbos = Turbos::from_str(TURBOS_EXCHANGE_ADDRESS)?;

    let run_data = RunData {
        sui_client: SuiClientBuilder::default()
        .ws_url("wss://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .build("https://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .await?
    };

    // // Testing Turbos TickMap
    // let turbos_dynamic_fields =  run_data
    //     .sui_client
    //     .read_api()
    //     .get_dynamic_fields(
    //     ObjectID::from_str("0x86ed41e9b4c6cce36de4970cfd4ae3e98d6281f13a1b16aa31fc73ec90079c3d")?,
    //     None,
    //     None
    // ).await?;

    // println!("{:#?}", turbos_dynamic_fields);

    // let turbos_tick = run_data
    //     .sui_client
    //     .read_api()
    //     .get_object_with_options(
    //         ObjectID::from_str("0xb5fed30450f21fb4df0c9881eb645be2dd583b41551ad47161a547c467bf7efd")?,
    //         SuiObjectDataOptions::full_content()
    //     )
    //     .await?;

    // println!("turbos_tick: {:#?}", turbos_tick);

    // let turbos_tick_word = run_data
    //     .sui_client
    //     .read_api()
    //     .get_object_with_options(
    //         ObjectID::from_str("0x7e90d1d4dc20d86ea40edab59eb1568f066f7e5fe74405ac45827a26ccc11127")?,
    //         SuiObjectDataOptions::full_content()
    //     )
    //     .await?;

    // println!("turbos_tick_word: {:#?}", turbos_tick_word);

    // let turbos_tick_map = run_data
    //     .sui_client
    //     .read_api()
    //     .get_dynamic_fields(
    //         ObjectID::from_str(TURBOS_TICK_MAP)?,
    //         None,
    //         None
    //     )
    //     .await?;

    // println!("turbos_tick_map: {:#?}", turbos_tick_map);

    // let exchanges = vec![cetus];
    let base_coin = TypeTag::from_str(SUI_COIN_TYPE)?;
    
    let cetus_markets = cetus.get_all_markets(&run_data.sui_client).await?;
    let turbos_markets = turbos.get_all_markets(&run_data.sui_client).await?;

    let mut markets = vec![];
    // markets.extend(cetus_markets.clone());
    markets.extend(turbos_markets.clone());

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
    // pool_id_to_fields.extend(cetus_pool_id_to_fields);
    pool_id_to_object_response.extend(turbos_pool_id_to_object_response);

    println!("pool_id_to_fields.keys().len(): {}", pool_id_to_object_response.keys().len());

    let paths = all_simple_paths(&market_graph.graph, &base_coin, &base_coin, 1, Some(7)).collect::<Vec<Vec<&TypeTag>>>().clone();

    let now = Instant::now();

    market_graph.update_markets_with_object_responses(&run_data.sui_client, &pool_id_to_object_response).await?;

    paths
        .iter()
        .for_each(|path| {
            // println!("SIMPLE CYCLE ({} HOP) ", path.len() - 1);
            // path
            //     .iter()
            //     .for_each(|coin| {
            //         println!("{}", *coin);
            //     });

            let mut best_path_rate = U64F64::from_num(1);

            let orig_decimals = coin_to_metadata.get(path[0]).unwrap().decimals as u32;
            let orig_amount = 5 * 10_u128.pow(orig_decimals);
            let mut amount_in = orig_amount;

            for pair in path[..].windows(2) {
                let orig = pair[0];
                let dest = pair[1];

                // Decimals for human readability (rates we would see on exchanges)
                let orig_decimals = coin_to_metadata.get(orig).unwrap().decimals as i32;
                let dest_decimals = coin_to_metadata.get(dest).unwrap().decimals as i32;

                // let ten =  U64F64::from_num(10);
                let adj = U64F64::from_num(10_f64.powi(dest_decimals - orig_decimals));

                let markets = market_graph
                    .graph
                    .edge_weight_mut(orig, dest)
                    .context("Missing edge weight")
                    .unwrap();

                let directional_rates = markets
                    .iter_mut()
                    .map(|market_info| {
                        let coin_x = market_info.market.coin_x();
                        let coin_y = market_info.market.coin_y();
                        if (coin_x, coin_y) == (orig, dest) {
                            let (_, amount_y) = market_info.market.compute_swap_x_to_y(amount_in);
                            amount_in = amount_y;
                            market_info.market.coin_x_price().unwrap()
                        } else if (coin_y, coin_x) == (orig, dest){
                            let (amount_x, _) = market_info.market.compute_swap_y_to_x(amount_in);
                            amount_in = amount_x;
                            market_info.market.coin_y_price().unwrap()
                        } else {
                            panic!("coin pair does not match");
                        }
                    });

                let best_leg_rate = directional_rates
                    .fold(U64F64::from_num(0), |max, current| {
                        cmp::max(max, current)
                    });

                println!("    {}: {} decimals", orig, orig_decimals);
                println!("    -> {}: {} decimals", dest, dest_decimals);
                // Using decimals for human readability
                println!("        leg rate: {}", best_leg_rate / adj);

                best_path_rate = best_path_rate * best_leg_rate;
            }

            println!("PROFIT: {}", I256::from(amount_in) - I256::from(orig_amount));

            // println!("{} HOP CYCLE RATE: {}", path.len() - 1, best_path_rate);

            println!("\n");
        });

        let elapsed = now.elapsed();
        println!("Elasped: {:.2?}", elapsed);
    
    // loop_blocks(run_data, vec![&flameswap]).await?;

    let bytes = [16, 39, 0, 0, 0, 0, 0, 0];
    println!("NUMBER: {}", u64::from_le_bytes(bytes));

    Ok(())
}