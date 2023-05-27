use arb_bot::Exchange;
use custom_sui_sdk::SuiClientBuilder;
use sui_sdk::SUI_COIN_TYPE;

use arb_bot::*;

use std::str::FromStr;
use sui_sdk::types::base_types::ObjectID;

use move_core_types::language_storage::TypeTag;

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
    // let base_coin = TypeTag::from_str(SUI_COIN_TYPE)?;
    
    let markets = cetus.get_all_markets(&run_data.sui_client).await?;

    let mut market_graph = MarketGraph::new(&markets)?;

    let pool_id_to_fields = cetus.get_pool_id_to_fields(&run_data.sui_client, &markets).await?;

    market_graph.update_markets_with_fields(&pool_id_to_fields)?;
    
    // let market_graph = DirectedMarketGraph::new(&run_data.sui_client, &exchanges, &base_coin).await?;
    // println!("{:#?}", market_graph.graph.all_edges());
    // loop_blocks(run_data, vec![&flameswap]).await?;

    Ok(())
}