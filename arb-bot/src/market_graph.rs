use anyhow::Context;

use fixed::types::U64F64;

use move_core_types::language_storage::TypeTag;

use petgraph::graphmap::DiGraphMap;
use petgraph::visit::{Dfs, Walker};

use std::collections::BTreeMap;
use std::rc::Rc;

use sui_sdk::rpc_types::SuiMoveValue;

use crate::markets::*;

// The DirectedMarketGraph should provide pure structure

pub struct DirectedMarketInfo {
    market: Rc<dyn Market>,
    price: Option<U64F64>,
    // fee: Option<U64F64>,
}

type DirectedMarketsInfo = Vec<DirectedMarketInfo>;

pub struct DirectedMarketGraph<'data> {
    pub graph: DiGraphMap<&'data TypeTag, DirectedMarketsInfo>,
    // pub cycles_by_token: HashMap<StructTag, Vec<Vec<StructTag>>>,
}

impl <'data> DirectedMarketGraph<'data> {
    // Not consuming. Data is external to the structure.
    // Uses Rc as multiple edges may refer to the same market.
    // Also 2 edges (directional) per market.
    pub fn new(markets: &'data Vec<Rc<dyn Market>>) -> Result<Self, anyhow::Error> {

        let mut graph = DiGraphMap::<&TypeTag, DirectedMarketsInfo>::with_capacity(15000, 15000);
        
        markets
            .iter()
            .try_for_each(|market| {
                let coin_x_node = market.coin_x();
                let coin_y_node = market.coin_y();

                if !graph.contains_edge(coin_x_node, coin_y_node) {
                    graph.add_edge(
                        coin_x_node,
                        coin_y_node,
                        vec![]
                    );
                }

                let mut edge_coin_x_to_coin_y = graph.edge_weight_mut(coin_x_node, coin_y_node).context("Edge to update does not exist.")?;
                edge_coin_x_to_coin_y.push(
                    DirectedMarketInfo {
                        market: Rc::clone(&market),
                        price: market.coin_x_price(),
                    }
                );

                if !graph.contains_edge(coin_y_node, coin_x_node) {
                    graph.add_edge(
                        coin_y_node,
                        coin_x_node,
                        vec![]
                    );
                }

                let mut edge_coin_y_to_coin_x = graph.edge_weight_mut(coin_y_node, coin_x_node).context("Edge to update does not exist.")?;
                edge_coin_y_to_coin_x.push(
                    DirectedMarketInfo {
                        market: Rc::clone(&market),
                        price: market.coin_y_price(),
                    }
                );

                Ok::<(), anyhow::Error>(())
            })?;

        Ok(
            DirectedMarketGraph {
                graph
            }
        )
    }

    // The callee of update may not necessarily (likely won't actually) 
    // maintain the order of vertex pairs we get by calling <GraphMap>.all_edges()
    // We'll need to identify the vertex pairs in the parameters we pass
    // Make more sense to iterate through all edges
    // But markets fields to edges is one to many
    pub fn update_markets_with_fields(&mut self, markets_fields: &Vec<BTreeMap<String, SuiMoveValue>>) -> Result<(), anyhow::Error> {
        self
            .graph
            .all_edges_mut()
            .for_each(|(origin, destination, weight)| {
                
            });

        Ok(())
    }
}