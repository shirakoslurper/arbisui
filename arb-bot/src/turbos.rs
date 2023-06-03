use std::str::FromStr;
use async_trait::async_trait;
use anyhow::{anyhow, Context};

use ethnum::U256;

use futures::{future, TryStreamExt};
use page_turner::PageTurner;
use serde_json::Value;
use fixed::types::U64F64;

use custom_sui_sdk::{
    SuiClient,
    apis::{
        QueryEventsRequest,
        GetDynamicFieldsRequest
    }
};

use sui_sdk::types::base_types::{ObjectID, ObjectIDParseError, ObjectType};
use sui_sdk::types::dynamic_field::DynamicFieldInfo;
use sui_sdk::rpc_types::{SuiObjectResponse, EventFilter, SuiEvent, SuiParsedData, SuiMoveStruct, SuiMoveValue, SuiObjectDataOptions};
 
use move_core_types::language_storage::{StructTag, TypeTag};
use std::collections::{BTreeMap, HashMap};

use crate::markets::{Exchange, Market};
use crate::sui_sdk_utils;
use crate::turbos_pool;

pub struct Turbos {
    package_id: ObjectID
}

impl Turbos {
    pub fn new(package_id: ObjectID) -> Self {
        Turbos {
            package_id,
        }
    }
}

impl FromStr for Turbos {
    type Err = anyhow::Error;

    fn from_str(package_id_str: &str) -> Result<Self, anyhow::Error> {
        Ok(
            Turbos {
                package_id: ObjectID::from_str(package_id_str).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
            }
        )
    }
}

