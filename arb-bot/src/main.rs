use arb_bot::Exchange;
use custom_sui_sdk::SuiClientBuilder;

use arb_bot::*;

use std::str::FromStr;
use sui_sdk::types::base_types::ObjectID;

const SUI_COIN_ADDRESS: &str = "0x0000000000000000000000000000000000000000000000000000000000000002";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let cetus = Cetus;

    let run_data = RunData {
        sui_client: SuiClientBuilder::default()
        .ws_url("wss://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .build("https://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .await?,
    };

    // cetus.get_all_markets(&run_data.sui_client).await?;

    let exchanges = vec![cetus];
    let base_coin_object_id = ObjectID::from_str(SUI_COIN_ADDRESS)?;

    let market_graph = MarketGraph::new(&run_data.sui_client, &exchanges, &base_coin_object_id).await?;

    println!("{:#?}", market_graph.graph.all_edges());
    // loop_blocks(run_data, vec![&flameswap]).await?;

    Ok(())
}