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

use crate::{markets::{Exchange, Market}, sui_sdk_utils::get_fields_from_object_response};
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
    pub async fn pool_from_fields(&self, sui_client: &SuiClient, fields: &BTreeMap<String, SuiMoveValue>) -> Result<turbos_pool::Pool, anyhow::Error> {
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

        let tick_map = Self::get_tick_map(sui_client, &tick_map_id).await?;

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

    pub async fn get_tick_map(
        sui_client: &SuiClient, 
        tick_map_id: &ObjectID
    ) -> Result<BTreeMap<i32, U256>, anyhow::Error> {
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
        
        let word_ids = tick_map_dynamic_field_infos
            .iter()
            .map(|dynamic_field_info| {
                Ok(dynamic_field_info.object_id)
            })
            .collect::<Result<Vec<ObjectID>, anyhow::Error>>()?;

        // The dynamic field object also holds word_pos in the field "name"
        // Tomorrow we'll refactor to work with a Vector SuiObjectResponses 
        let word_object_responses = sui_sdk_utils::get_object_responses(sui_client, &word_ids).await?;

        let word_pos_to_word = word_object_responses
            .into_iter()
            .map(|word_object_response| {
                let fields = get_fields_from_object_response(&word_object_response)?;
                let word_pos = if let SuiMoveValue::Struct(struct_value) = fields
                    .get("name")
                    .context("Missing field name.")? {
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
                        return Err(anyhow!("name field does not match SuiMoveValue::String value."));
                    };

                // Moving the casts/conversions to outside the if let makes this more modular
                let word = U256::from_str(
                    if let SuiMoveValue::String(str_value) = fields
                        .get("value")
                        .context("Missing field value.")? {
                            str_value
                        } else {
                            return Err(anyhow!("value field does not match SuiMoveValue::String value."));
                        }
                )?;

                Ok((word_pos, word))
            })
            .collect::<Result<BTreeMap<i32, U256>, anyhow::Error>>()?;

        Ok(word_pos_to_word)
    }

    pub async fn get_initialized_ticks(&self, sui_client: &SuiClient) -> Result<(), anyhow::Error>{
        let pool_dynamic_field_infos = sui_client
            .read_api()
            .pages(
                GetDynamicFieldsRequest {
                    object_id:self.package_id.clone(), // We can make this consuming if it saves time
                    cursor: None,
                    limit: None,
                }
            )
            .items()
            .try_collect::<Vec<DynamicFieldInfo>>()
            .await?;

        let tick_object_type = format!("{}::pool::Tick", self.package_id);

        let tick_object_ids = pool_dynamic_field_infos
            .into_iter()
            .filter(|dynamic_field_info| {
                tick_object_type == dynamic_field_info.object_type
            })
            .map(|tick_dynamic_field_info| {
                tick_dynamic_field_info.object_id
            })
            .collect::<Vec<ObjectID>>();

        let tick_object_responses = sui_sdk_utils::get_object_responses(sui_client, tick_object_ids).await?;

        // let tick_index_to_tick = tick_object_responses
        //     .into_iter()
        //     .map(|tick_object_response| {
        //         let fields = get_fields_from_object_response(&tick_object_response)?;
        //         let tick_move_struct = 

        //         let tick_index = if let SuiMoveValue::Struct(struct_value) = fields
        //             .get("name")
        //             .context("Missing field name.")? {
        //                 // format!("0x{}", struct_value.address)
        //                 // println!("tick_current_index: {:#?}", struct_value);
        //                 if let SuiMoveStruct::WithTypes{ type_, fields } = struct_value {
        //                     if let SuiMoveValue::Number(num_value) = fields
        //                         .get("bits")
        //                         .context("Missing field bits.")? {
        //                             *num_value as i32
        //                         } else {
        //                             return Err(anyhow!("bits field does not match MoveValue::Number value."));
        //                         }
        //                 } else {
        //                     return Err(anyhow!("struct_value does not match SuiMoveStruct::WithTypes value."));
        //                 }
        //             } else {
        //                 return Err(anyhow!("name field does not match SuiMoveValue::String value."));
        //             };

        //         let tick_fields = if let SuiMoveValue::Struct(struct_value) = fields
        //             .get("value")
        //             .context("Missing field value.")? {
        //                 // format!("0x{}", struct_value.address)
        //                 // println!("tick_current_index: {:#?}", struct_value);
        //                 if let SuiMoveStruct::WithTypes{ type_, fields } = struct_value {
        //                     fields
        //                 } else {
        //                     return Err(anyhow!("struct_value does not match SuiMoveStruct::WithTypes value."));
        //                 }
        //             } else {
        //                 return Err(anyhow!("name field does not match SuiMoveValue::String value."));
        //             };


        //         // Moving the casts/conversions to outside the if let makes this more modular
        //         let fee_growth_outside_a = u128::from_str(
        //             if let SuiMoveValue::String(str_value) = tick_fields
        //                 .get("fee_growth_outside_a")
        //                 .context("Missing field fee_growth_outside_a.")? {
        //                     str_value
        //                 } else {
        //                     return Err(anyhow!("fee_growth_outside_a field does not match SuiMoveValue::String value."));
        //                 }
        //         );

        //         // Moving the casts/conversions to outside the if let makes this more modular
        //         let fee_growth_outside_b = u128::from_str(
        //             if let SuiMoveValue::String(str_value) = tick_fields
        //                 .get("fee_growth_outside_b")
        //                 .context("Missing field fee_growth_outside_b.")? {
        //                     str_value
        //                 } else {
        //                     return Err(anyhow!("fee_growth_outside_b field does not match SuiMoveValue::String value."));
        //                 }
        //         );

        //         // Moving the casts/conversions to outside the if let makes this more modular
        //         let liquidity_gross = u128::from_str(
        //             if let SuiMoveValue::String(str_value) = tick_fields
        //                 .get("liquidity_gross")
        //                 .context("Missing field liquidity_gross.")? {
        //                     str_value
        //                 } else {
        //                     return Err(anyhow!("liquidity_gross field does not match SuiMoveValue::String value."));
        //                 }
        //         );



        //         Ok((tick_index, word))
        //     })
        //     .collect::<Result<BTreeMap<i32, turbos_pool::Tick>, anyhow::Error>>()?;


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