impl Turbos {
    pub fn tickless_pool_from_fields(&self, fields: &BTreeMap<String, SuiMoveValue>) -> Result<turbos_pool::Pool, anyhow::Error> {
        // println!("{#?}")
        let protocol_fees_a = u64::from_str(
            if let SuiMoveValue::String(str_value) = fields
                .get("protocol_fees_a")
                .context("Missing field protocol_fees_a.")? {
                    str_value
                } else {
                    return Err(anyhow!("protocol_fees_a field does not match SuiMoveValue::String value."));
                }
        )?;

        let protocol_fees_b = u64::from_str(
            if let SuiMoveValue::String(str_value) = fields
                .get("protocol_fees_b")
                .context("Missing field protocol_fees_b.")? {
                    str_value
                } else {
                    return Err(anyhow!("protocol_fees_b field does not match SuiMoveValue::String value."));
                }
        )?;

        let sqrt_price = u128::from_str(
            if let SuiMoveValue::String(str_value) = fields
                .get("sqrt_price")
                .context("Missing field sqrt_price.")? {
                    str_value
                } else {
                    return Err(anyhow!("sqrt_price field does not match SuiMoveValue::String value."));
                }
        )?;

        let tick_spacing = if let SuiMoveValue::Number(num_value) = fields
            .get("tick_spacing")
            .context("Missing tick_spacing fee.")? {
                *num_value
            } else {
                return Err(anyhow!("tick_spacing field does not match SuiMoveValue::Number value."));
            };

        let max_liquidity_per_tick = u128::from_str(
            if let SuiMoveValue::String(str_value) = fields
                .get("max_liquidity_per_tick")
                .context("Missing field max_liquidity_per_tick.")? {
                    str_value
                } else {
                    return Err(anyhow!("max_liquidity_per_tick field does not match SuiMoveValue::String value."));
                }
        )?;

        let fee = if let SuiMoveValue::Number(num_value) = fields
            .get("fee")
            .context("Missing field fee.")? {
                *num_value
            } else {
                return Err(anyhow!("fee field does not match SuiMoveValue::Number value."));
            };

        let fee_protocol = if let SuiMoveValue::Number(num_value) = fields
            .get("fee_protocol")
            .context("Missing field fee_protocol.")? {
                *num_value
            } else {
                return Err(anyhow!("fee_protocol field does not match SuiMoveValue::Number value."));
            };

        let unlocked = if let SuiMoveValue::Bool(bool_value) = fields
            .get("unlocked")
            .context("Missing field unlocked.")? {
                *bool_value
            } else {
                return Err(anyhow!("unlocked field does not match SuiMoveValue::Number value."));
            };

        let fee_growth_global_a = u128::from_str(
            if let SuiMoveValue::String(str_value) = fields
                .get("fee_growth_global_a")
                .context("Missing field fee_growth_global_a.")? {
                    str_value
                } else {
                    return Err(anyhow!("fee_growth_global_a field does not match SuiMoveValue::String value."));
                }
        )?;

        let fee_growth_global_b = u128::from_str(
            if let SuiMoveValue::String(str_value) = fields
                .get("fee_growth_global_b")
                .context("Missing field fee_growth_global_b.")? {
                    str_value
                } else {
                    return Err(anyhow!("fee_growth_global_b field does not match SuiMoveValue::String value."));
                }
        )?;

        let liquidity = u128::from_str(
            if let SuiMoveValue::String(str_value) = fields
                .get("liquidity")
                .context("Missing field liquidity.")? {
                    str_value
                } else {
                    return Err(anyhow!("liquidity field does not match SuiMoveValue::String value."));
                }
        )?;

        let tick_current_index = if let SuiMoveValue::Struct(struct_value) = fields
            .get("tick_current_index")
            .context("Missing field tick_current_index.")? {
                // format!("0x{}", struct_value.address)
                // println!("tick_current_index: {:#?}", struct_value);
                if let SuiMoveStruct::WithTypes{ type_, fields } = struct_value {
                    if let SuiMoveValue::Number(num_value) = fields
                        .get("bits")
                        .context("Missing field bits.")? {
                            *num_value as i32
                        } else {
                            return Err(anyhow!("bits field does not match MoveValue::Number value."));
                        }
                } else {
                    return Err(anyhow!("struct_value does not match SuiMoveStruct::WithTypes value."));
                }
            } else {
                return Err(anyhow!("tick_current_index field does not match SuiMoveValue::String value."));
            };

        let initialized_ticks = BTreeMap::new();

        let tick_map_id = if let SuiMoveValue::Struct(struct_value) = fields
            .get("tick_map")
            .context("Missing field tick_map.")? {
                // format!("0x{}", struct_value.address)
                // println!("tick_current_index: {:#?}", struct_value);
                if let SuiMoveStruct::WithTypes{ type_, fields } = struct_value {
                    if let SuiMoveValue::UID{ id } = fields
                        .get("id")
                        .context("Missing field id.")? {
                            id.clone()
                        } else {
                            return Err(anyhow!("id field does not match MoveValue::UID value."));
                        }
                } else {
                    return Err(anyhow!("struct_value does not match SuiMoveStruct::WithTypes value."));
                }
            } else {
                return Err(anyhow!("tick_map_id field does not match SuiMoveValue::String value."));
            };

        let tick_map = BTreeMap::new();

        Ok(
            turbos_pool::Pool {
                protocol_fees_a,
                protocol_fees_b,
                sqrt_price,
                tick_current_index,
                tick_spacing,
                max_liquidity_per_tick,
                fee,
                fee_protocol,
                unlocked,
                fee_growth_global_a,
                fee_growth_global_b,
                liquidity,
                initialized_ticks, // new
                tick_map
            }
        )
    }

