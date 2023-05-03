use std::str::FromStr;
use arb_bot::Exchange;
use sui_sdk::rpc_types::{SuiObjectDataOptions, SuiObjectResponseQuery};
use sui_sdk::types::base_types::ObjectID;
use custom_sui_sdk::SuiClientBuilder;

use arb_bot::*;

struct TestCoinExchange {
    package_id: ObjectID,
}

impl Exchange for TestCoinExchange {
    fn package_id(&self) -> &ObjectID {
        &self.package_id
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let test_exchange_package_id = ObjectID::from_str("0x6b84da4f5dc051759382e60352377fea9d59bc6ec92dc60e0b6387e05274415f")?;

    let test_exchange = TestCoinExchange {
        package_id: test_exchange_package_id,
    };

    let run_data = RunData {
        sui_client: SuiClientBuilder::default()
        .ws_url("wss://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .build("https://sui-mainnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f")
        .await?,
    };

    loop_blocks(run_data, vec![test_exchange]).await?;

    Ok(())
}