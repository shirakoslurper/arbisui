use move_core_types::language_storage::TypeTag;
use sui_sdk::types::base_types::ObjectID;
use custom_sui_sdk::SuiClient;
use async_trait::async_trait;

use std::collections::{BTreeMap, HashMap};

use fixed::types::U64F64;

use sui_sdk::rpc_types::{SuiMoveValue, SuiObjectResponse};
use dyn_clone::DynClone;

#[async_trait]
pub trait Exchange: Send + Sync {
    fn package_id(&self) -> &ObjectID;
    async fn get_all_markets(&self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error>; // -> Result<Vec<Box<dyn Market>>>
    // async fn get_pool_id_to_fields(&self, sui_client: &SuiClient, markets: &[Box<dyn Market>]) -> Result<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>, anyhow::Error>;
    async fn get_pool_id_to_object_response(&self, sui_client: &SuiClient, markets: &[Box<dyn Market>]) -> Result<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error>;
}

pub trait Market: Send + Sync + DynClone {
    fn coin_x(&self) -> &TypeTag;
    fn coin_y(&self) -> &TypeTag;
    fn coin_x_price(&self) -> Option<U64F64>;
    fn coin_y_price(&self) -> Option<U64F64>;
    fn update_with_fields(&mut self, fields: &BTreeMap<String, SuiMoveValue>) -> Result<(), anyhow::Error>;
    fn pool_id(&self) -> &ObjectID;
}

dyn_clone::clone_trait_object!(Market);
