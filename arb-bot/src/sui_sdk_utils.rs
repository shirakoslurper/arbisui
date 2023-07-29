use anyhow::{anyhow, Context};

use custom_sui_sdk::SuiClient;

use futures::{future, TryStreamExt};

use move_core_types::language_storage::TypeTag;

use std::str::FromStr;
use std::collections::{HashMap, BTreeMap};

use sui_sdk::types::base_types::{ObjectID, ObjectType};
use sui_sdk::rpc_types::{SuiObjectResponse, SuiObjectDataOptions, SuiParsedData, SuiMoveStruct, SuiMoveValue};

use crate::constants::OBJECT_REQUEST_LIMIT;

// Should return Option - would be more intuitive...
pub fn get_fields_from_object_response(
    object_response: &SuiObjectResponse
) -> Option<BTreeMap<String, SuiMoveValue>> {
    if let Some(object_data) = object_response.clone().data {
        if let Some(parsed_data) = object_data.content {
            if let SuiParsedData::MoveObject(parsed_move_object) = parsed_data {
                if let SuiMoveStruct::WithFields(field_map) = parsed_move_object.fields {
                    Some(field_map)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

// Switching to this just makes things little more flexible than the above
// Can choose when to throw an error
// Works for SuiMoveStruct::WithTypes as well
// Instead of get() we call read_dynamic_field_value() which returns a value (not reference). 
// Otherwise nearly identical.
// If the additional clones prove to be detrimental than it is a simple switch back
pub fn read_fields_from_object_response(
    response: &SuiObjectResponse
) -> Option<SuiMoveStruct> {
    if let Some(object_data) = response.clone().data {
        if let Some(parsed_data) = object_data.content {
            if let SuiParsedData::MoveObject(parsed_move_object) = parsed_data {
                Some(parsed_move_object.fields)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

pub async fn get_object_responses(
    sui_client: &SuiClient, 
    object_ids: &[ObjectID]
) -> Result<Vec<SuiObjectResponse>, anyhow::Error> {
    let chunked_object_responses = future::try_join_all(
        object_ids
        .chunks(OBJECT_REQUEST_LIMIT)
        .map(|object_ids| {
            async {
                let object_responses = sui_client
                    .read_api()
                    .multi_get_object_with_options(
                        object_ids.to_vec(),
                        SuiObjectDataOptions::full_content()
                    )
                    .await?;

                Ok::<Vec<SuiObjectResponse>, anyhow::Error>(object_responses)
            }
        })
    )
    .await?;

    let object_responses = chunked_object_responses
        .into_iter()
        .flatten()
        .collect::<Vec<SuiObjectResponse>>();

    Ok(object_responses)
}

pub async fn get_object_id_to_object_response(
    sui_client: &SuiClient, 
    object_ids: &[ObjectID]
) -> Result<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error> {
    let chunked_object_responses = future::try_join_all(
        object_ids
        .chunks(OBJECT_REQUEST_LIMIT)
        .map(|object_ids| {
            async {
                let object_responses = sui_client
                    .read_api()
                    .multi_get_object_with_options(
                        object_ids.to_vec(),
                        SuiObjectDataOptions::full_content()
                    )
                    .await?;

                Ok::<Vec<SuiObjectResponse>, anyhow::Error>(object_responses)
            }
        })
    )
    .await?;

    let object_id_to_object_responses = chunked_object_responses
        .into_iter()
        .flatten()
        .map(|object_response| {
            Ok((object_response.object_id()?, object_response))
        })
        .collect::<Result<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error>>()?;

    Ok(object_id_to_object_responses)
}

pub mod sui_move_value {
    use super::*;
    use ethnum::U256;

    // // U256: Decimal string or Hex
    // pub fn read_field_as_u128(
    //     sui_move_struct: &SuiMoveStruct,
    //     field: &str
    // ) -> Result<U256, anyhow::Error> {
    //     let dynamic_field_value = sui_move_struct
    //         .read_dynamic_field_value(field)
    //         .context(format!("Missing field '{}'.", field))?;

    //     match dynamic_field_value {
    //         SuiMoveValue::String(decimal_string_value) => {
    //             Ok(u128::from_str(&decimal_string_value)?)
    //         },
    //         SuiMoveValue::Address(hex_value) => {
    //             Ok(
    //                 u128::from_le_bytes(
    //                     hex_value
    //                         .to_vec()
    //                         .try_into()
    //                         .map_err(|err| anyhow!(format!("Failed to convert {:?} into [u8, 32]", err)))
    //                         .context("Failed to convert hex_value U256's Vec<U8> to [u8, 32].")?
    //                 )
    //             )
    //         },
    //         _ => {
    //             Err(anyhow!(format!("'{}' U256 field must be encoded as a SuiMoveValue::String (decimal string) or SuiMoveValue::Address (hex) value.", field)))
    //         }
    //     }
    // }

    // Note: Given that Sui types can be encoded in various ways
    // in JSON, it makes sense to try converting from those types
    // into the Sui type we want (or the closest matching type we have in Rust)
    // U256: Decimal string or Hex
    pub fn read_field_as_u256(
        sui_move_struct: &SuiMoveStruct,
        field: &str
    ) -> Result<U256, anyhow::Error> {
        let dynamic_field_value = sui_move_struct
            .read_dynamic_field_value(field)
            .context(format!("Missing field '{}'.", field))?;

        match dynamic_field_value {
            SuiMoveValue::String(decimal_string_value) => {
                Ok(U256::from_str(&decimal_string_value)?)
            },
            SuiMoveValue::Address(hex_value) => {
                let ret = U256::from_be_bytes(
                    hex_value.to_inner()
                );

                // panic!("ret: {}", ret);
                println!("RAAAAAAAA");

                Ok(
                    ret
                )
            },
            _ => {
                Err(anyhow!(format!("'{}' U256 field must be encoded as a SuiMoveValue::String (decimal string) or SuiMoveValue::Address (hex) value.", field)))
            }
        }
    }


    pub fn get_number(sui_move_struct: &SuiMoveStruct, field: &str) -> Result<u32, anyhow::Error> {
        if let SuiMoveValue::Number(num_value) = sui_move_struct
            .read_dynamic_field_value(field)
            .context(format!("Missing field '{}'.", field))? {
                Ok(num_value)
            } else {
                Err(anyhow!(format!("'{}' field does not match SuiMoveValue::Number variant.", field)))
            }
    }

    pub fn get_string(sui_move_struct: &SuiMoveStruct, field: &str) -> Result<String, anyhow::Error> {
        if let SuiMoveValue::String(str_value) = sui_move_struct
            .read_dynamic_field_value(field)
            .context(format!("Missing field '{}'.", field))? {
                Ok(str_value)
            } else {
                Err(anyhow!(format!("'{}' field does not match SuiMoveValue::String variant.", field)))
            }
    }

    pub fn get_bool(sui_move_struct: &SuiMoveStruct, field: &str) -> Result<bool, anyhow::Error> {
        if let SuiMoveValue::Bool(bool_value) = sui_move_struct
            .read_dynamic_field_value(field)
            .context(format!("Missing field '{}'.", field))? {
                Ok(bool_value)
            } else {
                Err(anyhow!(format!("'{}' field does not match SuiMoveValue::Bool variant.", field)))
            }
    }

    pub fn get_struct(sui_move_struct: &SuiMoveStruct, field: &str) -> Result<SuiMoveStruct, anyhow::Error> {
        if let SuiMoveValue::Struct(struct_value) = sui_move_struct
            .read_dynamic_field_value(field)
            .context(format!("Missing field '{}'.", field))? {
                Ok(struct_value)
            } else {
                Err(anyhow!(format!("'{}' field does not match SuiMoveValue::Struct variant.", field)))
            }
    }

    pub fn get_uid(sui_move_struct: &SuiMoveStruct, field: &str) -> Result<ObjectID, anyhow::Error> {
        if let SuiMoveValue::UID{ id }= sui_move_struct
            .read_dynamic_field_value(field)
            .context(format!("Missing field '{}'.", field))? {
                Ok(id)
            } else {
                Err(anyhow!(format!("'{}' field does not match SuiMoveValue::UID variant.", field)))
            }
    }
}