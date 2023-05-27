use anyhow::Context;

use fixed::types::U64F64;

use move_core_types::language_storage::TypeTag;

use petgraph::graphmap::DiGraphMap;
use petgraph::visit::{Dfs, Walker};

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::cell::{RefCell, Ref};
use std::borrow::BorrowMut;

use sui_sdk::types::base_types::ObjectID;
use sui_sdk::rpc_types::SuiMoveValue;

use crate::markets::*;
use crate::hash::coin_pair_hash;

// The DirectedMarketGraph should provide pure structure

pub struct DirectedMarketInfo {
    market: Rc<RefCell<dyn Market>>,
    // price: Option<U64F64>,
    // fee: Option<U64F64>,
}

impl DirectedMarketInfo {
    fn new(market: Rc<RefCell<dyn Market>>) -> Self {
        DirectedMarketInfo {
            market
        }
    }

    fn market_update_with_fields(&self, fields: &BTreeMap<String, SuiMoveValue>) -> Result<(), anyhow::Error> {
        (*self.market).borrow_mut().update_with_fields(fields)
    }

    // fn market_coin_x(&self) -> &TypeTag {
    //     (*self.market).borrow().coin_x()
    // }

    // fn market_coin_y(&self) -> &TypeTag {
    //     (*self.market).borrow().coin_y()
    // }

    // fn market_pool_id(&self) -> &ObjectID {
    //     (*self.market).borrow().pool_id()
    // }

    fn market_coin_x(&self) -> Ref<'_, TypeTag> {
        Ref::map((*self.market).borrow(), |mi| mi.coin_x())
    }

    fn market_coin_y(&self) -> Ref<'_, TypeTag> {
        Ref::map((*self.market).borrow(), |mi| mi.coin_y())
    }

    fn market_pool_id(&self) -> Ref<'_, ObjectID> {
        Ref::map((*self.market).borrow(), |mi| mi.pool_id())
    }
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
    pub fn new(markets: &'data Vec<((TypeTag, TypeTag), Rc<RefCell<dyn Market>>)>) -> Result<Self, anyhow::Error> {

        let mut graph = DiGraphMap::<&TypeTag, DirectedMarketsInfo>::with_capacity(15000, 15000);
        
        markets
            .iter()
            .try_for_each(|((coin_x, coin_y), market)| {
                let coin_x_node = &coin_x;
                let coin_y_node = &coin_y;

                if !graph.contains_edge(coin_x_node, coin_y_node) {
                    graph.add_edge(
                        coin_x_node,
                        coin_y_node,
                        vec![]
                    );
                }

                let edge_coin_x_to_coin_y = graph.edge_weight_mut(coin_x_node, coin_y_node).context("Edge to update does not exist.")?;
                edge_coin_x_to_coin_y.push(
                    DirectedMarketInfo {
                        market: Rc::clone(&market),
                        // price: market.coin_x_price(),
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
                    DirectedMarketInfo {
                        market: Rc::clone(&market),
                        // price: market.coin_y_price(),
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
    pub fn update_markets_with_fields(&mut self, pool_id_to_fields: &HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>) -> Result<(), anyhow::Error> {
        self
            .graph
            .all_edges()
            .try_for_each(|(_, _, markets_infos)| {
                markets_infos
                    .iter()
                    .try_for_each(|market_info| {
                        let pool_id = market_info.market_pool_id().clone();
                        let fields = pool_id_to_fields.get(&pool_id).context("Missing fields for pool.")?;
                        market_info.market_update_with_fields(fields)?;
                        Ok::<(),  anyhow::Error>(())
                    })?;
                Ok::<(),  anyhow::Error>(())
            })?;
        Ok(())
    }
}