    pub async fn get_tick_map_with_id(
        &self, 
        sui_client: &SuiClient, 
        tick_map_id: &ObjectID
    ) -> Result<(), anyhow::Error> {
        let tick_map_dynamic_field_infos = sui_client
            .read_api()
            .pages(
                GetDynamicFieldsRequest {
                    object_id: tick_map_id.clone(), // We can make this consuming if it saves time
                    cursor: None,
                    limit: None,
                }
            )
            .items()
            .try_collect::<Vec<DynamicFieldInfo>>()
            .await?;
        
        let word_pos_and_word_ids = tick_map_dynamic_field_infos
            .iter()
            .map(|dynamic_field_info| {

                // word_pos is available! name field is of type i32
                let word_pos_field = dynamic_field_info
                    .name
                    .value
                    .get("bits")
                    .context("Missing field bits.")?;

                let word_pos = if let Value::Number(num_value) = word_pos_field {
                        num_value.as_i64().context("failed to cast num_value to i64")? as i32
                    } else {
                        return Err(anyhow!("bits field does not match MoveValue::Number value."));
                    };

                let word_id = dynamic_field_info.object_id;

                Ok((word_pos, word_id))
            })
            .collect::<Result<Vec<(i32, ObjectID)>, anyhow::Error>>()?;

        let chunked_word_pos_to_word = future::try_join_all(
            word_pos_and_word_ids
                    .chunks()
                    .iter()
                    .map(|word_pos_and_word_ids| {
                        async {
                            // let (word_poss, word_ids): (Vec<i32>, Vec<>) = word_pos_and_word_ids
                            //     .iter()
                            //     .cloned()
                            //     .unzip();

                            // let object_responses = sui_client
                            //     .read_api()
                            //     .multi_get_object_with_options(
                            //         pool_ids.to_vec(),
                            //         SuiObjectDataOptions::full_content()
                            //     )
                            //     .await?;

                            // AW FUCK HUGE ISSUE
                            // CAN'T GUARANTEE THAT ZIPPING
                            // WORKS PROPERLY - RPC must send in same order as request or
                            // we're fucked
                            // It seems to have been the case but could be dumb to rely on this
                            // Am sure the object ID is part of the Response

                            let pool_id_to_object_response = pool_ids
                                .iter()
                                .cloned()
                                .zip(object_responses.into_iter())
                                .collect::<HashMap<ObjectID, SuiObjectResponse>>();

                            Ok::<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error>(pool_id_to_object_response)
                        }
                    })

        )

        Ok(())
    }


}

#[async_trait]
impl Exchange for Turbos {
    fn package_id(&self) -> &ObjectID {
        &self.package_id
    }

