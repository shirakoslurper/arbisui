// #![feature(async_closure)]

use std::str::FromStr;
use sui_sdk::types::base_types::ObjectID;
use custom_sui_sdk::SuiClient;
use async_trait::async_trait;
// use anyhow::{anyhow, Context};

use sui_sdk::rpc_types::{SuiObjectDataOptions, SuiObjectResponseQuery, SuiObjectResponse};
use sui_sdk::types::base_types::ObjectType;
// use sui_sdk::types::dynamic_field::DynamicFieldInfo;
// use sui_sdk::error::{Error, SuiRpcResult};

use crate::markets::Exchange;

const EXCHANGE_ADDRESS: &str = "0x6b84da4f5dc051759382e60352377fea9d59bc6ec92dc60e0b6387e05274415f";
// const GLOBAL: &str = "0x3083e3d751360c9084ba33f6d9e1ad38fb2a11cffc151f2ee4a5c03da61fb1e2";
const POOLS: &str = "0x6edec171d3b4c6669ac748f6de77f78635b72aac071732b184677db19eefd9e8";

pub struct FlameSwap;

#[async_trait]
impl Exchange for FlameSwap {
    fn package_id(&self) -> Result<ObjectID, anyhow::Error> {
        ObjectID::from_str(EXCHANGE_ADDRESS).map_err(|err| err.into())
    }

    async fn get_all_markets(&self, sui_client: &SuiClient) -> Result<(), anyhow::Error> {

        // Returns a DynamicFieldPage
        let pools_dynamic_fields = sui_client
            .read_api()
            .get_dynamic_fields(
                ObjectID::from_str(POOLS)?,
                None,
                None
            )
            .await?;

        // There will be multiple pages so we have to do a while has_next_page
        // to get all pools
        println!("Cursor Next: {:#?}", pools_dynamic_fields.has_next_page);

        let pool_object_ids = pools_dynamic_fields
            .data
            .iter()
            .map(|field| {
                field.object_id
            })
            .collect::<Vec<ObjectID>>();

        println!(
            "Num pools: {:#?}", 
            pool_object_ids.len()
        );

        let pools = sui_client
            .read_api()
                .multi_get_object_with_options(
                pool_object_ids,
                SuiObjectDataOptions::full_content()
            )
            .await?;

        // println!("{:#?}", pools);

        pools.into_iter().for_each(|pool| {
            if let Some(data) = pool.data {
                if let Some(type_) = data.type_ {
                    if let ObjectType::Struct(move_object_type) = type_ {
                        move_object_type
                            .type_params()
                            .iter()
                            .for_each(|type_param| {
                                println!("{:#?}", type_param)
                            })
                    }
                }
            }
        });

        Ok(())
    }

}

// struct FlameswapMarket {
//     coin_x: ObjectID,
//     coin_y: ObjectID,
// }

// fn markets_from_sui_object_response(pools: Vec<SuiObjectResponse>) -> Vec<FlameswapMarket> {

//     vec![]
// }

// fn market_from_sui_object_response(pool: SuiObjectResponse) -> Result<(), anyhow::Error> {
//     if let Some(data) = pool.data {
//         if let Some(type_) = data.type_ {
//             if let ObjectType::Struct(move_object_type) = type_ {
//                 move_object_type
//                     .type_params()
//                     .iter()
//                     .map(|type_param| {
//                         println!("{:#?}", type_param)
//                     })
//             }
//         }
//     }


// }