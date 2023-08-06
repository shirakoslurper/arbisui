use custom_sui_sdk::SuiClientBuilder;
use sui_sdk::SUI_COIN_TYPE;

use arb_bot::*;

use anyhow::Context;

use clap::Parser;

use ethnum::I256;

use std::cmp;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::collections::HashSet;

use std::time::{Instant, Duration};

use sui_keys::keystore::{Keystore, FileBasedKeystore, AccountKeystore};

use sui_sdk::rpc_types::SuiObjectDataOptions;
use sui_sdk::types::base_types::{ObjectID, ObjectIDParseError};

use move_core_types::language_storage::TypeTag;


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
const TURBOS_CURRENT_PACKAGE_ADDRESS: &str = "0xeb9210e2980489154cc3c293432b9a1b1300edd0d580fe2269dd9cda34baee6d";
const TURBOS_VERSIONED_ID: &str = "0xf1cf0e81048df168ebeb1b8030fad24b3e0b53ae827c25053fff0779c1445b6f";

const KRIYADEX_PACKAGE_ADDRESS: &str = "0xa0eba10b173538c8fecca1dff298e488402cc9ff374f8a12ca7758eebe830b66";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let run_data_opts = RunDataOpts::parse();
    let keystore_path = run_data_opts.keystore_path;
    let keystore = Keystore::File(FileBasedKeystore::new(&keystore_path)?);
    let key_index = run_data_opts.key_index;

    let mut cetus = Cetus::new(
        ObjectID::from_str(CETUS_PACKAGE_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?, 
        ObjectID::from_str(CETUS_ROUTER_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
        ObjectID::from_str(CETUS_GLOBAL_CONFIG_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?
    );
    let mut turbos = Turbos::new(
        ObjectID::from_str(TURBOS_ORIGINAL_PACKAGE_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?, 
        ObjectID::from_str(TURBOS_CURRENT_PACKAGE_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
        ObjectID::from_str(TURBOS_VERSIONED_ID).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
    );
    let mut kriyadex = KriyaDex::new(
        ObjectID::from_str(KRIYADEX_PACKAGE_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?
    );

    // 100 Requests / Sec
    let rate_limiter = Arc::new(RateLimiter::direct(Quota::per_second(nonzero!(95u32))));

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

    let source_coin = TypeTag::from_str(SUI_COIN_TYPE)?;
    
    let mut cetus_markets = cetus.get_all_markets(&run_data.sui_client).await?;

    // let mut target_markets = HashSet::new();
    // target_markets.insert(ObjectID::from_str("0x2e041f3fd93646dcc877f783c1f2b7fa62d30271bdef1f21ef002cebf857bded")?);
    // target_markets.insert(ObjectID::from_str("0x296f2d8717eef03ec701357213fe2318d2281c31c609ffb446a14e3ce07d7754")?);
    // target_markets.insert(ObjectID::from_str("0x86ed41e9b4c6cce36de4970cfd4ae3e98d6281f13a1b16aa31fc73ec90079c3d")?);
    // target_markets.insert(ObjectID::from_str("0x51ee9f5e33c1d7b38b197a09acb17ef0027e83e6d0b3c0f6466855398e4c1cba")?);
    // target_markets.insert(ObjectID::from_str("0xb05c45ef2b9647cab2ccc21e2a85af14d81d0e0a4aabf5219de50902f0cee1d8")?);
    // target_markets.insert(ObjectID::from_str("0x31970253068fc315682301b128b17e6c84a60b1cf0397641395d2b65268ed924")?);
    
    // target_markets.insert(ObjectID::from_str("0xe63cedb411544f435221df201157db8666c910b7c7dd58c385cbc6a7a26f218b")?);
    // target_markets.insert(ObjectID::from_str("0xc8d7a1503dc2f9f5b05449a87d8733593e2f0f3e7bffd90541252782e4d2ca20")?);
    // target_markets.insert(ObjectID::from_str("0x46b44725cae3e9b31b722f79adbc00acc25faa6f41881c635b55a0ee65d9d4f4")?);
    // target_markets.insert(ObjectID::from_str("0xcf994611fd4c48e277ce3ffd4d4364c914af2c3cbb05f7bf6facd371de688630")?);
    // target_markets.insert(ObjectID::from_str("0x06d8af9e6afd27262db436f0d37b304a041f710c3ea1fa4c3a9bab36b3569ad3")?);

    // target_markets.insert(ObjectID::from_str("0x2c6fc12bf0d093b5391e7c0fed7e044d52bc14eb29f6352a3fb358e33e80729e")?);
    // target_markets.insert(ObjectID::from_str("0x81f6bdb7f443b2a55de8554d2d694b7666069a481526a1ff0c91775265ac0fc1")?);
    // target_markets.insert(ObjectID::from_str("0xff44a06b08481a0e2587537f0ef8f042de3b311f45f01d4dbae1cb15c507a204")?);
    // target_markets.insert(ObjectID::from_str("0x20739112ab4d916d05639f13765d952795d53b965d206dfaed92fff7729e29af")?);
    // target_markets.insert(ObjectID::from_str("0x238f7e4648e62751de29c982cbf639b4225547c31db7bd866982d7d56fc2c7a8")?);
    // target_markets.insert(ObjectID::from_str("0xb8a6b18fa8a9d773125b89e6def125a48c28e6d85d7e4f2e1424a62ffcef0bb5")?);

    // target_markets.insert(ObjectID::from_str("0xc93fb2ccd960bd8e369bd95a7b2acd884abf45943e459e95835941322e644ef1")?);

    // target_markets.insert(ObjectID::from_str("0x5eb2dfcdd1b15d2021328258f6d5ec081e9a0cdcfa9e13a0eaeb9b5f7505ca78")?);
    // target_markets.insert(ObjectID::from_str("0x9b2a5da1310657a622f22c2fb54e7be2eb0a858a511b8c4987c9dd5df96d11f3")?);

    // cetus_markets = cetus_markets
    //     .into_iter()
    //     .filter(|market| {
    //         target_markets.contains(market.pool_id())
    //     })
    //     .collect::<Vec<_>>();

    let turbos_markets = turbos.get_all_markets(&run_data.sui_client).await?;
    let kriyadex_markets: Vec<Box<dyn Market>> = kriyadex.get_all_markets(&run_data.sui_client).await?;

    let mut markets = vec![];
    markets.extend(turbos_markets.clone());
    markets.extend(cetus_markets.clone());
    markets.extend(kriyadex_markets.clone());

    println!("{}", markets[0].package_id());

    let usdc_weth_pool = ObjectID::from_str("0x84fa8fe46a41151396beeabc9167a114c06e1f882d827c4a7f5ab8676de63e14")?;

    markets = markets
        .into_iter()
        .filter(|market| {
            market.pool_id() != &usdc_weth_pool
        })
        .collect::<Vec<_>>();

    println!("markets.len(): {}", markets.len());

    let mut market_graph = MarketGraph::new(&markets)?;

    let max_intermediate_nodes = run_data_opts.max_intermediate_nodes;
   
    market_graph.add_cycles(
        &source_coin,
        max_intermediate_nodes
    )?;

    loop_blocks(
        &run_data,
        // &vec![Box::new(cetus), Box::new(turbos)],
        &vec![Box::new(cetus), Box::new(turbos), Box::new(kriyadex)],
        &mut market_graph,
        &source_coin
    ).await?;

    Ok(())
}