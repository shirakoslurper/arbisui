use librarian::*;

use clap::Parser;
use custom_sui_sdk::SuiClientBuilder;
use futures::StreamExt;
use futures_core::Stream;
use governor::{Quota, RateLimiter};
use move_core_types::language_storage::StructTag;
use nonzero_ext::*;
use sui_sdk::rpc_types::EventFilter;
use sui_sdk::types::base_types::{ObjectID, ObjectIDParseError};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use std::pin::Pin;
use std::boxed::Box;

const TURBOS_ORIGINAL_PACKAGE_ADDRESS: &str = "0x91bfbc386a41afcfd9b2533058d7e915a1d3829089cc268ff4333d54d6339ca1";
const TURBOS_CURRENT_PACKAGE_ADDRESS: &str = "0xeb9210e2980489154cc3c293432b9a1b1300edd0d580fe2269dd9cda34baee6d";
const TURBOS_VERSIONED_ID: &str = "0xf1cf0e81048df168ebeb1b8030fad24b3e0b53ae827c25053fff0779c1445b6f";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    // Get all the book/pool ids

    // Filter book/pool ids (via black list or something)
    // We also might start with a whitelist so decouple filtering from the following steps
    // If this were a separate process we might even consider some command line options here....
    
    // P.S. We can only enforce checkpoint pinning on a per pool basis

    // We need to subsribe to the events stream before hand

    // We should get events on a per pool basis 
    // (because, worst case scenario, they will be pool specific (kriya))
    // We should move away from the thing where we get a mapping of event 
    // type_ to pool id

    // We should map the types to objects or functions even that can handle the events

    // How general do we want this to be?

    // These objects should be more general. They should implement a trait for handling
    // events. But perhaps in as specific manner.

    // For stuff like Kriya we need to know all the pools to get the events
    // let sui_client = 

    let run_data_opts = RunDataOpts::parse();

    let rate_limiter = Arc::new(RateLimiter::direct(Quota::per_second(nonzero!(95u32))));

    let sui_client = SuiClientBuilder::default()
        .ws_url(
            &run_data_opts.wss_url
        )
        .build(
            &run_data_opts.rpc_url,
            &rate_limiter
        )
        .await?;

    let mut turbos = turbos::Turbos::new(
        ObjectID::from_str(TURBOS_ORIGINAL_PACKAGE_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?, 
        ObjectID::from_str(TURBOS_CURRENT_PACKAGE_ADDRESS).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
        ObjectID::from_str(TURBOS_VERSIONED_ID).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
    );

    let turbos_market_builders = turbos.get_all_market_builders(&sui_client).await?;
    println!("turbos_market_builder.len(): {}", turbos_market_builders.len());

    let pool_state_changing_event_filters = turbos_market_builders
        .iter()
        .flat_map(|market_builder| {
            market_builder
                .event_struct_tag_to_pool_field()
                .keys()
                .cloned()
                .map(|event_struct_tag| {
                    event_struct_tag
                })
                .collect::<HashSet<StructTag>>()
        })
        .collect::<HashSet<StructTag>>()
        .into_iter()
        .map(|event_struct_tag| {
            EventFilter::MoveEventType(
                event_struct_tag
            )
        })
        .collect::<Vec<EventFilter>>();

    println!("pool_state_changing_event_filters: {:#?}", pool_state_changing_event_filters);

    let mut subscribe_pool_state_changing_events = sui_client
        .event_api()
        .subscribe_event(
            EventFilter::Any(
                pool_state_changing_event_filters
            )
        )
        .await?;

    // let pinned_subscribe_pool_state_changing_events = 

//    let x =  subscribe_pool_state_changing_events == ();

    // if let Some(event) = subscribe_pool_state_changing_events.next().await {
    //     println!("main: {:#?}", event);
    // }

    sync_and_maintain_markets(&sui_client, subscribe_pool_state_changing_events, turbos_market_builders).await?;

    Ok(())
}