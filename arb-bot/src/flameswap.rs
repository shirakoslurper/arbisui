use std::str::FromStr;
use sui_sdk::types::base_types::ObjectID;
// use anyhow::Result;

use crate::markets::Exchange;

const EXCHANGE_ADDRESS: &str = "0x6b84da4f5dc051759382e60352377fea9d59bc6ec92dc60e0b6387e05274415f";

pub struct FlameSwap;

impl Exchange for FlameSwap {
    fn package_id(&self) -> Result<ObjectID, anyhow::Error> {
        ObjectID::from_str(EXCHANGE_ADDRESS).map_err(|err| err.into())
    }

    // fn get_all_markets(&self) -> Result<()> {

    // }

}