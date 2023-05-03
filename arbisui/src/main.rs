use futures::StreamExt;
use sui_sdk::rpc_types::EventFilter;

use sui_client_builder::SuiClientBuilder;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default()
        .build("https://sui-testnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .await?;

    // println!("{:#?}", )

    // let mut subscribe_all = sui
    //     .event_api()
    //     .subscribe_event(EventFilter::All(vec![]))
    //     .await?;
    // loop {
    //     println!("{:?}", subscribe_all.next().await);
    // }

    // .ws_url("wss://sui-testnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")

    Ok(())
}