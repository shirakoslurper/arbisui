use std::str::FromStr;
use async_trait::async_trait;
use anyhow::{anyhow, Context};

use ethnum::U256;

use futures::{future, TryStreamExt};
use page_turner::PageTurner;
use serde_json::Value;
use fixed::{types::U64F64, consts::E};

use custom_sui_sdk::{
    SuiClient,
    apis::{
        QueryEventsRequest,
        GetDynamicFieldsRequest
    },
    transaction_builder::{TransactionBuilder, ProgrammableObjectArg},
    programmable_transaction_sui_json::ProgrammableTransactionArg
};

use sui_sdk::types::{base_types::{ObjectID, ObjectIDParseError, ObjectType, SuiAddress}, object::Object};
use sui_sdk::types::dynamic_field::DynamicFieldInfo;
use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_sdk::rpc_types::{
    SuiObjectResponse, 
    EventFilter, 
    SuiEvent, 
    SuiMoveValue, 
    SuiTypeTag
};
 
use sui_sdk::json::SuiJsonValue;

use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::account_address::AccountAddress;
use move_core_types::value::MoveValue;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::markets::{Exchange, Market};
use crate::sui_sdk_utils::{self, sui_move_value};
use crate::fast_v2_pool;
use crate::fast_cronje_pool;
use crate::sui_json_utils::move_value_to_json;

#[derive(Debug, Clone)]
pub struct KriyaDex {
    package_id: ObjectID,
    event_struct_tag_to_pool_field: HashMap<StructTag, String>,
}

impl KriyaDex {
    pub fn new(package_id: ObjectID) -> Self {

        KriyaDex {
            package_id,
            event_struct_tag_to_pool_field: HashMap::new(),
        }
    }
}

impl KriyaDex {
    pub fn package_id(&self) -> &ObjectID {
        &self.package_id
    }

    fn event_package_id(&self) -> &ObjectID {
        &self.package_id
    }

    fn event_filters(&self) -> Vec<EventFilter> {
        self
            .event_struct_tag_to_pool_field()
            .keys()
            .cloned()
            .map(|event_struct_tag| {
                EventFilter::MoveEventType(
                    event_struct_tag
                )
            })
            .collect::<Vec<_>>()
    }

    fn event_struct_tag_to_pool_field(&self) -> &HashMap<StructTag, String> {
        &self.event_struct_tag_to_pool_field
    }