    async fn get_all_markets(&self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error> {
        let pool_created_events = sui_client
            .event_api()
            .pages(
                QueryEventsRequest {
                    query: EventFilter::MoveEventType(
                        StructTag::from_str(
                            &format!("{}::pool_factory::PoolCreatedEvent", self.package_id)
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

        let pool_ids = pool_created_events
            .into_iter()
            .map(|pool_created_event| {
                let parsed_json = pool_created_event.parsed_json;
                if let Value::String(pool_id_value) = parsed_json.get("pool").context("Failed to get pool_id for a CetusMarket")? {
                    println!("pool_id: {}", pool_id_value);
                    Ok(ObjectID::from_str(&format!("0x{}", pool_id_value))?)
                } else {
                    Err(anyhow!("Failed to match pattern."))
                }
            })
            .collect::<Result<Vec<ObjectID>, anyhow::Error>>()?;


        let pool_id_to_object_response = sui_sdk_utils::get_pool_ids_to_object_response(sui_client, &pool_ids).await?;

        let markets = pool_id_to_object_response
            .into_iter()
            .map(|(pool_id, object_response)| {
                // println!("{:#?}", object_response);

                let fields = sui_sdk_utils::get_fields_from_object_response(&object_response)?;
                let (coin_x, coin_y) = get_coin_pair_from_object_response(&object_response)?;

                let coin_x_sqrt_price = U64F64::from_bits(
                    u128::from_str(
                        if let SuiMoveValue::String(str_value) = fields
                            .get("sqrt_price")
                            .context("Missing field sqrt_price.")? {
                                &str_value
                            } else {
                                return Err(anyhow!("sqrt_price field does not match SuiMoveValue::String value."));
                            }
                    )?
                );
        
                let coin_y_sqrt_price = U64F64::from_num(1) / coin_x_sqrt_price;

                Ok(
                    Box::new(
                        TurbosMarket {
                            coin_x,
                            coin_y,
                            pool_id,
                            coin_x_sqrt_price: Some(coin_x_sqrt_price),
                            coin_y_sqrt_price: Some(coin_y_sqrt_price),
                        }
                    ) as Box<dyn Market>
                )
            })
            .collect::<Result<Vec<Box<dyn Market>>, anyhow::Error>>()?;

        Ok(markets)
    }

    async fn get_pool_id_to_object_response(&self, sui_client: &SuiClient, markets: &[Box<dyn Market>]) -> Result<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error> {
        let pool_ids = markets
            .iter()
            .map(|market| {
                *market.pool_id()
            })
            .collect::<Vec<ObjectID>>();

        sui_sdk_utils::get_pool_ids_to_object_response(sui_client, &pool_ids).await
    }
}
#[derive(Debug, Clone)]
struct TurbosMarket {
    coin_x: TypeTag,
    coin_y: TypeTag,
    pool_id: ObjectID,
    coin_x_sqrt_price: Option<U64F64>, // In terms of y. x / y
    coin_y_sqrt_price: Option<U64F64>, // In terms of x. y / x
}

impl Market for TurbosMarket {
    fn coin_x(&self) -> &TypeTag {
        &self.coin_x
    }

    fn coin_y(&self) -> &TypeTag {
        &self.coin_y
    }

    fn coin_x_price(&self) -> Option<U64F64> {
        if let Some(coin_x_sqrt_price) = self.coin_x_sqrt_price {
            Some(coin_x_sqrt_price * coin_x_sqrt_price)
        } else {
            self.coin_x_sqrt_price
        }
    }

    fn coin_y_price(&self) -> Option<U64F64> {
        if let Some(coin_y_sqrt_price) = self.coin_y_sqrt_price {
            Some(coin_y_sqrt_price * coin_y_sqrt_price)
        } else {
            self.coin_y_sqrt_price
        }
    }

    fn update_with_fields(&mut self, fields: &BTreeMap<String, SuiMoveValue>) -> Result<(), anyhow::Error> {
        let coin_x_sqrt_price = U64F64::from_bits(
            u128::from_str(
                if let SuiMoveValue::String(str_value) = fields
                    .get("sqrt_price")
                    .context("Missing field sqrt_price.")? {
                        str_value
                    } else {
                        return Err(anyhow!("sqrt_price field does not match SuiMoveValue::String value."));
                    }
            )?
        );

        let coin_y_sqrt_price = U64F64::from_num(1) / coin_x_sqrt_price;
        
        self.coin_x_sqrt_price = Some(coin_x_sqrt_price);
        self.coin_y_sqrt_price = Some(coin_y_sqrt_price);

        // println!("coin_x<{}>: {}", self.coin_x, self.coin_x_price.unwrap());
        // println!("coin_y<{}>: {}\n", self.coin_y, self.coin_y_price.unwrap());

        Ok(())
    }

    fn pool_id(&self) -> &ObjectID {
        &self.pool_id
    }

}

fn get_coin_pair_from_object_response (
    response: &SuiObjectResponse
) -> Result<(TypeTag, TypeTag), anyhow::Error> {
    // println!("{:#?}", response);
    if let Some(data) = response.clone().data {
        if let Some(type_) = data.type_ {
            if let ObjectType::Struct(move_object_type) = type_ {
                let type_params = move_object_type.type_params();

                Ok(
                    (
                        type_params.get(0).context("Missing coin_x")?.clone(),
                        type_params.get(1).context("Missing coin_y")?.clone()
                    )
                )
            } else {
                Err(anyhow!("Does not match the ObjectType::Struct variant"))
            }
        } else {
            Err(anyhow!("Expected Some"))
        }
    } else {
        Err(anyhow!("Expected Some"))
    }
}