use std::{str::FromStr};
use async_trait::async_trait;
use anyhow::{anyhow, Context};

use futures::{future, TryStreamExt};
use page_turner::PageTurner;
use serde_json::Value;
use fixed::{types::{U64F64}};

use custom_sui_sdk::{
    SuiClient,
    apis::QueryEventsRequest
};

use sui_sdk::types::{base_types::{ObjectID}};
use sui_sdk::rpc_types::{SuiObjectDataOptions, SuiObjectResponse, EventFilter, SuiEvent, SuiParsedData, SuiMoveStruct, SuiMoveValue};
 
use move_core_types::language_storage::{StructTag, TypeTag};
use std::collections::BTreeMap;

use crate::markets::{Exchange, Market};

const EXCHANGE_ADDRESS: &str = "0x1eabed72c53feb3805120a081dc15963c204dc8d091542592abaf7a35689b2fb";
const GLOBAL: &str = "0xdaa46292632c3c4d8f31f23ea0f9b36a28ff3677e9684980e4438403a67a3d8f";
const POOLS: &str = "0xf699e7f2276f5c9a75944b37a0c5b5d9ddfd2471bf6242483b03ab2887d198d0";

pub struct Cetus;

#[async_trait]
impl Exchange for Cetus {
    fn package_id(&self) -> Result<ObjectID, anyhow::Error> {
        ObjectID::from_str(EXCHANGE_ADDRESS).map_err(|err| err.into())
    }

    // Cetus has us query for events
    async fn get_all_markets(&self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error> {

        // TODO: Write page turner
        let pool_created_events = sui_client
            .event_api()
            .pages(
                QueryEventsRequest {
                    query: EventFilter::MoveEventType(
                        StructTag::from_str(
                            &format!("{}::factory::CreatePoolEvent", EXCHANGE_ADDRESS)
                        )?
                    ),
                    cursor: None,
                    limit: None,
                    descending_order: true,
                }
            )
            .items()
            .try_collect::<Vec<SuiEvent>>()
            .await?;

        let mut markets: Vec<Box<dyn Market>> = Vec::new();

        for pool_created_event in pool_created_events {
            let parsed_json = &pool_created_event.parsed_json;
            if let (
                Value::String(coin_x_value), 
                Value::String(coin_y_value), 
                Value::String(pool_id_value)
            ) = 
                (
                    parsed_json.get("coin_type_a").context("Failed to get coin_type_a for a CetusMarket")?,
                    parsed_json.get("coin_type_b").context("Failed to get coin_type_b for a CetusMarket")?,
                    parsed_json.get("pool_id").context("Failed to get pool_id for a CetusMarket")?
                ) {
                    let coin_x = TypeTag::from_str(&format!("0x{}", coin_x_value))?;
                    let coin_y = TypeTag::from_str(&format!("0x{}", coin_y_value))?;
                    let pool_id = ObjectID::from_str(&format!("0x{}", pool_id_value))?;

                    // println!("{:#?}", coin_y);

                    markets.push(
                        Box::new(
                            CetusMarket {
                                coin_x,
                                coin_y,
                                pool_id,
                                coin_x_price: None,
                                coin_y_price: None,
                            }
                        )
                    );
                } else {
                    return Err(anyhow!("Failed to match pattern."));
                }
        }

        // println!("{:#?}", markets);
        // let pool_ids = markets
        //     .iter()
        //     .map(|market| market.pool_id)
        //     .collect::<Vec<ObjectID>>();
        
        // println!("# pool_ids {}", pool_ids.len());

        // let pools_fields = CetusMarket::get_markets_info(sui_client, pool_ids).await?;

        Ok(markets)
    }

}

#[derive(Debug)]
struct CetusMarket {
    coin_x: TypeTag,
    coin_y: TypeTag,
    pool_id: ObjectID,
    coin_x_price: Option<U64F64>, // In terms of y. x / y
    coin_y_price: Option<U64F64>, // In terms of x. y / x
}

const OBJECT_REQUEST_LIMIT: usize = 50;

impl CetusMarket {
    // We're getting the fields
    // WE could rename this and make it a bit more general
    // async fn get_objects_fields
    async fn get_markets_info(sui_client: &SuiClient, pool_ids: Vec<ObjectID>) -> Result<Vec<BTreeMap<String, SuiMoveValue>>, anyhow::Error> {
        // 50 Object Requests Limit
        // Make requests for pool_ids in batches of 50
        let chunked_pool_ids = pool_ids.chunks(OBJECT_REQUEST_LIMIT);

        let chunked_pool_object_responses = future::try_join_all(
            chunked_pool_ids
            .map(|pool_ids| {
                async {
                    let pool_object_responses = sui_client
                    .read_api()
                    .multi_get_object_with_options(
                        pool_ids.to_vec(),
                        SuiObjectDataOptions::full_content()
                    )
                    .await?;

                    let pool_fields = pool_object_responses
                        .into_iter()
                        .map(|pool_object_response| {
                            get_fields_from_object_response(pool_object_response)
                        })
                        .collect::<Result<Vec<BTreeMap<String, SuiMoveValue>>, anyhow::Error>>()?;

                    Ok::<Vec<BTreeMap<String, SuiMoveValue>>, anyhow::Error>(pool_fields)
                }
            })
        )
        .await?;

        let pool_object_responses = chunked_pool_object_responses
            .into_iter()
            .flatten()
            .collect::<Vec<BTreeMap<String, SuiMoveValue>>>();

        Ok(pool_object_responses)
    }

    fn update_market(&mut self, fields: &BTreeMap<String, SuiMoveValue>) -> Result <(), anyhow::Error> {
        let coin_x_price = U64F64::from_bits(
            u128::from_str(
                if let SuiMoveValue::String(str_value) = fields
                    .get("current_sqrt_price")
                    .context("Missing field current_sqrt_price.")? {
                        str_value
                    } else {
                        return Err(anyhow!("current_sqrt_price field does not match SuiMoveValue::String value."));
                    }
            )?
        );

        let coin_y_price = U64F64::from_num(1) / coin_x_price;
        
        self.coin_x_price = Some(coin_x_price);
        self.coin_y_price = Some(coin_y_price);

        Ok(())
    }
}

impl Market for CetusMarket {
    fn coin_x(&self) -> &TypeTag {
        &self.coin_x
    }

    fn coin_y(&self) -> &TypeTag {
        &self.coin_y
    }

    fn coin_x_price(&self) -> Option<U64F64> {
        self.coin_x_price
    }

    fn coin_y_price(&self) -> Option<U64F64> {
        self.coin_y_price
    }
}

// Helpers

// We'll need to deal with the math on this side
// Price is simple matter of ((current_sqrt_price / (2^64))^2) * (10^(a - b))
fn get_fields_from_object_response(response: SuiObjectResponse) -> Result<BTreeMap<String, SuiMoveValue>, anyhow::Error> {
    if let Some(object_data) = response.data {
        if let Some(parsed_data) = object_data.content {
            if let SuiParsedData::MoveObject(parsed_move_object) = parsed_data {
                if let SuiMoveStruct::WithFields(field_map) = parsed_move_object.fields {
                    // println!("{:#?}", field_map.get("current_sqrt_price").context("Could not get current_sqrt_price from fields")?);
                    Ok(field_map)
                } else {
                    Err(anyhow!("Does not match the SuiMoveStruct::WithFields variant"))
                }
            } else {
                Err(anyhow!("Does not match the SuiParsedData::MoveObject variant"))
            }
        } else {
            Err(anyhow!("Expected Some"))
        }
    } else {
        Err(anyhow!("Expected Some"))
    }
}