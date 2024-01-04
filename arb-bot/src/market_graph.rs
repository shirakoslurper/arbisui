use anyhow::Context;

use futures::future;

use move_core_types::language_storage::TypeTag;

use petgraph::algo::all_simple_paths;
use petgraph::graphmap::DiGraphMap;

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::time::{Instant, Duration};

use custom_sui_sdk::SuiClient;

// use rayon::prelude::*;/

use sui_sdk::types::base_types::ObjectID;
use sui_sdk::rpc_types::{SuiMoveValue, SuiObjectResponse};

use crate::markets::*;

// The DirectedMarketGraph should provide pure structure

// #[derive(Debug)]
pub struct MarketInfo {
    pub market: Box<dyn Market>,
}

// type MarketsInfo = Vec<MarketInfo>;

pub struct MarketGraph<'data> {
    // Node: coin type
    // Edge: map of pool id to market info
    pub graph: DiGraphMap<&'data TypeTag, HashMap<ObjectID, MarketInfo>>,
    pub pool_id_to_coin_pair: HashMap<ObjectID, (&'data TypeTag, &'data TypeTag)>,
    pub source_coin_to_cycles: HashMap<TypeTag, Vec<Vec<&'data TypeTag>>>,
    pub pool_id_and_source_coin_to_cycles: HashMap<(ObjectID, TypeTag), Vec<Vec<&'data TypeTag>>>, // Regardless of source coin? 
}

impl <'data> MarketGraph<'data> {
    // Not consuming. Data is external to the structure.
    // Uses Rc as multiple edges may refer to the same market.
    // Also 2 edges (directional) per market.
    pub fn new(markets: &'data [Box<dyn Market>]) -> Result<Self, anyhow::Error> {

        let mut graph = DiGraphMap::<&TypeTag, HashMap<ObjectID, MarketInfo>>::with_capacity(15000, 15000);
        let mut pool_id_to_coin_pair = HashMap::new();

        markets
            .iter()
            .try_for_each(|market| {
                let coin_x_node = market.coin_x();
                let coin_y_node = market.coin_y();

                // Only need to insert pool id to coin pair once
                pool_id_to_coin_pair.insert(
                    market.pool_id().clone(),
                    (coin_x_node, coin_y_node)
                );

                if !graph.contains_edge(coin_x_node, coin_y_node) {
                    graph.add_edge(
                        coin_x_node,
                        coin_y_node,
                        HashMap::new()
                    );
                }

                let edge_coin_x_to_coin_y = graph.edge_weight_mut(coin_x_node, coin_y_node).context("Edge to update does not exist.")?;
                edge_coin_x_to_coin_y.insert(
                    market.pool_id().clone(),
                    MarketInfo {
                        market: dyn_clone::clone_box(&**market)
                    }
                );

                if !graph.contains_edge(coin_y_node, coin_x_node) {
                    graph.add_edge(
                        coin_y_node,
                        coin_x_node,
                        HashMap::new()
                    );
                }

                let edge_coin_y_to_coin_x = graph.edge_weight_mut(coin_y_node, coin_x_node).context("Edge to update does not exist.")?;
                edge_coin_y_to_coin_x.insert(
                    market.pool_id().clone(),
                    MarketInfo {
                        market: dyn_clone::clone_box(&**market)
                    }
                );

                Ok::<(), anyhow::Error>(())
            })?;

        Ok(
            MarketGraph {
                graph,
                pool_id_to_coin_pair,
                source_coin_to_cycles: HashMap::new(),
                pool_id_and_source_coin_to_cycles: HashMap::new()
            }
        )
    }

    // The callee of update may not necessarily (likely won't actually) 
    // maintain the order of vertex pairs we get by calling <GraphMap>.all_edges()
    // We'll need to identify the vertex pairs in the parameters we pass
    // Make more sense to iterate through all edges
    // But markets fields to edges is one to many

    // Now only updates provided markets
    pub async fn update_markets_with_object_responses(
        &mut self, 
        sui_client: &SuiClient, 
        pool_id_to_object_response: &HashMap<ObjectID, SuiObjectResponse>
    ) -> Result<(), anyhow::Error> {

        future::try_join_all(
            self
            .graph
            .all_edges_mut()
            .map(|(_, _, markets_infos)| {
                async {
                    future::try_join_all(
                        markets_infos
                        .iter_mut()
                        .map(|(_, market_info)| {
                            async {
                                let pool_id = *market_info.market.pool_id();
                                // let object_response = pool_id_to_object_response.get(&pool_id).context("Missing fields for pool.")?;
                                // market_info.market.update_with_object_response(sui_client, object_response).await?;
                                if let Some(object_response) = pool_id_to_object_response.get(&pool_id) {
                                    // let now = Instant::now();
                                    market_info.market.update_with_object_response(sui_client, object_response).await?;
                                    // println!("time elapsed to update {}: {:#?}", market_info.market.pool_id(), now.elapsed());
                                }

                                Ok::<(), anyhow::Error>(())
                            }
                        })
                    ).await?;

                    Ok::<(),  anyhow::Error>(())
                }
            })
        )
        .await?;

        // for (pool_id, response) in pool_id_to_object_response {
        //     self.update_market_with_object_response(
        //         sui_client, 
        //         pool_id, 
        //         response
        //     ).await?;
        // }

        Ok(())
    }

