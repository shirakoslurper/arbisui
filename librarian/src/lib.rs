pub mod constants;
// pub mod markets;
pub mod fast_v3_pool;
pub mod sui_sdk_utils;
pub mod sui_json_utils;
pub mod turbos;

use clap::Parser;

use sui_sdk::rpc_types::{Checkpoint, SuiEvent};
use std::pin::Pin;
use std::thread;
use crossbeam::channel::unbounded;
use custom_sui_sdk::SuiClient;
use custom_sui_sdk::error::SuiRpcResult;
use futures::{future, StreamExt};
use futures_core::Stream;
use crate::turbos::*;


// use crossbeam_channel::unbounded;

// Note that we *can* parse out the pool
// fn playback_for_market(
//     events_cache: Vec<SuiEvent>,
//     mut target_market: Box<dyn Market>,
// ) -> Box<dyn Market> {
//     events_cache
//         .iter()
//         .filter(|event| {
//             // market.matches_update_event(event)
//         })
//         .fold(target_market, |market, event| {
//             // market.update_with_event(event)
//         })
// }

#[derive(Parser)]
#[clap(
    name = "arb-bot",
    about = "hopefully he makes money",
    rename_all = "kebab-case"
)]

pub struct RunDataOpts {
    #[clap(long, default_value = "wss://sui-testnet.blastapi.io:443/25957d97-3d27-4236-8056-6b3f4eff7f0b")]
    pub wss_url: String,
    // #[clap(long, default_value = "https://sui-mainnet.blastapi.io:443/338f3a96-cd39-41f8-88d9-4faefe2eee21")]
    #[clap(long, default_value = "https://fullnode.testnet.sui.io:443")]
    pub rpc_url: String,
}


// pub async fn sync_markets(
//     sui_client: SuiClient,
//     mut events_stream: impl Stream<Item = SuiRpcResult<SuiEvent>>,
//     market_builders_to_sync: Vec<TurbosMarketBuilder> // We'lll extend this to Box<dyn MarketBuilder>
// ) -> Result<Vec<TurbosMarket>, anyhow::Error> {
    
//     tokio::pin!(events_stream);

//     if let Some(event_result) = events_stream.next().await {
//         println!("lib: {:#?}", event_result);
//     }

//     // println!("AHHHH");

//     // let (fetch_markets_cue_sender, fetch_markets_cue_receiver) = unbounded();
//     // let (fetched_markets_sender, fetched_markets_receiver) = unbounded();

//     println!("pre thread");
//     // The 
//     // let other_client = sui_client.clone();
//     thread::spawn(move || async move {
//         println!("thread");
//         // loop {
//             if let Ok(()) = fetch_markets_cue_receiver.recv() {
//                 let checkpoint_pinned_markets = get_checkpoint_pinned_markets(
//                     &sui_client,
//                     market_builders_to_sync
//                 )
//                 .await
//                 .unwrap();
    
//                 fetched_markets_sender.send(checkpoint_pinned_markets).unwrap();
//             } else {
//                 panic!("Ahhhhh");
//             }
//         // }
//     });

//     // let mut event_cache = Vec::new();
//     let mut start_recording = false;
//     let mut synced_markets = Vec::new();

//     while let Some(event_result) = events_stream.next().await {
//         println!("loop!");
//         // event_cache.push(event_result?);

//         // if start_recording == false {
//         //     println!("sending!");
//         //     fetch_markets_cue_sender.send(())?;
//         //     start_recording = true;
//         // }

//         // if let Ok(checkpoint_pinned_markets) = fetched_markets_receiver.recv() {
//         //     for (checkpoint, mut market) in checkpoint_pinned_markets.into_iter() {
//         //         // play events in event cache
//         //         println!("Syncing {}", market.pool_id());
//         //         for event in event_cache.iter() {
//         //             // only play events if they match the market
//         //             if let Some(pool_id) = market.try_parse_pool_id_from_event(event)? {
//         //                 // Apply events after the market's checkpoint
//         //                 if pool_id == *market.pool_id() && event.timestamp_ms.unwrap() >= checkpoint.timestamp_ms {
//         //                     market.update_with_event(&event)?;
//         //                 }
//         //             }
//         //             // skip otherwise
//         //         }
    
//         //         synced_markets.push(market);
//         //     }
    
//         //     // for markets 
//         //     // synced_markets = checkpoint_pinned_markets.iter;
//         //     break;
//         // }
//     }

//     println!("post loop");

//     Ok(
//         (
//             synced_markets
//         )
//     )
// } 

