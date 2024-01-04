use async_trait::async_trait;
use anyhow::{anyhow, Context};

use ethnum::U256;

use futures::{future, TryStreamExt};
use fixed::types::U64F64;

use custom_sui_sdk::{
    SuiClient,
    apis::{
        QueryEventsRequest,
        GetDynamicFieldsRequest
    },
    transaction_builder::{TransactionBuilder, ProgrammableObjectArg},
    programmable_transaction_sui_json::ProgrammableTransactionArg
};

use page_turner::PageTurner;

use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::identifier::Identifier;
use move_core_types::account_address::AccountAddress;
use move_core_types::value::MoveValue;

use itertools::{Either, Itertools};

use serde_json::Value;
use std::str::FromStr;
use std::collections::{BTreeMap, HashMap};

use sui_sdk::json::SuiJsonValue;

use sui_sdk::types::{
    base_types::{ObjectID, ObjectIDParseError, ObjectType, SuiAddress, SequenceNumber}, 
    dynamic_field::DynamicFieldInfo,
    object::Object, 
    messages_checkpoint::CheckpointSequenceNumber, 
    programmable_transaction_builder::ProgrammableTransactionBuilder,
};

use sui_sdk::rpc_types::{Checkpoint};

use sui_sdk::rpc_types::{
    SuiObjectResponse, 
    EventFilter, 
    SuiEvent, 
    SuiParsedData, 
    SuiMoveStruct, 
    SuiMoveValue, 
    SuiObjectDataOptions, 
    SuiTypeTag
};

// use crate::{
//     // markets::{Exchange, Market}, ß
// };
use crate::fast_v3_pool;
use crate::sui_json_utils::move_value_to_json;
use crate::sui_sdk_utils::{self, sui_move_value, get_fields_from_object_response};

#[derive(Debug, Clone)]
pub struct Turbos {
    original_package_id: ObjectID,
    package_id: ObjectID,
    versioned_id: ObjectID,
}

impl Turbos {
    pub fn new(original_package_id: ObjectID, package_id: ObjectID, versioned_id: ObjectID) -> Self {
        Turbos {
            original_package_id,
            package_id,
            versioned_id,
            // event_struct_tag_to_pool_field,
        }
    }
}

impl Turbos {
    fn original_package_id(&self) -> &ObjectID {
        &self.original_package_id
    }

    fn package_id(&self) -> &ObjectID {
        &self.package_id
    }

    fn event_package_id(&self) -> &ObjectID {
        &self.original_package_id
    }

