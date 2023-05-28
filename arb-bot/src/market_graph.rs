use anyhow::Context;



use move_core_types::language_storage::TypeTag;

use petgraph::graphmap::{DiGraphMap};

use std::collections::{BTreeMap, HashMap};

use sui_sdk::types::base_types::ObjectID;
use sui_sdk::rpc_types::SuiMoveValue;

use crate::markets::*;

// The DirectedMarketGraph should provide pure structure

// #[derive(Debug)]
pub struct MarketInfo {
    pub market: Box<dyn Market>,
}

// type MarketsInfo = Vec<MarketInfo>;

pub struct MarketGraph<'data> {
    pub graph: DiGraphMap<&'data TypeTag, Vec<MarketInfo>>,
    // pub cycles_by_token: HashMap<StructTag, Vec<Vec<StructTag>>>,
}

impl <'data> MarketGraph<'data> {
    // Not consuming. Data is external to the structure.
    // Uses Rc as multiple edges may refer to the same market.
    // Also 2 edges (directional) per market.
    pub fn new(markets: &'data [Box<dyn Market>]) -> Result<Self, anyhow::Error> {

        let mut graph = DiGraphMap::<&TypeTag, Vec<MarketInfo>>::with_capacity(15000, 15000);
        
        markets
            .iter()
            .try_for_each(| market| {
                let coin_x_node = market.coin_x();
                let coin_y_node = market.coin_y();

                if !graph.contains_edge(coin_x_node, coin_y_node) {
                    graph.add_edge(
                        coin_x_node,
                        coin_y_node,
                        vec![]
                    );
                }

                let edge_coin_x_to_coin_y = graph.edge_weight_mut(coin_x_node, coin_y_node).context("Edge to update does not exist.")?;
                edge_coin_x_to_coin_y.push(
                    MarketInfo {
                        market: dyn_clone::clone_box(&**market)
                    }
                );

                if !graph.contains_edge(coin_y_node, coin_x_node) {
                    graph.add_edge(
                        coin_y_node,
                        coin_x_node,
                        vec![]
                    );
                }

                let edge_coin_y_to_coin_x = graph.edge_weight_mut(coin_y_node, coin_x_node).context("Edge to update does not exist.")?;
                edge_coin_y_to_coin_x.push(
                    MarketInfo {
                        market: dyn_clone::clone_box(&**market)
                    }
                );

                Ok::<(), anyhow::Error>(())
            })?;

        Ok(
            MarketGraph {
                graph
            }
        )
    }

    // The callee of update may not necessarily (likely won't actually) 
    // maintain the order of vertex pairs we get by calling <GraphMap>.all_edges()
    // We'll need to identify the vertex pairs in the parameters we pass
    // Make more sense to iterate through all edges
    // But markets fields to edges is one to many
    pub fn update_markets_with_fields(&mut self, pool_id_to_fields: &HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>) -> Result<(), anyhow::Error> {
        self
            .graph
            .all_edges_mut()
            .try_for_each(|(_, _, markets_infos)| {
                markets_infos
                    .iter_mut()
                    .try_for_each(|market_info| {
                        let pool_id = *market_info.market.pool_id();
                        let fields = pool_id_to_fields.get(&pool_id).context("Missing fields for pool.")?;
                        market_info.market.update_with_fields(fields)?;
                        Ok::<(),  anyhow::Error>(())
                    })?;
                Ok::<(),  anyhow::Error>(())
            })?;
        Ok(())
    }
}