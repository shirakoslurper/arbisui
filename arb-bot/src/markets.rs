use sui_sdk::types::base_types::ObjectID;
use anyhow::Result;

pub trait Exchange: Send + Sync {
    fn package_id(&self) -> Result<ObjectID, anyhow::Error>;
    // fn get_all_markets(&self) -> Result<()>; // -> Result<Vec<Box<dyn Market>>>
}

// pub trait Market: Send + Sync {

// }