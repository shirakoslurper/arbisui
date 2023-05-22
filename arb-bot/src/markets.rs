use move_core_types::language_storage::StructTag;
use sui_sdk::types::base_types::ObjectID;
use custom_sui_sdk::SuiClient;
use async_trait::async_trait;
// use anyhow::Result;

#[async_trait]
pub trait Exchange: Send + Sync {
    fn package_id(&self) -> Result<ObjectID, anyhow::Error>;
    async fn get_all_markets(&self, sui_client: &SuiClient) -> Result<(), anyhow::Error>; // -> Result<Vec<Box<dyn Market>>>
    // async fn get_markets_info(&self, markets: Vec<impl Market>) -> Result<(), anyhow::Error>;
}

// pub trait Market: Send + Sync {
//     fn coin_x() -> &StructTag;
//     fn coin_y() -> &StructTag;
//     fn pool_id() -> &ObjectID;
// }