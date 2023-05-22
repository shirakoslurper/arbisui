use arb_bot::Exchange;
use custom_sui_sdk::SuiClientBuilder;

use arb_bot::*;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    // let flameswap = FlameSwap;
    let cetus = Cetus;

    let run_data = RunData {
        sui_client: SuiClientBuilder::default()
        .ws_url("wss://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .build("https://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .await?,
    };

    // flameswap.get_all_markets(&run_data.sui_client).await?;
    cetus.get_all_markets(&run_data.sui_client).await?;

    // loop_blocks(run_data, vec![&flameswap]).await?;

    Ok(())
}