    // Cetus has us query for events
    async fn get_all_markets_(&mut self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error> {
        println!("Get all markets!");

        let pool_created_events = sui_client
            .event_api()
            .pages(
                QueryEventsRequest {
                    query: EventFilter::MoveEventType(
                        StructTag::from_str(
                            &format!("{}::spot_dex::PoolCreatedEvent", self.package_id)
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

        println!("kriya len pool_created_events: {}", pool_created_events.len());

        let pool_ids = pool_created_events
            .into_iter()
            .map(|pool_created_event| {
                let parsed_json = pool_created_event.parsed_json;
                if let Value::String(pool_id_value) = parsed_json.get("pool_id").context("Failed to get pool_id for a CetusMarket")? {
                    // println!("turbos: pool_id: {}", pool_id_value);
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
    
                    let fields = sui_sdk_utils::read_fields_from_object_response(&object_response).context(format!("Missing fields for pool {}.", pool_id))?;
    
                    let (coin_x, coin_y) = get_coin_pair_from_object_response(&object_response)?;
    
                    // Add event filter struct tags
                    self.event_struct_tag_to_pool_field.insert(
                        StructTag::from_str(
                            &format!("{}::spot_dex::SwapEvent<{}>", self.package_id, coin_x)
                        )?,
                        "pool_id".to_string()
                    );

                    self.event_struct_tag_to_pool_field.insert(
                        StructTag::from_str(
                            &format!("{}::spot_dex::SwapEvent<{}>", self.package_id, coin_y)
                        )?,
                        "pool_id".to_string()
                    );

                    // let is_stable = sui_move_value::get_bool(
                    //     &fields,
                    //     "is_stable"
                    // )?;

                    // // Filtering for uncorrelated pools ONLY right now
                    // // SKIP STABLE
                    // if is_stable {
                    //     // println!("{:#?}", object_response);
                    //     return Ok(None);
                    // }

                    Ok(
                        Some(
                            Box::new(
                                KriyaDexMarket {
                                    parent_exchange: self.clone(),  // reevaluate clone
                                    coin_x,
                                    coin_y,
                                    pool_id,
                                    computing_pool: None    // We'll grab this later so we don't have to deal with async stuff
                                }
                            ) as Box<dyn Market>
                        )
                    )
                })
                .filter_map(|x| x.transpose())
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

    pub fn computing_pool_from_object_response(&self, response: &SuiObjectResponse) -> Result<KriyaComputingPool, anyhow::Error> {

        let fields = sui_sdk_utils::read_fields_from_object_response(response).context("missing fields")?;
        
        // panic!("fields: {:#?}", fields);

        let id = response.data.as_ref().context("data field from object response is None")?.object_id;
        
        let token_x = u64::from_str(
            &sui_move_value::get_string(
                &fields, 
                "token_x"
            )?
        )?;

        let token_y = u64::from_str(
            &sui_move_value::get_string(
                &fields, 
                "token_y"
            )?
        )?;

        let protocol_fee_percent = u64::from_str(
            &sui_move_value::get_string(
                &fields, 
                "protocol_fee_percent"
            )?
        )?;

        let lp_fee_percent = u64::from_str(
            &sui_move_value::get_string(
                &fields, 
                "lp_fee_percent"
            )?
        )?;

        let is_swap_enabled = sui_move_value::get_bool(
            &fields, 
            "is_swap_enabled"
        )?;

        let is_stable = sui_move_value::get_bool(
            &fields, 
            "is_stable"
        )?;

        let scale_x = u64::from_str(
            &sui_move_value::get_string(
                &fields, 
                "scaleX"
            )?
        )?;

        let scale_y = u64::from_str(
            &sui_move_value::get_string(
                &fields, 
                "scaleY"
            )?
        )?;

        let pool = if !is_stable {
            KriyaComputingPool::Uncorrelated(
                fast_v2_pool::Pool {
                    id,
                    reserve_x: token_x,
                    reserve_y: token_y,
                    protocol_fee: protocol_fee_percent,
                    lp_fee: lp_fee_percent,
                    unlocked: is_swap_enabled,
                }
            )
        } else {
            KriyaComputingPool::Stable(
                fast_cronje_pool::Pool {
                    id,
                    reserve_x: token_x,
                    reserve_y: token_y,
                    protocol_fee: protocol_fee_percent,
                    lp_fee: lp_fee_percent,
                    scale_x,
                    scale_y,
                    unlocked: is_swap_enabled,
                }
            )
        };

        Ok(
            pool
        )
    }
}

#[async_trait]
impl Exchange for KriyaDex {
    fn package_id(&self) -> &ObjectID {
       self.package_id()
    }

    fn event_package_id(&self) -> &ObjectID {
        &self.event_package_id()
    }

    fn event_filters(&self) -> Vec<EventFilter> {
        self.event_filters()
    }

    fn event_struct_tag_to_pool_field(&self) -> &HashMap<StructTag, String>{
        self.event_struct_tag_to_pool_field()
    }

    async fn get_all_markets(&mut self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error> {
        self.get_all_markets_(sui_client).await
    }

    async fn get_pool_id_to_object_response(&self, sui_client: &SuiClient, markets: &[Box<dyn Market>]) -> Result<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error> {
        self.get_pool_id_to_object_response(sui_client, markets).await
    }
}

#[derive(Debug, Clone)]
pub enum KriyaComputingPool {
    Uncorrelated(fast_v2_pool::Pool),
    Stable(fast_cronje_pool::Pool)
}

impl KriyaComputingPool {
    fn coin_x_price(&self) -> U64F64 {
        match self {
            KriyaComputingPool::Uncorrelated(cp) => {
                U64F64::from_num(cp.reserve_x / cp.reserve_y)
            },
            KriyaComputingPool::Stable(cp) => {
                U64F64::from_num(cp.reserve_x / cp.reserve_y)
            },
        }
    }

    fn coin_y_price(&self) -> U64F64 {
        match self {
            KriyaComputingPool::Uncorrelated(cp) => {
                U64F64::from_num(cp.reserve_y / cp.reserve_x)
            },
            KriyaComputingPool::Stable(cp) => {
                U64F64::from_num(cp.reserve_y / cp.reserve_x)
            },
        }
    }

    fn apply_swap_effects(
        &mut self,
        x_to_y: bool,
        amount_in: u64,
        amount_out: u64
    ) {
        match self {
            KriyaComputingPool::Uncorrelated(cp) => {
                cp.apply_swap_effects(x_to_y, amount_in, amount_out)
            },
            KriyaComputingPool::Stable(cp) => {
                cp.apply_swap_effects(x_to_y, amount_in, amount_out)
            },
        }
    }

    fn apply_add_liquidity_effects(
        &mut self,
        amount_x: u64,
        amount_y: u64
    ) {
        match self {
            KriyaComputingPool::Uncorrelated(cp) => {
                cp.apply_add_liquidity_effects(amount_x, amount_y)
            },
            KriyaComputingPool::Stable(cp) => {
                cp.apply_add_liquidity_effects(amount_x, amount_y)
            },
        }
    }

    fn apply_remove_liquidity_effects(
        &mut self,
        amount_x: u64,
        amount_y: u64
    ) {
        match self {
            KriyaComputingPool::Uncorrelated(cp) => {
                cp.apply_remove_liquidity_effects(amount_x, amount_y)
            },
            KriyaComputingPool::Stable(cp) => {
                cp.apply_remove_liquidity_effects(amount_x, amount_y)
            },
        }
    }

    fn unlocked(
        &self
    ) -> bool{
        match self {
            KriyaComputingPool::Uncorrelated(cp) => {
                cp.unlocked
            },
            KriyaComputingPool::Stable(cp) => {
                cp.unlocked
            },
        }
    }

    fn calc_swap_exact_amount_in(
        &self,
        amount_in: u64,
        x_to_y: bool
    ) -> (u64, u64) {
        match self {
            KriyaComputingPool::Uncorrelated(cp) => {
                cp.calc_swap_exact_amount_in(amount_in, x_to_y)
            },
            KriyaComputingPool::Stable(cp) => {
                cp.calc_swap_exact_amount_in(amount_in, x_to_y)
            },
        }
    }

}

#[derive(Debug, Clone)]
struct KriyaDexMarket {
    parent_exchange: KriyaDex,
    coin_x: TypeTag,
    coin_y: TypeTag,
    pool_id: ObjectID,
    computing_pool: Option<KriyaComputingPool>
}

impl KriyaDexMarket {
    fn coin_x(&self) -> &TypeTag {
        &self.coin_x
    }

    fn coin_y(&self) -> &TypeTag {
        &self.coin_y
    }

    fn coin_x_price(&self) -> Option<U64F64> {
        if let Some(kcp) = &self.computing_pool {
            Some (
                kcp.coin_x_price()
            )
        } else {
            None
        }
    }

    fn coin_y_price(&self) -> Option<U64F64> {
        if let Some(kcp) = &self.computing_pool {
            Some (
                kcp.coin_y_price()
            )
        } else {
            None
        }
    }

    // rename to "..pool_object_response"
    // recall that we 
    async fn update_with_object_response(&mut self, sui_client: &SuiClient, object_response: &SuiObjectResponse) -> Result<(), anyhow::Error> {
        // let fields = sui_sdk_utils::read_fields_from_object_response(object_response).context("Missing fields for object_response.")?;

        self.computing_pool = Some(self.parent_exchange.computing_pool_from_object_response(object_response)?);
        Ok(())
    }

    fn update_with_event(&mut self, event: &SuiEvent) -> Result<(), anyhow::Error> {
        let type_ = &event.type_;
        let event_parsed_json = &event.parsed_json;
        let computing_pool = self
            .computing_pool
            .as_mut()
            .context("computing_pool is None")?;

        // Amortize this so we only allocate these once. Cant be computed at compile time.
        let swap_coin_x_event_type = StructTag::from_str(
                &format!("{}::spot_dex::SwapEvent<{}>", &self.parent_exchange.package_id, &self.coin_x)
            ).context("KriyaDEX: failed to create event struct tag")?;

        let swap_coin_y_event_type = StructTag::from_str(
                &format!("{}::spot_dex::SwapEvent<{}>", &self.parent_exchange.package_id, &self.coin_y)
            ).context("KriyaDEX: failed to create event struct tag")?;

        let add_liq_event_type = StructTag::from_str(
                &format!("{}::spot_dex::LiquidityAddedEvent", &self.parent_exchange.package_id)
            ).context("KriyaDEX: failed to create event struct tag")?;

        let remove_liq_event_type = StructTag::from_str(
                &format!("{}::spot_dex::LiquidityRemovedEvent", &self.parent_exchange.package_id)
            ).context("KriyaDEX: failed to create event struct tag")?;

        // let update_config_event_type = StructTag::from_str(
        //         &format!("{}::spot_dex::ConfigUpdatedEvent", &self.parent_exchange.package_id)
        //     ).context("KriyaDEX: failed to create event struct tag")?;

        match type_ {
            swap_coin_x_event_type => {
                let amount_in = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("amount_in").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent amount_in is not Value::String."))
                    }
                )?;
                let amount_out = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("amount_out").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent amount_out is not Value::String."))
                    }
                )?;

                computing_pool.apply_swap_effects(
                    true,
                    amount_in,
                    amount_out
                );

            },
            swap_coin_y_event_type => {
                let amount_in = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("amount_in").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent amount_in is not Value::String."))
                    }
                )?;
                let amount_out = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("amount_out").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent amount_out is not Value::String."))
                    }
                )?;

                // coin x
                let x_to_y = false;
                
                computing_pool.apply_swap_effects(
                    x_to_y,
                    amount_in,
                    amount_out
                );

            },
            add_liq_event_type => {
                let amount_x = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("amount_x").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent amount_in is not Value::String."))
                    }
                )?;
                let amount_y = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("amount_y").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent amount_out is not Value::String."))
                    }
                )?;

                computing_pool.apply_add_liquidity_effects(
                    amount_x,
                    amount_y
                );
            },
            remove_liq_event_type => {
                let amount_x = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("amount_x").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent amount_in is not Value::String."))
                    }
                )?;
                let amount_y = u64::from_str(
                    if let serde_json::Value::String(str) = event_parsed_json.get("amount_y").context("")? {
                        str
                    } else {
                        return Err(anyhow!("SwapEvent amount_out is not Value::String."))
                    }
                )?;

                computing_pool.apply_remove_liquidity_effects(
                    amount_x,
                    amount_y
                );
            },
            // update_config_event_type => {
            //     // return Err(anyhow!());
            //     // This is an update to all pools....
            // },
            _ => {
                // do nothing
            }
        }

        Ok(())

    }

    fn pool_id(&self) -> &ObjectID {
        &self.pool_id
    }

    fn package_id(&self) -> &ObjectID {
        &self.parent_exchange.package_id
    }

    fn compute_swap_x_to_y(&self, amount_specified: u128) -> (u128, u128) {
        
        let (amount_x_delta, amount_y_delta) = self
            .computing_pool
            .as_ref()
            .unwrap()
            .calc_swap_exact_amount_in(
                amount_specified as u64,
                true
            );

        (amount_x_delta as u128, amount_y_delta as u128)
    }

    fn compute_swap_y_to_x(&self, amount_specified: u128) -> (u128, u128) {
        
        let (amount_x_delta, amount_y_delta) = self
            .computing_pool
            .as_ref()
            .unwrap()
            .calc_swap_exact_amount_in(
                amount_specified as u64,
                false
            );

        (amount_x_delta as u128, amount_y_delta as u128)
    }

    async fn add_swap_to_programmable_transaction(
        &self,
        transaction_builder: &TransactionBuilder,
        pt_builder: &mut ProgrammableTransactionBuilder,
        orig_coins: Option<Vec<ObjectID>>, // the actual coin object in (that you own and has money)
        x_to_y: bool,
        amount_in: u128,
        amount_out: u128,
    ) -> Result<(), anyhow::Error> {

        // Arg0: &mut Pool
        // Arg1: Coin<> as single coin (we need to merge)
        // Arg2: u64 amount_in
        // Arg3: u64 min_amount_out

        let coin_x_sui_type_tag = SuiTypeTag::new(format!("{}", self.coin_x));
        let coin_y_sui_type_tag = SuiTypeTag::new(format!("{}", self.coin_y));

        let pool_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::from_object_id(self.pool_id.clone())
        );

        // Arg1: &mut Pool
        // The coins to merge into ther first coin
        let orig_coin_arg = if let Some(mut orig_coins_to_merge) = orig_coins {
            
            let primary_coin_id = orig_coins_to_merge.pop().context("No primary coin id to pop.")?;

            // Remaining coins to merge
            if orig_coins_to_merge.len() > 0 {
                let coins_to_merge_obj_args = orig_coins_to_merge
                    .into_iter()
                    .map(|coin_to_merge| {
                        ProgrammableObjectArg::ObjectID(coin_to_merge.clone())
                    })
                    .collect::<Vec<ProgrammableObjectArg>>();

                let primary_coin_obj_arg = ProgrammableObjectArg::ObjectID(
                    primary_coin_id
                );

                ProgrammableTransactionArg::Argument(
                    transaction_builder.programmable_merge_coins(
                        pt_builder,
                        primary_coin_obj_arg.clone(),
                        coins_to_merge_obj_args,
                    ).await?
                )
            } else {
                ProgrammableTransactionArg::SuiJsonValue(
                    SuiJsonValue::from_object_id(primary_coin_id)
                )
            }
        } else {
            ProgrammableTransactionArg::Argument(
                transaction_builder.programmable_split_gas_coin(pt_builder, amount_in as u64).await
            )
        };

        // Arg2: u64
        // The amount in
        let amount_specified_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::U64(amount_in as u64)
                )
                .context("failed to convert MoveValue for amount_specified to JSON")?
            )?
        );

        // Arg3: u64
        // The minimum amount out we're expecting 
        let amount_threshold_arg = ProgrammableTransactionArg::SuiJsonValue(
            SuiJsonValue::new(
                move_value_to_json(
                    &MoveValue::U64(amount_out as u64)
                )
                .context("failed to convert MoveValue for amount_specified to JSON")?
            )?
        );

        let call_args = vec![
            pool_arg,
            orig_coin_arg,
            amount_specified_arg,
            amount_threshold_arg
        ];

        let type_args = vec![
            coin_x_sui_type_tag,
            coin_y_sui_type_tag
        ];

        let function = if x_to_y {
            "swap_token_x_"
        } else {
            "swap_token_y_"
        };

        transaction_builder.programmable_move_call(
            pt_builder,
            self.parent_exchange.package_id.clone(),
            "spot_dex",
            function,
            type_args,
            call_args
        ).await?;

        Ok(())
    }

    fn viable(&self) -> bool {
        if let Some(cp) = &self.computing_pool {
            cp.unlocked()
        } else {
            false
        }
    }
}

#[async_trait]
impl Market for KriyaDexMarket {
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
        orig_coins: Option<Vec<ObjectID>>, // the actual coin object in (that you own and has money)
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
        )
        .await
    }
}

fn get_coin_pair_from_object_response (
    object_response: &SuiObjectResponse
) -> Result<(TypeTag, TypeTag), anyhow::Error> {
    // println!("{:#?}", response);
    if let Some(data) = object_response.clone().data {
        if let Some(type_) = data.type_ {
            if let ObjectType::Struct(move_object_type) = type_ {
                let type_params = move_object_type.type_params();

                // Ty0 is the first coin
                // Ty1 is the second coin

                Ok(
                    (
                        type_params.get(0).context("Missing coin_x")?.clone(),
                        type_params.get(1).context("Missing coin_y")?.clone(),
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