    async fn get_all_pool_ids(
        &self,
        sui_client: &SuiClient
    ) -> Result<Vec<ObjectID>, anyhow::Error> {
        let pool_created_events = sui_client
            .event_api()
            .pages(
                QueryEventsRequest {
                    query: EventFilter::MoveEventType(
                        StructTag::from_str(
                            &format!("{}::pool_factory::PoolCreatedEvent", self.original_package_id)
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
                if let Value::String(pool_id_value) = parsed_json.get("pool").context(format!("Failed to get pool for a TurbosMarket: {:#?}", parsed_json))? {
                    // println!("turbos: pool_id: {}", pool_id_value);
                    Ok(ObjectID::from_str(&format!("0x{}", pool_id_value))?)
                } else {
                    Err(anyhow!("Failed to match pattern."))
                }
            })
            .collect::<Result<Vec<ObjectID>, anyhow::Error>>()?;

        Ok(pool_ids)
    }

    pub async fn get_all_market_builders(
        &self,
        sui_client: &SuiClient
    ) -> Result<Vec<TurbosMarketBuilder>, anyhow::Error> {
        let pool_ids = self.get_all_pool_ids(sui_client).await?;

        Ok(
            pool_ids
                .into_iter()
                .map(|pool_id| {
                    let mut event_struct_tag_to_pool_field = HashMap::new();
                    event_struct_tag_to_pool_field.insert(
                        StructTag::from_str(
                            &format!("{}::pool::SwapEvent", self.original_package_id)
                        ).expect("Turbos: failed to create event struct tag"),
                        "pool".to_string()
                    );
                    TurbosMarketBuilder {
                        exchange: self.clone(),
                        pool_id,
                        event_struct_tag_to_pool_field
                    }
                })
                .collect()
        )
    }

    pub async fn get_checkpoint_pinned_market_with_id(
        &self,
        sui_client: &SuiClient,
        pool_id: ObjectID,
        event_struct_tag_to_pool_field: HashMap<StructTag, String>
    ) -> Result<(Checkpoint, TurbosMarket), anyhow::Error> {
        // println!("get_checkpoint_pinned_market_with_id: entry");

        let pool_object_response = sui_client
            .read_api()
            .get_object_with_options(
                pool_id.clone(),
                SuiObjectDataOptions::full_content()
            )
            .await?;

        // println!("get_checkpoint_pinned_market_with_id: got pool_object_response");

        self.get_checkpoint_pinned_market_with_object_response(
            sui_client, 
            pool_object_response,
            event_struct_tag_to_pool_field
        ).await
    }


    // This allows us to cut down on request significantly
    pub async fn get_checkpoint_pinned_market_with_object_response(
        &self,
        sui_client: &SuiClient,
        pool_object_response: SuiObjectResponse,
        event_struct_tag_to_pool_field: HashMap<StructTag, String>
    ) -> Result<(Checkpoint, TurbosMarket), anyhow::Error> {
        let pool_id = pool_object_response
            .data
            .as_ref()
            .context("SuiobjectResponse's data field is None")?
            .object_id;

        let (coin_x, coin_y, fee) = get_coin_pair_and_fee_from_object_response(&pool_object_response)?;

        // println!("get_checkpoint_pinned_market_with_id: got coin pair and fee");

        let (checkpoint, computing_pool) = self.checkpoint_pinned_computing_pool(
            sui_client,
            &pool_id
        ).await?;

        // let mut event_struct_tag_to_pool_field = HashMap::new();
        // event_struct_tag_to_pool_field.insert(
        //     StructTag::from_str(
        //         &format!("{}::pool::SwapEvent", self.original_package_id)
        //     ).expect("Turbos: failed to create event struct tag"),
        //     "pool".to_string()
        // );

        Ok(
            (
                checkpoint,
                TurbosMarket {
                    parent_exchange: self.clone(),  // reevaluate clone
                    coin_x,
                    coin_y,
                    fee,
                    pool_id,
                    computing_pool,
                    event_struct_tag_to_pool_field
                }
            )
        )
    }

    pub async fn checkpoint_pinned_computing_pool(
        &self,
        sui_client: &SuiClient,
        pool_id: &ObjectID
    ) -> Result<(Checkpoint, fast_v3_pool::Pool), anyhow::Error> {

        let (checkpoint, pool_object_response, tick_object_responses)= self
            .get_checkpoint_pinned_pool_and_tick_object_responses(sui_client, pool_id)
            .await?;

        // println!("    POOL {}:\n        NUM RECEIVED TICK OBJECT IDS: {}\n        NUM RECEIVED TICK OBJECT RESPONSES: {}", pool_id, tick_object_ids.len(), tick_object_responses.len());
        // Consider some checks to make sure we're gettin complete responses

        // We collect into a BTreeMap for the sort on insertion
        let tick_index_to_tick = tick_object_responses
            .into_iter()
            .map(|tick_object_response| {
                Self::tick_index_and_tick_from_object_response(
                    tick_object_response
                )
            })
            .filter_map(|x| x.transpose())
            .collect::<Result<BTreeMap<i32, fast_v3_pool::Tick>, anyhow::Error>>()?;
        
        let id = pool_object_response.data.as_ref().context("data field from object response is None")?.object_id;

        let fields = sui_sdk_utils::read_fields_from_object_response(&pool_object_response).context("missing fields")?;

        let sqrt_price = u128::from_str(
            &sui_move_value::get_string(&fields, "sqrt_price")?
        )?;

        let tick_spacing = sui_move_value::get_number(&fields, "tick_spacing")?;

        let fee = sui_move_value::get_number(&fields, "fee")? as u64;

        let unlocked = sui_move_value::get_bool(&fields, "unlocked")?;

        let liquidity = u128::from_str(
            &sui_move_value::get_string(&fields, "liquidity")?
        )?;

        let tick_current_index = sui_move_value::get_number(
            &sui_move_value::get_struct(&fields, "tick_current_index")?,
            "bits"
        )? as i32;

        Ok(
            (
                checkpoint,
                fast_v3_pool::Pool {
                    id,
                    sqrt_price,
                    tick_current_index,
                    tick_spacing,
                    fee,
                    unlocked,
                    liquidity,
                    ticks: tick_index_to_tick,
                }
            )
        )
    }

    pub async fn get_checkpoint_pinned_pool_and_tick_object_responses(
        &self, 
        sui_client: &SuiClient, 
        pool_id: &ObjectID
    ) -> Result<(Checkpoint, SuiObjectResponse, Vec<SuiObjectResponse>), anyhow::Error> {
        let pool_dynamic_field_infos = sui_client
            .read_api()
            .pages(
                GetDynamicFieldsRequest {
                    object_id: pool_id.clone(),
                    cursor: None,
                    limit: None,
                }
            )
            .items()
            .try_collect::<Vec<DynamicFieldInfo>>()
            .await?;

        // println!("Len pool dynamic fields: {}", pool_dynamic_field_infos.len());

        let tick_object_type = format!("{}::pool::Tick", self.original_package_id);

        let mut pool_and_tick_object_ids = pool_dynamic_field_infos
            .into_iter()
            .filter(|dynamic_field_info| {
                tick_object_type == dynamic_field_info.object_type
            })
            .map(|tick_dynamic_field_info| {
                tick_dynamic_field_info.object_id
            })
            .collect::<Vec<ObjectID>>();

        pool_and_tick_object_ids.push(pool_id.clone());

        println!("pre get_checkpoint_pinned_object_responses");

        let (checkpoint, pool_and_tick_object_responses) = sui_sdk_utils::get_checkpoint_pinned_object_responses(
            sui_client,
            pool_and_tick_object_ids
        ).await?;

        println!("post get_checkpoint_pinned_object_responses");

        let (mut pool_object_responses, tick_object_responses): (Vec<_>, Vec<_>) = pool_and_tick_object_responses
            .into_iter()
            .into_iter()
            .partition_map(|response| {
                if response.data.as_ref().expect("SuiObjectResponse's data field is None").object_id == *pool_id {
                    Either::Left(response)
                } else {
                    Either::Right(response)
                }
            });

        let pool_object_response = pool_object_responses
            .pop()
            .context(
                format!("Missing a SuiObjectResponse for pool {}", pool_id)
            )?;

        Ok((checkpoint, pool_object_response, tick_object_responses))
    }

    fn tick_index_and_tick_from_object_response(
        tick_object_response: SuiObjectResponse
    ) -> Result<Option<(i32, fast_v3_pool::Tick)>, anyhow::Error> {
        let fields = sui_sdk_utils::read_fields_from_object_response(&tick_object_response).context("Missing fields.")?;

        let tick_index = sui_move_value::get_number(
            &sui_move_value::get_struct(
                &fields, 
                "name")?, 
            "bits"
        )? as i32;

        let sqrt_price = fast_v3_pool::tick_math::sqrt_price_from_tick_index(tick_index);

        let tick_fields = sui_move_value::get_struct(&fields, "value").context("turbos struct")?;

        let initialized = sui_move_value::get_bool(&tick_fields, "initialized")?;

        if !initialized {
            return Ok(None)
        }

        let liquidity_gross = u128::from_str(
            &sui_move_value::get_string(&tick_fields, "liquidity_gross")?
        )?;

        let liquidity_net = u128::from_str(
            &sui_move_value::get_string(
                &sui_move_value::get_struct(
                    &tick_fields, 
                    "liquidity_net"
                )?, 
                "bits"
            )?
        )? as i128;


        let tick = fast_v3_pool::Tick {
            index: tick_index,
            sqrt_price,
            liquidity_gross,
            liquidity_net,
        };

        Ok(Some((tick_index, tick)))
    }
}

// #[async_trait]
// impl Exchange for Turbos {
//     fn package_id(&self) -> &ObjectID {
//        self.package_id()
//     }

//     fn event_package_id(&self) -> &ObjectID {
//         self.event_package_id()
//     }

//     fn event_filters(&self) -> Vec<EventFilter> {
//         self.event_filters()
//     }

//     fn event_struct_tag_to_pool_field(&self) -> &HashMap<StructTag, String> {
//         self.event_struct_tag_to_pool_field()
//     }

//     async fn get_all_markets(&mut self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error> {
//         self.get_all_markets_(sui_client).await
//     }

//     async fn get_pool_id_to_object_response(&self, sui_client: &SuiClient, markets: &[Box<dyn Market>]) -> Result<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error> {
//         self.get_pool_id_to_object_response(sui_client, markets).await
//     }
// }

// impl TurbosMarketBuilder

// This gives us the information needed to build. But allows us to delay building
#[derive(Clone, Debug)]
pub struct TurbosMarketBuilder {
    exchange: Turbos,
    // we should either have the pool id or the object response
    // let choose to build off of minimal information
    pool_id: ObjectID,
    event_struct_tag_to_pool_field: HashMap<StructTag, String>
}

impl TurbosMarketBuilder {

    // pub fn event_filters(&self) -> Vec<EventFilter> {
    //     self
    //         .event_struct_tag_to_pool_field
    //         .keys()
    //         .cloned()
    //         .map(|event_struct_tag| {
    //             EventFilter::MoveEventType(
    //                 event_struct_tag
    //             )
    //         })
    //         .collect::<Vec<_>>()
    // }

    // pub fn event_struct_tags(&self) -> Vec<StructTag> {
    //     self
    //         .event_struct_tag_to_pool_field
    //         .keys()
    //         .cloned()
    //         .collect::<Vec<_>>()
    // }

    pub fn event_struct_tag_to_pool_field(&self) -> &HashMap<StructTag, String> {
        &self.event_struct_tag_to_pool_field
    }

    // This will consume the builder
    // Perhaps we should make this reference based for redundancy
    // in cas we have to rebuild
    pub async fn build_checkpoint_pinned_market(
        self,
        sui_client: &SuiClient
    ) -> Result<(Checkpoint, TurbosMarket), anyhow::Error> {
        self
            .exchange
            .get_checkpoint_pinned_market_with_id(
                sui_client, 
                self.pool_id,
                self.event_struct_tag_to_pool_field
            )
            .await
    }
}

#[derive(Debug, Clone)]
pub struct TurbosMarket {
    pub parent_exchange: Turbos,
    pub coin_x: TypeTag,
    pub coin_y: TypeTag,
    pub fee: TypeTag,
    pub pool_id: ObjectID,
    pub computing_pool: fast_v3_pool::Pool,
    pub event_struct_tag_to_pool_field: HashMap<StructTag, String>
    // pub version: SequenceNumber
}

const SUI_STD_LIB_PACKAGE_ID: &str = "0x0000000000000000000000000000000000000000000000000000000000000002";
const CLOCK_OBJECT_ID: &str = "0x0000000000000000000000000000000000000000000000000000000000000006";

impl TurbosMarket {

    fn coin_x(&self) -> &TypeTag {
        &self.coin_x
    }

    fn coin_y(&self) -> &TypeTag {
        &self.coin_y
    }

    fn event_struct_tag_to_pool_field(&self) -> &HashMap<StructTag, String> {
        &self.event_struct_tag_to_pool_field
    }

    fn contains_event_type(
        &self,
        sui_event_type: &StructTag
    ) -> bool {
        self.event_struct_tag_to_pool_field.contains_key(sui_event_type)
    }

    // We chould invite solutions that encourage further reasonaable optimization
    // Like filtering before applying
    // Rather than blindly trying to apply a change
    pub fn try_parse_pool_id_from_event(
        &self,
        sui_event: &SuiEvent
    ) -> Result<Option<ObjectID>, anyhow::Error> {
        if let Some(pool_field_str) = self.event_struct_tag_to_pool_field.get(&sui_event.type_) {
            if let Some(value) = sui_event.parsed_json.get(pool_field_str) {
                if let Value::String(pool_id_str) = value {
                    Ok(Some(ObjectID::from_str(&pool_id_str)?))
                } else {
                    Err(anyhow!("parsed_json field should match Value::String variant"))
                }
            } else {
                Err(anyhow!("Event has no such field '{}'", pool_field_str))
            }
        } else {
            Ok(None)
        }
    }

    pub fn update_with_event(&mut self, event: &SuiEvent) -> Result<(), anyhow::Error> {
        let type_ = &event.type_;
        let event_parsed_json = &event.parsed_json;

        // Amortize this so we only allocate these once. Cant be computed at compile time.
        let swap_event_type = StructTag::from_str(
                &format!("{}::pool::SwapEvent", &self.parent_exchange.original_package_id)
            ).context("Turbos: failed to create event struct tag")?;

        let add_liq_event_type = StructTag::from_str(
                &format!("{}::pool::MintEvent", &self.parent_exchange.original_package_id)
            ).context("Turbos: failed to create event struct tag")?;

        let remove_liq_event_type = StructTag::from_str(
                &format!("{}::pool::BurnEvent", &self.parent_exchange.original_package_id)
            ).context("Turbos: failed to create event struct tag")?;

        let update_status_event_type = StructTag::from_str(
                &format!("{}::pool::TogglePoolStatusEvent", &self.parent_exchange.original_package_id)
            ).context("Turbos: failed to create event struct tag")?;

        match type_ {
            swap_event_type => {
                let amount_a = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("amount_a").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent amount_a is not Value::String."))
                    }
                )?;
                let amount_b = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("amount_b").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent amount_b is not Value::String."))
                    }
                )?;
                let fee_amount = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("fee_amount").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent fee_amount is not Value::String."))
                    }
                )?;
                let a_to_b = *if let serde_json::Value::Bool(bool_inner) = event_parsed_json.get("a_to_b").context("")? {
                    bool_inner
                } else {
                    return Err(anyhow!("SwapEvent a_to_b is not Value::Bool."))
                };

                let amount_specified = if a_to_b {
                    amount_a + fee_amount
                } else {
                    amount_b + fee_amount
                };

                let sqrt_price_limit = if a_to_b {
                    fast_v3_pool::tick_math::MIN_SQRT_PRICE_X64 + 1
                } else {
                    fast_v3_pool::tick_math::MAX_SQRT_PRICE_X64 - 1
                };

                self.computing_pool.apply_swap(
                    a_to_b,
                    amount_specified, 
                    true, 
                    sqrt_price_limit
                );

            },
            add_liq_event_type => {
                let tick_lower_index = u32::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("tick_lower_index").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent tick_lower_index is not Value::String."))
                    }
                )? as i32;
                let tick_upper_index = u32::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("tick_upper_index").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent tick_upper_index is not Value::String."))
                    }
                )? as i32;
                let liquidity_delta = u128::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("liquidity_delta").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent liquidity_delta is not Value::String."))
                    }
                )?;

                self.computing_pool.apply_add_liquidity(
                    tick_lower_index, 
                    tick_upper_index, 
                    liquidity_delta
                );
            },
            remove_liq_event_type => {
                let tick_lower_index = u32::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("tick_lower_index").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent tick_lower_index is not Value::String."))
                    }
                )? as i32;
                let tick_upper_index = u32::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("tick_upper_index").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent tick_upper_index is not Value::String."))
                    }
                )? as i32;
                let liquidity_delta = u128::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("liquidity_delta").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent liquidity_delta is not Value::String."))
                    }
                )?;

                self.computing_pool.apply_remove_liquidity(
                    tick_lower_index, 
                    tick_upper_index, 
                    liquidity_delta
                );
            },
            update_status_event_type => {
                let status = *if let serde_json::Value::Bool(bool_inner) = event_parsed_json.get("status").context("")? {
                    bool_inner
                } else {
                    return Err(anyhow!("SwapEvent status is not Value::Bool."))
                };

                self.computing_pool.apply_update_unlocked(
                    status
                );
            },
            _ => {
                // do nothing
                println!("update_with_event: did nothing.");
            }
        }

        // self.version = 

        Ok(())

    }

    pub fn pool_id(&self) -> &ObjectID {
        &self.pool_id
    }

    fn package_id(&self) -> &ObjectID {
        &self.parent_exchange.package_id
    }

    fn compute_swap_x_to_y(&self, amount_specified: u128) -> (u128, u128) {
        
        let swap_state = self.computing_pool.compute_swap_result(
            true, 
            amount_specified as u64, 
            true, 
            fast_v3_pool::tick_math::MIN_SQRT_PRICE_X64 + 1,
        );

        (swap_state.amount_a as u128, swap_state.amount_b as u128)
    }

    fn compute_swap_y_to_x(&self, amount_specified: u128) -> (u128, u128) {
        
        let swap_state = self.computing_pool.compute_swap_result(
            false, 
            amount_specified as u64, 
            true, 
            fast_v3_pool::tick_math::MAX_SQRT_PRICE_X64 - 1,
        );

        (swap_state.amount_a as u128, swap_state.amount_b as u128)
    }

    fn viable(&self) -> bool {
        if self.computing_pool.liquidity > 0  && self.computing_pool.unlocked && self.computing_pool.liquidity_sanity_check() {
            true
        } else {
            false
        }
    }

    async fn add_swap_to_programmable_transaction(
        &self,
        transaction_builder: &TransactionBuilder,
        pt_builder: &mut ProgrammableTransactionBuilder,
        orig_coins: Option<Vec<ObjectID>>, // the actual coin object in (that you own and has money)
        x_to_y: bool,
        amount_in: u128,
        amount_out: u128,
        recipient: SuiAddress
    ) -> Result<(), anyhow::Error> {

        // Arg8: &Clock
        let clock_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::from_object_id(
                ObjectID::from_str(CLOCK_OBJECT_ID)?
            )
        );

        let clock_timestamp_arg = ProgrammableTransactionArg::Argument(
            transaction_builder.programmable_move_call(
                pt_builder,
                ObjectID::from_str(SUI_STD_LIB_PACKAGE_ID)?,
                "clock",
                "timestamp_ms",
                vec![],
                vec![clock_arg.clone()]
            ).await?
        );

        let time_til_expiry = 1000u64;

        let clock_delta_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::U64(time_til_expiry)
                )
                .context("failed to convert MoveValue for amount_specified_is_input to JSON")?
            )?
        );

        // Arg7: u64
        let deadline_arg = ProgrammableTransactionArg::Argument(
            transaction_builder.programmable_move_call(
                pt_builder,
                self.parent_exchange.package_id.clone(),
                "math_u64",
                "wrapping_add",
                vec![],
                vec![clock_timestamp_arg, clock_delta_arg]
            ).await?   
        );

        // swap_a_b and swap_b_c arguments
        // Arg0: &mut Pool<Ty0, Ty1, Ty2>
        let pool_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::from_object_id(self.pool_id.clone())
        );

        // Arg1: vector<Coin<Ty0 or Ty1>>
        let orig_coins_args_vec = if let Some(oc) = orig_coins {
            oc
                .into_iter()
                .map(|orig_coin| {
                    ProgrammableObjectArg::ObjectID(orig_coin)
                })
                .collect::<Vec<ProgrammableObjectArg>>()
        } else {
            vec![
                ProgrammableObjectArg::Argument(
                    transaction_builder.programmable_split_gas_coin(pt_builder, amount_in as u64).await
                )
            ]
        };

        let orig_coins_arg = ProgrammableTransactionArg::Argument(
            transaction_builder
                .programmable_make_object_vec(
                    pt_builder,
                    orig_coins_args_vec
                ).await?
        );

        // Arg2: u64
        let amount_specified_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::U64(amount_in as u64)
                )
                .context("failed to convert MoveValue for amount_specified to JSON")?
            )?
        );

        // Arg3: u64
        // The amount out we're expecting 
        let amount_threshold_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::U64(amount_out as u64)
                )
                .context("failed to convert MoveValue for amount_specified to JSON")?
            )?
        );

        // Arg4: u128
        let sqrt_price_limit_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::U128(
                        if x_to_y {
                            fast_v3_pool::tick_math::MIN_SQRT_PRICE_X64 + 1
                        } else {
                            fast_v3_pool::tick_math::MAX_SQRT_PRICE_X64 - 1
                        }
                    )
                )
                .context("failed to convert MoveValue for sqrt_price_limit to JSON")?
            )?
        );
        
        // Arg5: bool
        let amount_specified_is_input_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::Bool(true)
                )
                .context("failed to convert MoveValue for amount_specified_is_input to JSON")?
            )?
        );

        // Arg6: address
        let recipient_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::Address(
                        AccountAddress::from(
                            recipient
                        )
                    )
                ).context("failed to convert MoveValue for recipient to JSON")?
            )?
        );

        // Arg9: &Versioned
        let versioned_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::from_object_id(
                self.parent_exchange.versioned_id.clone()
            )
        );

        let call_args = vec![
            pool_arg,               // Arg0
            orig_coins_arg,     // Arg1
            amount_specified_arg,   // Arg2
            amount_threshold_arg,   // Arg3
            sqrt_price_limit_arg,   // Arg4
            amount_specified_is_input_arg,  // Arg5
            recipient_arg,          // Arg6
            deadline_arg,           // Arg7
            clock_arg,              // Arg8
            versioned_arg         // Arg9
        ];

        let type_args = vec![
            SuiTypeTag::new(format!("{}", self.coin_x)), 
            SuiTypeTag::new(format!("{}", self.coin_y)),
            SuiTypeTag::new(format!("{}", self.fee)),
        ];

        let function = if x_to_y {
            "swap_a_b"
        } else {
            "swap_b_a"
        };

        transaction_builder.programmable_move_call(
            pt_builder,
            self.parent_exchange.package_id.clone(),
            "swap_router",
            function,
            type_args,
            call_args
        ).await?;

        Ok(())
    }

}

