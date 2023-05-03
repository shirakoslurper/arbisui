use sui_sdk::types::base_types::ObjectID;

pub trait Exchange: Send + Sync {
    fn package_id(&self) -> &ObjectID;
    // fn get_all_markets(&self)
}
