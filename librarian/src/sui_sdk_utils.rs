use anyhow::{anyhow, Context};

use custom_sui_sdk::SuiClient;

use futures::{future, TryStreamExt};

use move_core_types::language_storage::TypeTag;

use page_turner::PageTurner;

use std::str::FromStr;
use std::collections::{HashMap, BTreeMap};

use custom_sui_sdk::apis::QueryObjectsRequest;

use sui_sdk::types::base_types::{ObjectID, ObjectType, SequenceNumber};
use sui_sdk::rpc_types::{
    Checkpoint, CheckpointId, QueryObjectsPage, SuiObjectResponse, SuiObjectData, 
    SuiObjectDataOptions, SuiObjectDataFilter, SuiObjectResponseQuery, SuiParsedData, 
    SuiMoveStruct, SuiMoveValue, SuiGetPastObjectRequest, SuiPastObjectResponse
};


use crate::constants::OBJECT_REQUEST_LIMIT;

// Should return Option - would be more intuitive...
pub fn get_fields_from_object_response(
    object_response: &SuiObjectResponse
) -> Option<BTreeMap<String, SuiMoveValue>> {
    if let Some(object_data) = &object_response.data {
        if let Some(parsed_data) = &object_data.content {
            if let SuiParsedData::MoveObject(parsed_move_object) = parsed_data {
                if let SuiMoveStruct::WithFields(field_map) = &parsed_move_object.fields {
                    Some(field_map.clone())
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
    if let Some(object_data) = &response.data {
        if let Some(parsed_data) = &object_data.content {
            if let SuiParsedData::MoveObject(parsed_move_object) = parsed_data {
                Some(parsed_move_object.fields.clone())
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

pub fn read_fields_from_object_data(
    object_data: &SuiObjectData
) -> Option<SuiMoveStruct> {
    if let Some(parsed_data) = &object_data.content {
        if let SuiParsedData::MoveObject(parsed_move_object) = parsed_data {
            Some(parsed_move_object.fields.clone())
        } else {
            None
        }
    } else {
        None
    }
}

pub fn read_version_from_object_response(
    response: &SuiObjectResponse
) -> Option<SequenceNumber> {
    if let Some(object_data) = &response.data {
        Some(object_data.version)
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

// oh fuck i just realized - if there is no cursor, there is no checkpoint.
// wait nvm we get a next cursor regardless of whether there is a next page
// phewwwww
pub async fn get_checkpoint_pinned_object_responses(
    sui_client: &SuiClient, 
    object_ids: Vec<ObjectID>
) -> Result<(Checkpoint, Vec<SuiObjectResponse>), anyhow::Error> {
    let object_data_filter = SuiObjectDataFilter::ObjectIds(object_ids);
    let object_data_options = SuiObjectDataOptions::full_content();

    let object_response_query = SuiObjectResponseQuery {
        filter: Some(object_data_filter),
        options: Some(object_data_options)
    };

    let mut result = Vec::new();

    println!("bugaboo");

    let QueryObjectsPage {
        data,
        next_cursor,
        has_next_page,
    } = sui_client
        .extended_api()
        .query_objects(
            object_response_query.clone(), 
            None, 
            None
        )
        .await?;

    println!("toodaloo");

    result.extend(
        data.iter()
            .map(|r| r.clone().try_into())
            .collect::<Result<Vec<_>, _>>()?,
    );

    if has_next_page {
        let data = sui_client
            .extended_api()
            .pages(
                QueryObjectsRequest {
                    query: object_response_query,
                    cursor: next_cursor.clone(),
                    limit: None
                }
            )
            .items()
            .try_collect::<Vec<SuiObjectResponse>>()
            .await?;

        result.extend(
            data.iter()
                .map(|r| r.clone().try_into())
                .collect::<Result<Vec<_>, _>>()?,
        );
    }

    let checkpoint = sui_client
        .read_api()
        .get_checkpoint(
            CheckpointId::SequenceNumber(
                next_cursor
                    .context("cursor is None")?
                    .at_checkpoint
                    .context("at_checkpoint is None")?
            )
        )
        .await?;

    Ok((checkpoint, result))
}

pub async fn get_past_object_responses(
    sui_client: &SuiClient, 
    past_object_requests: &[SuiGetPastObjectRequest]
) -> Result<Vec<SuiPastObjectResponse>, anyhow::Error> {
    let chunked_past_object_responses = future::try_join_all(
        past_object_requests
        .chunks(OBJECT_REQUEST_LIMIT)
        .map(|past_object_requests| {
            async {
                let object_responses = sui_client
                    .read_api()
                    .try_multi_get_parsed_past_object(
                        past_object_requests.to_vec(),
                        SuiObjectDataOptions::full_content()
                    )
                    .await?;

                Ok::<Vec<SuiPastObjectResponse>, anyhow::Error>(object_responses)
            }
        })
    )
    .await?;

    let past_object_responses = chunked_past_object_responses
        .into_iter()
        .flatten()
        .collect::<Vec<SuiPastObjectResponse>>();

    Ok(past_object_responses)
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
                // let ret = U256::from_le_bytes(
                //     hex_value.to_inner()
                // );

                // let ret = U256::from_str_hex(&format!("{}", hex_value))?;

                // This is what works but it makes dealing with actual hex values 
                // a little difficult....
                let ret = U256::from_str(&format!("{}", hex_value)[2..])?;

                // panic!("ret: {}", ret);
                // println!("RAAAAAAAA from 'hex' {}: fields:{}", hex_value, sui_move_struct);
                // panic!();

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