// #[async_trait]
// impl Market for TurbosMarket {
//     fn coin_x(&self) -> &TypeTag {
//         self.coin_x()
//     }

//     fn coin_y(&self) -> &TypeTag {
//         self.coin_y()
//     }

//     fn coin_x_price(&self) -> Option<U64F64> {
//         self.coin_x_price()
//     }

//     fn coin_y_price(&self) -> Option<U64F64> {
//         self.coin_y_price()
//     }

//     async fn update_with_object_response(&mut self, sui_client: &SuiClient, object_response: &SuiObjectResponse) -> Result<(), anyhow::Error> {
//         self.update_with_object_response(sui_client, object_response).await
//     }

//     fn pool_id(&self) -> &ObjectID {
//         self.pool_id()
//     }

//     fn package_id(&self) -> &ObjectID {
//         self.package_id()
//     }

//     // fn router_id(&self) -> &ObjectID {
//     //     self.router_id()
//     // }

//     // fn compute_swap_x_to_y_mut(&mut self, amount_specified: u128) -> (u128, u128) {
//     //     self.compute_swap_x_to_y_mut(amount_specified)
//     // }

//     // fn compute_swap_y_to_x_mut(&mut self, amount_specified: u128) -> (u128, u128) {
//     //     self.compute_swap_y_to_x_mut(amount_specified)
//     // }

