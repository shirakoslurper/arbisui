use custom_sui_sdk::SuiClientBuilder;
use sui_sdk::SUI_COIN_TYPE;

use arb_bot::*;

use anyhow::Context;

use std::cmp;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use sui_sdk::rpc_types::SuiMoveValue;
use sui_sdk::types::base_types::ObjectID;

use move_core_types::language_storage::TypeTag;

use fixed::types::U64F64;

use petgraph::algo::all_simple_paths;

const SUI_COIN_ADDRESS: &str = "0x0000000000000000000000000000000000000000000000000000000000000002";
const CETUS_EXCHANGE_ADDRESS: &str = "0x1eabed72c53feb3805120a081dc15963c204dc8d091542592abaf7a35689b2fb";
const TURBOS_EXCHANGE_ADDRESS: &str = "0x91bfbc386a41afcfd9b2533058d7e915a1d3829089cc268ff4333d54d6339ca1";

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

    // let exchanges = vec![cetus];
    let base_coin = TypeTag::from_str(SUI_COIN_TYPE)?;
    
    let cetus_markets = cetus.get_all_markets(&run_data.sui_client).await?;
    let turbos_markets = turbos.get_all_markets(&run_data.sui_client).await?;

    // turbos_markets
    //     .iter()
    //     .for_each(|market| {
    //         println!("coin_x: {}\ncoin_y: {}", market.coin_x(), market.coin_y());
    //     });

    let mut markets = vec![];
    // markets.extend(cetus_markets.clone());
    markets.extend(turbos_markets.clone());

    let mut market_graph = MarketGraph::new(&markets)?;

    let cetus_pool_id_to_fields = cetus
        .get_pool_id_to_object_response(&run_data.sui_client, &cetus_markets)
        .await?
        .iter()
        .map(|(pool_id, object_response)| {
            Ok(
                (
                    pool_id.clone(),
                    sui_sdk_utils::get_fields_from_object_response(object_response)?
                )
            )
        })
        .collect::<Result<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>, anyhow::Error>>()?;

    let turbos_pool_id_to_fields = turbos
        .get_pool_id_to_object_response(&run_data.sui_client, &cetus_markets)
        .await?
        .iter()
        .map(|(pool_id, object_response)| {
            Ok(
                (
                    pool_id.clone(),
                    sui_sdk_utils::get_fields_from_object_response(object_response)?
                )
            )
        })
        .collect::<Result<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>, anyhow::Error>>()?;

    let mut pool_id_to_fields = HashMap::new();
    // pool_id_to_fields.extend(cetus_pool_id_to_fields);
    pool_id_to_fields.extend(turbos_pool_id_to_fields);

    market_graph.update_markets_with_fields(&pool_id_to_fields)?;

    all_simple_paths(&market_graph.graph, &base_coin, &base_coin, 1, Some(4))
        .for_each(|path: Vec<&TypeTag>| {
            println!("SIMPLE CYCLE: ");
            // path
            //     .iter()
            //     .for_each(|coin| {
            //         println!("{}", *coin);
            //     });

            let mut best_path_rate = U64F64::from_num(1);

            for pair in path[..].windows(2) {
                let orig = pair[0];
                let dest = pair[1];

                let markets = market_graph
                    .graph
                    .edge_weight(orig, dest)
                    .context("Missing edge weight")
                    .unwrap();

                let directional_rates = markets
                    .iter()
                    .map(|market_info| {
                        let coin_x = market_info.market.coin_x();
                        let coin_y = market_info.market.coin_y();
                        if (coin_x, coin_y) == (orig, dest) {
                            market_info.market.coin_x_price().unwrap()
                        } else if (coin_y, coin_x) == (orig, dest){
                            market_info.market.coin_y_price().unwrap()
                        } else {
                            panic!("coin pair does not match");
                        }
                    });

                let best_leg_rate = directional_rates
                    .fold(U64F64::from_num(0), |max, current| {
                        cmp::max(max, current)
                    });

                println!("{}", orig);
                println!("    -> {}:", dest);
                println!("    leg rate: {}", best_leg_rate);

                best_path_rate *= best_leg_rate;
            }

            println!("PATH RATE: {}", best_path_rate);

            println!("\n");
        });
    
    // loop_blocks(run_data, vec![&flameswap]).await?;

    Ok(())
}