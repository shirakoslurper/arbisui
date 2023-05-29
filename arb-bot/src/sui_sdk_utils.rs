use anyhow::{anyhow, Context};

use custom_sui_sdk::SuiClient;

use futures::{future, TryStreamExt};

use move_core_types::language_storage::TypeTag;

use std::collections::{HashMap, BTreeMap};

use sui_sdk::types::base_types::{ObjectID, ObjectType};
use sui_sdk::rpc_types::{SuiObjectResponse, SuiObjectDataOptions, SuiParsedData, SuiMoveStruct, SuiMoveValue};

use crate::constants::OBJECT_REQUEST_LIMIT;

// We'll need to deal with the math on this side
// Price is simple matter of ((current_sqrt_price / (2^64))^2) * (10^(a - b))
pub fn get_fields_from_object_response(
    response: &SuiObjectResponse
) -> Result<BTreeMap<String, SuiMoveValue>, anyhow::Error> {
    if let Some(object_data) = response.clone().data {
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

pub async fn get_pool_ids_to_object_response(
    sui_client: &SuiClient, 
    pool_ids: &[ObjectID]
) -> Result<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error> {
    let chunked_pool_id_to_object_response = future::try_join_all(
        pool_ids
        .chunks(OBJECT_REQUEST_LIMIT)
        .map(|pool_ids| {
            async {
                let object_responses = sui_client
                    .read_api()
                    .multi_get_object_with_options(
                        pool_ids.to_vec(),
                        SuiObjectDataOptions::full_content()
                    )
                    .await?;

                let pool_id_to_object_response = pool_ids
                    .iter()
                    .cloned()
                    .zip(object_responses.into_iter())
                    .collect::<HashMap<ObjectID, SuiObjectResponse>>();

                Ok::<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error>(pool_id_to_object_response)
            }
        })
    )
    .await?;

    let pool_id_to_object_response = chunked_pool_id_to_object_response
        .into_iter()
        .flatten()
        .collect::<HashMap<ObjectID, SuiObjectResponse>>();

    Ok(pool_id_to_object_response)
}

pub fn fields_from_pool_id_to_object_response(
    pool_id_to_object_response: HashMap<ObjectID, SuiObjectResponse>
) -> Result<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>, anyhow::Error> {
    let pool_id_to_fields = pool_id_to_object_response
        .iter()
        .map(|(pool_id, object_response)| {
            Ok(
                (
                    pool_id.clone(), 
                    get_fields_from_object_response(object_response)?
                )
            )
        })
        .collect::<Result<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>, anyhow::Error>>()?;

    Ok(pool_id_to_fields)
}

// pub async fn get_pool_id_to_fields(
//     sui_client: &SuiClient, 
//     pool_ids: &[ObjectID]
// ) -> Result<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>, anyhow::Error> {
//     let chunked_pool_id_to_fields = future::try_join_all(
//         pool_ids
//         .chunks(OBJECT_REQUEST_LIMIT)
//         .map(|pool_ids| {
//             async {
//                 let pool_object_responses = sui_client
//                     .read_api()
//                     .multi_get_object_with_options(
//                         pool_ids.to_vec(),
//                         SuiObjectDataOptions::full_content()
//                     )
//                     .await?;

//                 let fields = pool_object_responses
//                     .into_iter()
//                     .map(|pool_object_response| {
//                         get_fields_from_object_response(&pool_object_response)
//                     })
//                     .collect::<Result<Vec<BTreeMap<String, SuiMoveValue>>, anyhow::Error>>()?;

//                 let pool_id_to_fields = pool_ids
//                     .iter()
//                     .cloned()
//                     .zip(fields.into_iter())
//                     .collect::<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>>();

//                 Ok::<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>, anyhow::Error>(pool_id_to_fields)
//             }
//         })
//     )
//     .await?;

//     let pool_id_to_fields = chunked_pool_id_to_fields
//         .into_iter()
//         .flatten()
//         .collect::<HashMap<ObjectID, BTreeMap<String, SuiMoveValue>>>();

//     Ok(pool_id_to_fields)
// }