//     fn compute_swap_x_to_y(&self, amount_specified: u128) -> (u128, u128) {
//         self.compute_swap_x_to_y(amount_specified)
//     }

//     fn compute_swap_y_to_x(&self, amount_specified: u128) -> (u128, u128) {
//         self.compute_swap_y_to_x(amount_specified)
//     }

//     fn viable(&self) -> bool {
//         self.viable()
//     }

//     async fn add_swap_to_programmable_transaction(
//         &self,
//         transaction_builder: &TransactionBuilder,
//         pt_builder: &mut ProgrammableTransactionBuilder,
//         orig_coins: Option<Vec<ObjectID>>, // the actual coin object in (that you own and has money)
//         x_to_y: bool,
//         amount_in: u128,
//         amount_out: u128,
//         recipient: SuiAddress
//     ) -> Result<(), anyhow::Error> {
//         self.add_swap_to_programmable_transaction(
//             transaction_builder,
//             pt_builder,
//             orig_coins,
//             x_to_y,
//             amount_in,
//             amount_out,
//             recipient
//         )
//         .await
//     }

// }

fn get_coin_pair_and_fee_from_object_response (
    object_response: &SuiObjectResponse
) -> Result<(TypeTag, TypeTag, TypeTag), anyhow::Error> {
    // println!("{:#?}", response);
    if let Some(data) = object_response.clone().data {
        if let Some(type_) = data.type_ {
            if let ObjectType::Struct(move_object_type) = type_ {
                let type_params = move_object_type.type_params();

                // Ty0 is the first coin
                // Ty1 is the second coin
                // Ty2 is a fee object

                // println!("{:#?}", type_params);
                // panic!();

                Ok(
                    (
                        type_params.get(0).context("Missing coin_x")?.clone(),
                        type_params.get(1).context("Missing coin_y")?.clone(),
                        type_params.get(2).context("Missing fee")?.clone()
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