use move_core_types::language_storage::TypeTag;
use fixed::types::U64F64;

use crate::markets;

// The DirectedMarketGraph should provide pure structure
// 

pub struct DirectedMarketInfo {
    market: Box<dyn Market>,
    price: Option(U64F64),
    fee: Option(U64F64,)
}

#[derive(Clone, Debug)]
type DirectedMarketsInfo = Vec<DirectedMarketInfo>;

pub struct DirectedMarketGraph<'data> {
    pub graph: DiGraphMap<&'data TypeTag, DirectedMarketInfo>,
    // pub cycles_by_token: HashMap<StructTag, Vec<Vec<StructTag>>>,
}

impl DirectedMarketGraph<'data> {
    pub async fn new<'data>(markets: &'data Vec<Box<dyn Market>>) -> Result<Self, anyhow::Error> {
        let mut graph = DiGraphMap::<&TypeTag, DirectedMarketsInfo>::with_capacity(15000, 15000);
        
        markets
            .iter()
            .for_each(|market| {
                let coin_x_node = market.coin_x().clone();
                let coin_y_node = market.coin_y().clone();

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
                        market: market.clone(),
                        price: market.coin_x_price(),
                    }
                );

                if !graph.contains_edge(coin_y_node, coin_x_node) {
                    graph.add_edge(
                        coin_y_node,
                        coin_x_node,
                        DirectedMarketInfo {
                            market,
                            price: None,
                        }
                    );
                }

                let mut edge_coin_y_to_coin_x = graph.edge_weight_mut(coin_y_node, coin_x_node).context("Edge to update does not exist.")?;
                edge_coin_y_to_coin_x.push(
                    DirectedMarketInfo {
                        market: market.clone(),
                        price: market.coin_y_price(),
                    }
                );
            });

        Ok(graph);
    }

    // 
    pub fn update(&mut self, ) -> Result<(), anyhow::Error> {

    }
}