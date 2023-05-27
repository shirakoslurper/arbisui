use custom_sui_sdk::SuiClientBuilder;
use sui_sdk::SUI_COIN_TYPE;

use arb_bot::*;

use anyhow::Context;

use std::cmp;
use std::str::FromStr;

use move_core_types::language_storage::TypeTag;

use fixed::types::U64F64;

use petgraph::algo::all_simple_paths;

const SUI_COIN_ADDRESS: &str = "0x0000000000000000000000000000000000000000000000000000000000000002";
const CETUS_EXCHANGE_ADDRESS: &str = "0x1eabed72c53feb3805120a081dc15963c204dc8d091542592abaf7a35689b2fb";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let cetus = Cetus::from_str(CETUS_EXCHANGE_ADDRESS)?;

    let run_data = RunData {
        sui_client: SuiClientBuilder::default()
        .ws_url("wss://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .build("https://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .await?
    };

    // cetus.get_all_markets(&run_data.sui_client).await?;

    // let exchanges = vec![cetus];
    let base_coin = TypeTag::from_str(SUI_COIN_TYPE)?;
    
    let markets = cetus.get_all_markets(&run_data.sui_client).await?;

    let mut market_graph = MarketGraph::new(&markets)?;

    let pool_id_to_fields = cetus.get_pool_id_to_fields(&run_data.sui_client, &markets).await?;

    market_graph.update_markets_with_fields(&pool_id_to_fields)?;

    // market_graph.graph.all_edges()
    //     .for_each(|(coin_x, coin_y, markets)| {
    //         println!("<{}, {}>: {} markets", coin_x, coin_y, markets.len());
    //     });

    // market_graph.graph.nodes()
    //     .for_each(|n| {
    //         println!("{}", n);
    //     });


    // market_graph.graph.neighbors(&base_coin)
    //     .for_each(|n| {
    //         println!("{}", n.coin_x());
    //     });

    all_simple_paths(&market_graph.graph, &base_coin, &base_coin, 1, Some(4))
        .for_each(|path: Vec<&TypeTag>| {
            // println!("SIMPLE CYCLE: ");
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

                // println!("{}", orig);
                // println!("    -> {}:", dest);
                // println!("    leg rate: {}", best_leg_rate);s

                best_path_rate = best_path_rate * best_leg_rate;
            }

            println!("PATH RATE: {}", best_path_rate);

            // println!("\n");
        });
    
    // let market_graph = DirectedMarketGraph::new(&run_data.sui_client, &exchanges, &base_coin).await?;
    // println!("{:#?}", market_graph.graph.all_edges());
    // loop_blocks(run_data, vec![&flameswap]).await?;

    Ok(())
}