pub async fn sync_and_maintain_markets(
    sui_client: &SuiClient,
    mut events_stream: impl Stream<Item = SuiRpcResult<SuiEvent>>,
    market_builders_to_sync: Vec<TurbosMarketBuilder> // We'lll extend this to Box<dyn MarketBuilder>
) -> Result<(), anyhow::Error> {
    
    tokio::pin!(events_stream);

    if let Some(event_result) = events_stream.next().await {
        // println!("lib: {:#?}", event_result);
        println!("event stream works!");
    }

    // let mut event_cache = Vec::new();
    let mut start_recording = false;
    // let mut synced_markets = Vec::new();
    let mut checkpoint_pinned_markets = Vec::new();
    
    // We can just apply the events
    // IF they have been applying backpressure
    // And we don't drop any...
    while let Some(event_result) = events_stream.next().await {

        let event = event_result?;

        println!("loop!");
        if start_recording == false {
            checkpoint_pinned_markets = get_checkpoint_pinned_markets(&sui_client, market_builders_to_sync.clone()).await?;
            start_recording = true;
        } else {
            for (checkpoint, market) in checkpoint_pinned_markets.iter_mut() {
                // play events in event cache
                println!("Syncing {}", market.pool_id());
                if let Some(pool_id) = market.try_parse_pool_id_from_event(&event)? {
                    // Apply events after the market's checkpoint
                    if pool_id == *market.pool_id() && event.timestamp_ms.unwrap() >= checkpoint.timestamp_ms {
                        market.update_with_event(&event)?;
                    }
                }
            }
        }
    }

    println!("post loop");

    Ok(())
    // let 

    // Ok(
    //     (
    //         checkpoint_pinne
    //     )
    // )
} 

async fn get_checkpoint_pinned_markets(
    sui_client: &SuiClient,
    market_builders_to_sync: Vec<TurbosMarketBuilder> // We'lll extend this to Box<dyn MarketBuilder>
) -> Result<Vec<(Checkpoint, TurbosMarket)>, anyhow::Error> {
    println!("AA");

    future::try_join_all(
        market_builders_to_sync
        .into_iter()
        .map(|market_builder| async {
            Ok::<(Checkpoint, TurbosMarket), anyhow::Error>(
                market_builder.build_checkpoint_pinned_market(sui_client).await?
            )
        })
    )
    .await
}

// // This is our "base" stream. 
// // It receives the events and is the source of events
// // for all other event queues
// pub async fn sync_markets(
//     events_stream: impl Stream<Item = SuiRpcResult<SuiEvent>>,
//     markets_to_sync: Vec<Box<dyn MarketBuilder>> // Builds something that implements Market, pool_id + Exchange
//     // type_to_pool: HashMap<StructTag, Box<dyn Handler>>
// ) -> Result<()> {

//     // swap looping synced state fetcher
//     // given that the computing pools are most importantly holders of state
//     // the should not be Option<T> but just T?

//     // Is it ok to create an empty pool but then sync it. 
//     // No what if we miss a sync?
//     // A pool should consist of real, if nonsynced, information to start.
//     thread::spawn(|| {
//         while Some(pool_to_sync) = pools_to_sync.pop() {
//             // One of the following two.
//             let (pool, checkpoint) = pool_to_sync.build_checkpoint_pinned();
//             // let checkpoint = pool_to_sync.update_checkpoint_pinned();
            
//             // Send request for cache and receive and playback
//             tx.send(
//                 EventCacheRequest {
//                     timestamp: checkpoint.timestamp,
//                     pool_id: pool.pool_id()
//                 }
//             );

//             // Send pool
//             tx.send(pool)

//         }

//         // We could technically send all the pools.
//         // What if we want to think modularly? 
//         // Do we want the main book manager loop
//         // to accept new books as they are added?
//         // Note: This does not exactly play nice with
//         // events like Kriyadex's where we can only subscribe if 
//         // we know specifics about the exchange's pools.
//         // To add a kriya pool we would have to suspend the current
//         // stream, resubscribe, and recalibrate.

//         // So we'll have to couple our book managing with our
//         // pool selection + event substription..

//         // Spinning up a new thread for every pool seems like a great
//         // way to destroy performace. + There will be redundant event
//         // streams for dex's like Trubos and Cetus


//         // OR
//         pools_to_sync.iter()
//     });

//     let mut event_cache = Vec::new();
//     while Some(event_result) = events_stream.next(
//         // We want to cue the synced pools request here

//         let event = event_result?;

//         // At this point we don't know 
//         event_cache.push();


//         // If we accept pools one by one, we can clear the
//         // event cache for every pool we receive, IF the event 
//         // cache is waiting for a signal to move onto the next 
//         // pool.

//         // We don't really have to worry about the cost of context switching here

//         // Playback does slightly more complex

//         // Or we can accept all pools at once.
//         // This would simplify things. 
//         // we could exit the loop, apply playback to all pools
//         // and rejoin the loop.

//         // If we're not meant to accept any more pools we can break it up into stages like this
//         if rx.try_recv() {
//             // Sequential works but im sure we can go faster
//             playback_for_markets();
//             // flush event cache
//             // then we break the while loop
//         }

//     )

//     // And then we pick up the stream again
//     // And start keeping the book.
// }


// // It seems that by the nature of event subscription, doing this in stages makes sense