    // We may need to use a HashMap<ObjectID, MarketInfo>
    // And or keep a map of pool ids to coin pairs
    // In 

    // so pool id -> coin pair
    // coin pair -> markets
    // pool id -> market

    // to allow for the updating of individual markets
    // We'll also need source and dest coins....
    pub async fn update_market_with_object_response(
        &mut self, 
        sui_client: &SuiClient, 
        pool_id: &ObjectID,
        response: &SuiObjectResponse
    ) -> Result<(), anyhow::Error>{
        let now = Instant::now();

        let (coin_a, coin_b) = self
            .pool_id_to_coin_pair
            .get(pool_id)
            .context("Missing coin pair for pool.")?;
        
        // Update markets in one direction (this is a directed graph)
        let a_to_b_pool_id_to_market = self
            .graph
            .edge_weight_mut(coin_a, coin_b)
            .context("Missing edge between coins a and b.")?;

        let a_to_b_market = a_to_b_pool_id_to_market
            .get_mut(pool_id)
            .context("Missing market for pool.")?;
        
        a_to_b_market.market.update_with_object_response(sui_client, response).await?;

        // Update markets in the other direction
        let b_to_a_pool_id_to_market = self
            .graph
            .edge_weight_mut(coin_a, coin_b)
            .context("Missing edge between coins b to a.")?;

        let b_to_a_market = b_to_a_pool_id_to_market
            .get_mut(pool_id)
            .context("Missing market for pool.")?;

        b_to_a_market.market.update_with_object_response(sui_client, response).await?;

        println!("time elapsed to update single market: {:#?}", now.elapsed());

        Ok(())
    }

    // Find cycles
    // This will feed to pool_to_paths
    // Guarantees that we aren't dealing with paths that do not exist
    // in the graph.
    fn find_cycles(
        &self,
        source_coin: &'data TypeTag,
        max_intermediate_nodes: usize,
    ) -> Vec<Vec<&'data TypeTag>> {
        all_simple_paths(
            &self.graph, 
            source_coin, 
            source_coin, 
            1, 
            Some(max_intermediate_nodes)
        ).collect::<Vec<Vec<&TypeTag>>>()
    }

    pub fn add_cycles(
        &mut self,
        source_coin: &'data TypeTag,
        max_intermediate_nodes: usize,
    ) -> Result<(), anyhow::Error>{
        self
            .source_coin_to_cycles
            .insert(
                source_coin.clone(),
                self.find_cycles(source_coin, max_intermediate_nodes)
            );

        let cycles = self
            .source_coin_to_cycles
            .get(source_coin)
            .context("No cycles for given source coin.")?;

        for cycle in cycles {
            for pair in cycle[..].windows(2) {
                let coin_a = pair[0];
                let coin_b = pair[1];

                let markets = self
                    .graph
                    .edge_weight(coin_a, coin_b)
                    .context(format!("Missing edge from {} to {}", coin_a, coin_b))?;

                for (pool_id, _) in markets {
                    let pool_cycles = self.pool_id_and_source_coin_to_cycles
                        .entry((pool_id.clone(), source_coin.clone()))
                        .or_insert(Vec::new());
                    
                    pool_cycles.push(
                        cycle.clone()
                    );
                }
            }
        }

        Ok(())
    }

    // pub fn pool_to_cycles(
    //     &self,
    //     source_coin: &'data TypeTag
    // ) -> Result<HashMap<ObjectID, Vec<&Vec<&'data TypeTag>>>, anyhow::Error> {
    //     let mut pool_to_cycles = HashMap::new();

    //     let cycles = self
    //         .source_coin_to_cycles
    //         .get(source_coin)
    //         .context("No cycles for given source coin.")?;

    //     for cycle in cycles {
    //         for pair in cycle[..].windows(2) {
    //             let coin_a = pair[0];
    //             let coin_b = pair[1];

    //             let markets = self
    //                 .graph
    //                 .edge_weight(coin_a, coin_b)
    //                 .context(format!("Missing edge from {} to {}", coin_a, coin_b))?;

    //             for (pool_id, _) in markets {
    //                 let pool_cycles = pool_to_cycles
    //                     .entry(pool_id.clone())
    //                     .or_insert(Vec::new());
                    
    //                 pool_cycles.push(
    //                     cycle
    //                 );
    //             }
    //         }
    //     }

    //     Ok(pool_to_cycles)
    // }
}

// source coin to pool to 

// // // Map pool to cycle
// // // Iterate through every cycle
// // // Iterate through every leg of every cycle
// // // Iterate though every pool of every leg
// // // Insert pool to cycle
// fn pool_to_path_from_paths<'a>(
//     &self,
//     paths: Vec<Vec<&'a TypeTag>>
// ) -> Result<(), anyhow::Error> {
//     let mut pool_to_path = HashMap::new();

//     for path in paths {
//         let markets = market_graph
//             .graph
//             .edge_weight()
//             .context("aaa")?;


//     }

//     Ok(())
// }