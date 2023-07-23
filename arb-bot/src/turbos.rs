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
    },
    transaction_builder::TransactionBuilder,
    programmable_transaction_sui_json::ProgrammableTransactionArg
};

use sui_sdk::types::{base_types::{ObjectID, ObjectIDParseError, ObjectType, SuiAddress}, object::Object};
use sui_sdk::types::dynamic_field::DynamicFieldInfo;
use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_sdk::types::transaction::Argument;
use sui_sdk::rpc_types::{SuiObjectResponse, EventFilter, SuiEvent, SuiParsedData, SuiMoveStruct, SuiMoveValue, SuiObjectDataOptions, SuiTypeTag};
 
use sui_sdk::json::SuiJsonValue;

use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::identifier::Identifier;
use move_core_types::account_address::AccountAddress;
use move_core_types::value::MoveValue;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use crate::{markets::{Exchange, Market}, sui_sdk_utils::get_fields_from_object_response};
use crate::sui_sdk_utils::{self, sui_move_value};
use crate::turbos_pool;
use crate::sui_json_utils::move_value_to_json;

#[derive(Debug, Clone)]
pub struct Turbos {
    original_package_id: ObjectID,
    package_id: ObjectID,
    versioned_id: ObjectID
}

impl Turbos {
    pub fn new(original_package_id: ObjectID, package_id: ObjectID, versioned_id: ObjectID) -> Self {
        Turbos {
            original_package_id,
            package_id,
            versioned_id,
        }
    }
}

// impl FromStr for Turbos {
//     type Err = anyhow::Error;

//     fn from_str(package_id_str: &str, router_id_str: &str) -> Result<Self, anyhow::Error> {
//         Ok(
//             Turbos {
//                 package_id: ObjectID::from_str(package_id_str).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
//                 router_id: ObjectID::from_str(router_id_str).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
//             }
//         )
//     }
// }

impl Turbos {
    fn original_package_id(&self) -> &ObjectID {
        &self.original_package_id
    }

    fn package_id(&self) -> &ObjectID {
        &self.package_id
    }

    // fn router_id(&self) -> &ObjectID {
    //     &self.router_id
    // }

    async fn get_all_markets(&self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error> {
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
                if let Value::String(pool_id_value) = parsed_json.get("pool").context("Failed to get pool_id for a CetusMarket")? {
                    println!("turbos: pool_id: {}", pool_id_value);
                    Ok(ObjectID::from_str(&format!("0x{}", pool_id_value))?)
                } else {
                    Err(anyhow!("Failed to match pattern."))
                }
            })
            .collect::<Result<Vec<ObjectID>, anyhow::Error>>()?;

        let pool_id_to_object_response = sui_sdk_utils::get_object_id_to_object_response(sui_client, &pool_ids).await?;

