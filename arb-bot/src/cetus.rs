use std::str::FromStr;
use async_trait::async_trait;
use anyhow::{anyhow, Context};

use futures::{future, TryStreamExt};
use page_turner::PageTurner;
use serde_json::Value;
use fixed::types::U64F64;

use custom_sui_sdk::{
    SuiClient,
    apis::QueryEventsRequest
};

use sui_sdk::types::base_types::{ObjectID, ObjectIDParseError};
use sui_sdk::rpc_types::{SuiObjectDataOptions, SuiObjectResponse, EventFilter, SuiEvent, SuiParsedData, SuiMoveStruct, SuiMoveValue};
 
use move_core_types::language_storage::{StructTag, TypeTag};
use std::collections::{BTreeMap, HashMap};

use crate::markets::{Exchange, Market};

// const GLOBAL: &str = "0xdaa46292632c3c4d8f31f23ea0f9b36a28ff3677e9684980e4438403a67a3d8f";
// const POOLS: &str = "0xf699e7f2276f5c9a75944b37a0c5b5d9ddfd2471bf6242483b03ab2887d198d0";

const OBJECT_REQUEST_LIMIT: usize = 50;

pub struct Cetus {
    package_id: ObjectID
}

impl Cetus {
    pub fn new(package_id: ObjectID) -> Self {
        Cetus {
            package_id,
        }
    }
}

impl FromStr for Cetus {
    type Err = anyhow::Error;

    fn from_str(package_id_str: &str) -> Result<Self, anyhow::Error> {
        Ok(
            Cetus {
                package_id: ObjectID::from_str(package_id_str).map_err(|err| <ObjectIDParseError as Into<anyhow::Error>>::into(err))?,
            }
        )
    }
}

#[async_trait]
impl Exchange for Cetus {
    fn package_id(&self) -> &ObjectID {
        &self.package_id
    }

    // Cetus has us query for events
    async fn get_all_markets(&self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error> {

        let pool_created_events = sui_client
            .event_api()
            .pages(
                QueryEventsRequest {
                    query: EventFilter::MoveEventType(
                        StructTag::from_str(
                            &format!("{}::factory::CreatePoolEvent", self.package_id)
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

        Ok(markets)
    }

    // Lets return a map instead
    async fn get_pool_id_to_fields(&self, sui_client: &SuiClient, markets: &[Box<dyn Market>]) -> Result<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>, anyhow::Error> {
        let pool_ids = markets
            .iter()
            .map(|market| {
                market.pool_id().clone()
            })
            .collect::<Vec<ObjectID>>();

        let chunked_pool_object_responses = future::try_join_all(
            pool_ids
            .chunks(OBJECT_REQUEST_LIMIT)
            .map(|pool_ids| {
                async {
                    let pool_object_responses = sui_client
                        .read_api()
                        .multi_get_object_with_options(
                            pool_ids.to_vec(),
                            SuiObjectDataOptions::full_content()
                        )
                        .await?;

                    let fields = pool_object_responses
                        .into_iter()
                        .map(|pool_object_response| {
                            get_fields_from_object_response(pool_object_response)
                        })
                        .collect::<Result<Vec<BTreeMap<String, SuiMoveValue>>, anyhow::Error>>()?;

                    let pool_id_to_fields = pool_ids
                        .iter()
                        .cloned()
                        .zip(fields.into_iter())
                        .collect::<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>>();

                    Ok::<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>, anyhow::Error>(pool_id_to_fields)
                }
            })
        )
        .await?;

        let pool_object_responses = chunked_pool_object_responses
            .into_iter()
            .flatten()
            .collect::<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>>();

        Ok(pool_object_responses)
    }
}

#[derive(Debug, Clone)]
struct CetusMarket {
    coin_x: TypeTag,
    coin_y: TypeTag,
    pool_id: ObjectID,
    coin_x_price: Option<U64F64>, // In terms of y. x / y
    coin_y_price: Option<U64F64>, // In terms of x. y / x
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

    fn update_with_fields(&mut self, fields: &BTreeMap<String, SuiMoveValue>) -> Result<(), anyhow::Error> {
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

        // println!("coin_x<{}>: {}", self.coin_x, self.coin_x_price.unwrap());
        // println!("coin_y<{}>: {}\n", self.coin_y, self.coin_y_price.unwrap());

        Ok(())
    }

    fn pool_id(&self) -> &ObjectID {
        &self.pool_id
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