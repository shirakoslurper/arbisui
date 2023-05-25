use move_core_types::language_storage::StructTag;
use sui_sdk::types::base_types::ObjectID;
use custom_sui_sdk::SuiClient;
use async_trait::async_trait;
use petgraph::graphmap::DiGraphMap;
use petgraph::visit::Dfs;
use std::collections::{HashMap, BTreeMap};
use futures::future;

use petgraph::visit::Walker;

use fixed::{types::{U64F64}};

use sui_sdk::rpc_types::SuiMoveValue;

// use move_core_types::language_storage::StructTag;
// use anyhow::Result;

#[async_trait]
pub trait Exchange: Send + Sync {
    fn package_id(&self) -> Result<ObjectID, anyhow::Error>;
    async fn get_all_markets(&self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error>; // -> Result<Vec<Box<dyn Market>>>
    // async fn get_markets_info(&self, markets: Vec<impl Market>) -> Result<(), anyhow::Error>;
}

pub trait Market: Send + Sync {
    fn coin_x(&self) -> &StructTag;
    fn coin_y(&self) -> &StructTag;
    fn coin_x_price(&self) -> Option<U64F64>;
    fn coin_y_price(&self) -> Option<U64F64>;
    // fn pool_id() -> &ObjectID;
}


// type CoinMarkets = Vec<Box<dyn Market>>;

#[derive(Clone, Debug)]
pub struct DirectedMarketInfo {
    price: Option<U64F64>,
}

pub struct MarketGraph {
    pub graph: DiGraphMap<ObjectID, DirectedMarketInfo>,
    // pub cycles_by_token: HashMap<StructTag, Vec<Vec<StructTag>>>,
}

impl MarketGraph {
    pub async fn new(sui_client: &SuiClient, exchanges: &Vec<impl Exchange>, base_coin: &ObjectID) -> Result<Self, anyhow::Error> {
        // We could techinally use ObjectID (of the coin) if it satisfies NodeTrait
        // We just need a unique identifier for the market

        let mut graph = DiGraphMap::<ObjectID, DirectedMarketInfo>::with_capacity(15000, 15000);

        let markets = future::try_join_all(
            exchanges
                .iter()
                .map(|exchange| {
                    async {
                        Ok::<Vec<Box<dyn Market>>, anyhow::Error>(
                            exchange
                                .get_all_markets(sui_client)
                                .await?
                        )
                    }
                })
        )
        .await?
        .into_iter()
        .flatten()
        .collect::<Vec<Box<dyn Market>>>();

        // Add tokens to market graph i
        markets
            .into_iter()
            .for_each(|market| {
                let coin_x_object_id = ObjectID::from_address(market.coin_x().address);
                let coin_y_object_id = ObjectID::from_address(market.coin_y().address);

                // There may be multiple markets for a coin pair
                // add_edge inserts the node if it does not exist
                if !graph.contains_edge(coin_x_object_id, coin_y_object_id) {
                    graph.add_edge(
                        coin_x_object_id,
                        coin_y_object_id,
                        DirectedMarketInfo {
                            price: None
                        }
                    );
                }

                if !graph.contains_edge(coin_y_object_id, coin_x_object_id) {
                    graph.add_edge(
                        coin_y_object_id,
                        coin_x_object_id,
                        DirectedMarketInfo {
                            price: None
                        }
                    );
                }
            });

        // Separate out the base_coin_graph? (We might want to use the same graph for multiple
        // things idk what but we might)

        let mut base_coin_graph = DiGraphMap::<ObjectID, DirectedMarketInfo>::with_capacity(15000, 15000);

        // Only retain the graph where cycles to the denominating currency exists
        let dfs_from_base_coin = Dfs::new(&graph, base_coin.clone());

        dfs_from_base_coin
            .iter(&graph)
            .for_each(|coin| {
                graph
                    .edges(coin)
                    .for_each(|(origin, destination, weight)| {
                        if !base_coin_graph.contains_edge(origin, destination) {
                            base_coin_graph.add_edge(origin, destination, weight.clone());
                        }
                    })
            });

        // For Later: Update the prices of all edges in the graph
        
        Ok(
            MarketGraph {
                graph: base_coin_graph
            }
        )
    }

    // pub async fn update(&self) {

    // }
}