        let markets = pool_id_to_object_response
            .into_iter()
            .map(|(pool_id, object_response)| {
                // println!("{:#?}", object_response);
                // panic!();

                let fields = sui_sdk_utils::read_fields_from_object_response(&object_response).context(format!("Missing fields for pool {}.", pool_id))?;

                let (coin_x, coin_y, fee) = get_coin_pair_and_fee_from_object_response(&object_response)?;

                let coin_x_sqrt_price = U64F64::from_bits(
                    u128::from_str(
                        & if let SuiMoveValue::String(str_value) = fields
                            .read_dynamic_field_value("sqrt_price")
                            .context(format!("Missing field sqrt_price for coin {}", coin_x))? {
                                str_value
                            } else {
                                return Err(anyhow!("sqrt_price field does not match SuiMoveValue::String value."));
                            }
                    )?
                );
        
                let coin_y_sqrt_price = U64F64::from_num(1) / coin_x_sqrt_price;

                Ok(
                    Box::new(
                        TurbosMarket {
                            parent_exchange: self.clone(),  // reevaluate clone
                            coin_x,
                            coin_y,
                            fee,
                            pool_id,
                            coin_x_sqrt_price: Some(coin_x_sqrt_price),
                            coin_y_sqrt_price: Some(coin_y_sqrt_price),
                            computing_pool: None    // We'll grab this later so we don't have to deal with async stuff
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

        sui_sdk_utils::get_object_id_to_object_response(sui_client, &pool_ids).await
    }

    // For all intents and purposes
    // "pool" is an object that can perform the computations
    pub async fn computing_pool_from_object_response(&self, sui_client: &SuiClient, response: &SuiObjectResponse) -> Result<turbos_pool::Pool, anyhow::Error> {

        let fields = sui_sdk_utils::read_fields_from_object_response(response).context("missing fields")?;

        let protocol_fees_a = u64::from_str(
            &sui_move_value::get_string(&fields, "protocol_fees_a")?
        )?;

        let protocol_fees_b = u64::from_str(
            &sui_move_value::get_string(&fields, "protocol_fees_b")?
        )?;

        let sqrt_price = u128::from_str(
            &sui_move_value::get_string(&fields, "sqrt_price")?
        )?;

        let tick_spacing = sui_move_value::get_number(&fields, "tick_spacing")?;

        let max_liquidity_per_tick = u128::from_str(
            &sui_move_value::get_string(&fields, "max_liquidity_per_tick")?
        )?;

        let fee = sui_move_value::get_number(&fields, "fee")?;

        let fee_protocol = sui_move_value::get_number(&fields, "fee_protocol")?;

        let unlocked = sui_move_value::get_bool(&fields, "unlocked")?;

        let fee_growth_global_a = u128::from_str(
            &sui_move_value::get_string(&fields, "fee_growth_global_a")?
        )?;

        let fee_growth_global_b = u128::from_str(
            &sui_move_value::get_string(&fields, "fee_growth_global_b")?
        )?;

        let liquidity = u128::from_str(
            &sui_move_value::get_string(&fields, "liquidity")?
        )?;

        let tick_current_index = sui_move_value::get_number(
            &sui_move_value::get_struct(&fields, "tick_current_index")?,
            "bits"
        )? as i32;

        // let init_tick_start = Instant::now();
        let ticks = self.get_ticks(sui_client, &response.object_id()?).await?;
        // let init_tick_duration = init_tick_start.elapsed();
        // println!("get_ticks(): {:?}", init_tick_duration);

        let tick_map_id = sui_move_value::get_uid(
            &sui_move_value::get_struct(&fields, "tick_map")?,
            "id"
        )?;

        // let tick_map_start = Instant::now();
        let tick_map = Self::get_tick_map(sui_client, &tick_map_id).await?;
        // let tick_map_duration = tick_map_start.elapsed();
        // println!("get_tick_map(): {:?}", tick_map_duration);

        // println!("pool end!");

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
                ticks, // new
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
        // println!("    TICK_MAP {}\n        NUM RECEIVED WORD OBJECT IDS: {}\n        NUM RECEIVED WORD OBJECT RESPONSES: {}", tick_map_id, word_ids.len(), word_object_responses.len());

        let word_pos_to_word = word_object_responses
            .into_iter()
            .map(|word_object_response| {
                let fields = sui_sdk_utils::read_fields_from_object_response(&word_object_response).context("Mising fields from word_object_response.")?;
                let word_pos = sui_move_value::get_number(
                    &sui_move_value::get_struct(&fields, "name")?,
                    "bits"
                )? as i32;

                // Moving the casts/conversions to outside the if let makes this more modular
                let word = U256::from_str(
                    &sui_move_value::get_string(&fields, "value")?
                )?;

                Ok((word_pos, word))
            })
            .collect::<Result<BTreeMap<i32, U256>, anyhow::Error>>()?;

        Ok(word_pos_to_word)
    }

    pub async fn get_ticks(&self, sui_client: &SuiClient, pool_id: &ObjectID) -> Result<BTreeMap<i32, turbos_pool::Tick>, anyhow::Error>{
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

        let tick_object_ids = pool_dynamic_field_infos
            .into_iter()
            .filter(|dynamic_field_info| {
                tick_object_type == dynamic_field_info.object_type
            })
            .map(|tick_dynamic_field_info| {
                tick_dynamic_field_info.object_id
            })
            .collect::<Vec<ObjectID>>();

        let tick_object_responses = sui_sdk_utils::get_object_responses(sui_client, &tick_object_ids).await?;

        // println!("    POOL {}:\n        NUM RECEIVED TICK OBJECT IDS: {}\n        NUM RECEIVED TICK OBJECT RESPONSES: {}", pool_id, tick_object_ids.len(), tick_object_responses.len());
        // Consider some checks to make sure we're gettin complete responses

        let tick_index_to_tick = tick_object_responses
            .into_iter()
            .map(|tick_object_response| {
                let fields = sui_sdk_utils::read_fields_from_object_response(&tick_object_response).context("Missing fields.")?;

                let tick_index = sui_move_value::get_number(
                    &sui_move_value::get_struct(
                        &fields, 
                        "name")?, 
                    "bits"
                )? as i32;

                let tick_fields = sui_move_value::get_struct(&fields, "value")?;

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

                // Moving the casts/conversions to outside the if let makes this more modular
                let fee_growth_outside_a = u128::from_str(
                    &sui_move_value::get_string(&tick_fields,"fee_growth_outside_a")?
                )?;
                
                // Moving the casts/conversions to outside the if let would make this more modular
                let fee_growth_outside_b = u128::from_str(
                    &sui_move_value::get_string(&tick_fields,"fee_growth_outside_b")?
                )?;
                
                let initialized = sui_move_value::get_bool(&tick_fields, "initialized")?;

                let tick = turbos_pool::Tick {
                    liquidity_gross,
                    liquidity_net,
                    fee_growth_outside_a,
                    fee_growth_outside_b,
                    initialized,
                };

                Ok((tick_index, tick))
            })
            .collect::<Result<BTreeMap<i32, turbos_pool::Tick>, anyhow::Error>>()?;

        Ok(tick_index_to_tick)
    }

}

#[async_trait]
impl Exchange for Turbos {
    fn package_id(&self) -> &ObjectID {
       self.package_id()
    }

    // fn router_id(&self) -> &ObjectID {
    //     self.router_id()
    //  }

    async fn get_all_markets(&self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error> {
        self.get_all_markets(sui_client).await
    }

    async fn get_pool_id_to_object_response(&self, sui_client: &SuiClient, markets: &[Box<dyn Market>]) -> Result<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error> {
        self.get_pool_id_to_object_response(sui_client, markets).await
    }
}

#[derive(Debug, Clone)]
struct TurbosMarket {
    parent_exchange: Turbos,
    coin_x: TypeTag,
    coin_y: TypeTag,
    fee: TypeTag,
    pool_id: ObjectID,
    coin_x_sqrt_price: Option<U64F64>, // In terms of y. x / y
    coin_y_sqrt_price: Option<U64F64>, // In terms of x. y / x
    computing_pool: Option<turbos_pool::Pool>
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

    // rename to "..pool_object_response"
    // recall that we 
    async fn update_with_object_response(&mut self, sui_client: &SuiClient, object_response: &SuiObjectResponse) -> Result<(), anyhow::Error> {
        let fields = sui_sdk_utils::read_fields_from_object_response(object_response).context("Missing fields for object_response.")?;
        let coin_x_sqrt_price = U64F64::from_bits(
            u128::from_str(
                &sui_move_value::get_string(&fields, "sqrt_price")?
            )?
        );

        let coin_y_sqrt_price = U64F64::from_num(1) / coin_x_sqrt_price;
        
        self.coin_x_sqrt_price = Some(coin_x_sqrt_price);
        self.coin_y_sqrt_price = Some(coin_y_sqrt_price);

        // println!("sq then mult: {}", U64F64::from_num(1) * (coin_x_sqrt_price * coin_x_sqrt_price) * (coin_y_sqrt_price * coin_y_sqrt_price));
        // println!("mult then sq: {}", U64F64::from_num(1) * (coin_x_sqrt_price * coin_y_sqrt_price) * (coin_x_sqrt_price * coin_y_sqrt_price));

        // let start = Instant::now();
        self.computing_pool = Some(self.parent_exchange.computing_pool_from_object_response(sui_client, object_response).await?);
        // let duration = start.elapsed();
        // println!("computing_pool_from_response(): {:?}", duration);

        Ok(())
    }

    fn pool_id(&self) -> &ObjectID {
        &self.pool_id
    }

    fn package_id(&self) -> &ObjectID {
        &self.parent_exchange.package_id
    }

    // fn router_id(&self) -> &ObjectID {
    //     &self.parent_exchange.router_id
    // }

    fn compute_swap_x_to_y_mut(&mut self, amount_specified: u128) -> (u128, u128) {
        
        let swap_state = turbos_pool::compute_swap_result(
            self.computing_pool.as_mut().unwrap(), 
            true, 
            amount_specified, 
            true, 
            turbos_pool::math_tick::MIN_SQRT_PRICE_X64 + 1,
            false
        );

        (swap_state.amount_a, swap_state.amount_b)
    }

    fn compute_swap_y_to_x_mut(&mut self, amount_specified: u128) -> (u128, u128) {
        
        let swap_state = turbos_pool::compute_swap_result(
            self.computing_pool.as_mut().unwrap(), 
            false, 
            amount_specified, 
            true, 
            turbos_pool::math_tick::MAX_SQRT_PRICE_X64 - 1,
            false
        );

        (swap_state.amount_a, swap_state.amount_b)
    }

    fn compute_swap_x_to_y(&self, amount_specified: u128) -> (u128, u128) {
        
        let swap_state = turbos_pool::compute_swap_result(
            &mut self.computing_pool.clone().unwrap(), 
            true, 
            amount_specified, 
            true, 
            turbos_pool::math_tick::MIN_SQRT_PRICE_X64 + 1,
            true
        );

        (swap_state.amount_a, swap_state.amount_b)
    }

    fn compute_swap_y_to_x(&self, amount_specified: u128) -> (u128, u128) {
        
        let swap_state = turbos_pool::compute_swap_result(
            &mut self.computing_pool.clone().unwrap(), 
            false, 
            amount_specified, 
            true, 
            turbos_pool::math_tick::MAX_SQRT_PRICE_X64 - 1,
            true
        );

        (swap_state.amount_a, swap_state.amount_b)
    }

    fn viable(&self) -> bool {
        if let Some(cp) = &self.computing_pool {
            // println!("liquidity: {}", cp.liquidity);
            if cp.liquidity > 0 {
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    async fn add_swap_to_programmable_transaction(
        &self,
        transaction_builder: &TransactionBuilder,
        pt_builder: &mut ProgrammableTransactionBuilder,
        orig_coins: Vec<ObjectID>, // the actual coin object in (that you own and has money)
        x_to_y: bool,
        amount_in: u128,
        amount_out: u128,
        recipient: SuiAddress
    ) -> Result<(), anyhow::Error> {

        // Arg8: &Clock
        let clock = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::from_object_id(
                ObjectID::from_str(CLOCK_OBJECT_ID)?
            )
        );

        let clock_timestamp = transaction_builder.programmable_move_call(
            pt_builder,
            ObjectID::from_str(SUI_STD_LIB_PACKAGE_ID)?,
            "clock",
            "timestamp_ms",
            vec![],
            vec![clock.clone()]
        ).await?;

        let time_til_expiry = 1000u64;

        let clock_delta = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::U64(time_til_expiry)
                )
                .context("failed to convert MoveValue for amount_specified_is_input to JSON")?
            )?
        );

        // Arg7: u64
        let deadline = transaction_builder.programmable_move_call(
            pt_builder,
            self.parent_exchange.package_id.clone(),
            "math_u64",
            "wrapping_add",
            vec![],
            vec![clock_timestamp, clock_delta]
        ).await?;

        // swap_a_b and swap_b_c arguments
        // Arg0: &mut Pool<Ty0, Ty1, Ty2>
        let pool = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::from_object_id(self.pool_id.clone())
        );

        // Arg1: vector<Coin<Ty0 or Ty1>>
        let orig_coins_arg = transaction_builder
            .programmable_make_object_vec(
                pt_builder,
                orig_coins
            ).await?;

        // Arg2: u64
        let amount_specified = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::U64(amount_in as u64)
                )
                .context("failed to convert MoveValue for amount_specified to JSON")?
            )?
        );

        // Arg3: u64
        // The amount out we're expecting 
        let amount_threshold = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::U64(amount_out as u64)
                )
                .context("failed to convert MoveValue for amount_specified to JSON")?
            )?
        );

        // Arg4: u128
        let sqrt_price_limit = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::U128(
                        if x_to_y {
                            turbos_pool::math_tick::MIN_SQRT_PRICE_X64 + 1
                        } else {
                            turbos_pool::math_tick::MAX_SQRT_PRICE_X64 - 1
                        }
                    )
                )
                .context("failed to convert MoveValue for sqrt_price_limit to JSON")?
            )?
        );
        
        // Arg5: bool
        let amount_specified_is_input = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::Bool(true)
                )
                .context("failed to convert MoveValue for amount_specified_is_input to JSON")?
            )?
        );

        // Arg6: address
        let recipient = ProgrammableTransactionArg::SuiJsonValue(
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
        let versioned = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::from_object_id(
                self.parent_exchange.versioned_id.clone()
            )
        );

        let call_args = vec![
            pool,               // Arg0
            orig_coins_arg,     // Arg1
            amount_specified,   // Arg2
            amount_threshold,   // Arg3
            sqrt_price_limit,   // Arg4
            amount_specified_is_input,  // Arg5
            recipient,          // Arg6
            deadline,           // Arg7
            clock,              // Arg8
            versioned           // Arg9
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

#[async_trait]
impl Market for TurbosMarket {
    fn coin_x(&self) -> &TypeTag {
        self.coin_x()
    }

    fn coin_y(&self) -> &TypeTag {
        self.coin_y()
    }

    fn coin_x_price(&self) -> Option<U64F64> {
        self.coin_x_price()
    }

    fn coin_y_price(&self) -> Option<U64F64> {
        self.coin_y_price()
    }

    async fn update_with_object_response(&mut self, sui_client: &SuiClient, object_response: &SuiObjectResponse) -> Result<(), anyhow::Error> {
        self.update_with_object_response(sui_client, object_response).await
    }

    fn pool_id(&self) -> &ObjectID {
        self.pool_id()
    }

    fn package_id(&self) -> &ObjectID {
        self.package_id()
    }

    // fn router_id(&self) -> &ObjectID {
    //     self.router_id()
    // }

    fn compute_swap_x_to_y_mut(&mut self, amount_specified: u128) -> (u128, u128) {
        self.compute_swap_x_to_y_mut(amount_specified)
    }

    fn compute_swap_y_to_x_mut(&mut self, amount_specified: u128) -> (u128, u128) {
        self.compute_swap_y_to_x_mut(amount_specified)
    }

    fn compute_swap_x_to_y(&self, amount_specified: u128) -> (u128, u128) {
        self.compute_swap_x_to_y(amount_specified)
    }

    fn compute_swap_y_to_x(&self, amount_specified: u128) -> (u128, u128) {
        self.compute_swap_y_to_x(amount_specified)
    }

    fn viable(&self) -> bool {
        self.viable()
    }

    async fn add_swap_to_programmable_transaction(
        &self,
        transaction_builder: &TransactionBuilder,
        pt_builder: &mut ProgrammableTransactionBuilder,
        orig_coins: Vec<ObjectID>, // the actual coin object in (that you own and has money)
        x_to_y: bool,
        amount_in: u128,
        amount_out: u128,
        recipient: SuiAddress
    ) -> Result<(), anyhow::Error> {
        self.add_swap_to_programmable_transaction(
            transaction_builder,
            pt_builder,
            orig_coins,
            x_to_y,
            amount_in,
            amount_out,
            recipient
        )
        .await
    }

}

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