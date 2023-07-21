use std::str::FromStr;
use async_trait::async_trait;
use anyhow::{anyhow, Context};

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

use sui_sdk::json::SuiJsonValue;

use sui_sdk::types::base_types::{ObjectID, ObjectIDParseError};
use sui_sdk::types::dynamic_field::DynamicFieldInfo;
use sui_sdk::rpc_types::{EventFilter, SuiEvent, SuiMoveValue, SuiObjectDataOptions, SuiMoveStruct, SuiObjectResponse};
 
use move_core_types::language_storage::{StructTag, TypeTag};
use std::collections::{BTreeMap, HashMap};

use crate::markets::{Exchange, Market};
use crate::sui_sdk_utils::{self, sui_move_value};
use crate::{cetus_pool, cetus};

// const GLOBAL: &str = "0xdaa46292632c3c4d8f31f23ea0f9b36a28ff3677e9684980e4438403a67a3d8f";
// const POOLS: &str = "0xf699e7f2276f5c9a75944b37a0c5b5d9ddfd2471bf6242483b03ab2887d198d0";

#[derive(Debug, Clone)]
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
                package_id: ObjectID::from_str(package_id_str).map_err(<ObjectIDParseError as Into<anyhow::Error>>::into)?,
            }
        )
    }
}

impl Cetus {
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

        // let mut markets: Vec<Box<dyn Market>> = Vec::new();

        let markets = pool_created_events
            .iter()
            .map(|pool_created_event| {
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

                        Ok(
                            Box::new(
                                CetusMarket {
                                    parent_exchange: self.clone(),
                                    coin_x,
                                    coin_y,
                                    pool_id,
                                    coin_x_sqrt_price: None,
                                    coin_y_sqrt_price: None,
                                    computing_pool: None
                                }
                            ) as Box<dyn Market>
                        )
                    } else {
                        Err(anyhow!("Failed to match pattern."))
                    }
            })
            .collect::<Result<Vec<Box<dyn Market>> ,anyhow::Error>>()?;

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

    pub async fn computing_pool_from_object_response(&self, sui_client: &SuiClient, response: &SuiObjectResponse) -> Result<cetus_pool::Pool, anyhow::Error> {
        // println!("{:#?}", response);
        
        let fields = sui_sdk_utils::read_fields_from_object_response(response).context("missing fields")?;

        // println!("cetus pool fields: {:#?}", fields);

        let tick_spacing = sui_move_value::get_number(&fields, "tick_spacing")?;

        let fee_rate = u64::from_str(
            &sui_move_value::get_string(&fields, "fee_rate")?
        )?;

        let liquidity = u128::from_str(
            &sui_move_value::get_string(&fields, "liquidity")?
        )?;

        let current_sqrt_price = u128::from_str(
            &sui_move_value::get_string(&fields, "current_sqrt_price")?
        )?;

        let current_tick_index = sui_move_value::get_number(
            &sui_move_value::get_struct(&fields, "current_tick_index")?,
            "bits"
        )? as i32;

        let fee_growth_global_a = u128::from_str(
            &sui_move_value::get_string(&fields, "fee_growth_global_a")?
        )?;

        let fee_growth_global_b = u128::from_str(
            &sui_move_value::get_string(&fields, "fee_growth_global_b")?
        )?;

        let fee_protocol_coin_a = u64::from_str(
            &sui_move_value::get_string(&fields, "fee_protocol_coin_a")?
        )?;

        let fee_protocol_coin_b = u64::from_str(
            &sui_move_value::get_string(&fields, "fee_protocol_coin_a")?
        )?;

        let tick_manager_struct = sui_move_value::get_struct(&fields, "tick_manager")?;

        let tick_manager_tick_spacing = sui_move_value::get_number(&tick_manager_struct, "tick_spacing")?;

        let tick_manager_ticks_skip_list_struct = sui_move_value::get_struct(&tick_manager_struct, "ticks")?;

        let tick_manager_ticks_skip_list_id = sui_move_value::get_uid(
            &tick_manager_ticks_skip_list_struct,
            "id"
        )?;

        let ticks = self.get_ticks(sui_client, &tick_manager_ticks_skip_list_id).await?;


        Ok(
            cetus_pool::Pool {
                tick_spacing,
                fee_rate,
                liquidity,
                current_sqrt_price,
                current_tick_index,
                fee_growth_global_a,
                fee_growth_global_b,
                fee_protocol_coin_a,
                fee_protocol_coin_b,
                tick_manager: cetus_pool::tick::TickManager {
                    tick_spacing: tick_manager_tick_spacing,
                    ticks
                },
                is_pause: false
            }
        )
    }

    async fn get_ticks(
        &self,
        sui_client: &SuiClient, 
        ticks_skip_list_id: &ObjectID
    ) -> Result<BTreeMap<i32, cetus_pool::tick::Tick>, anyhow::Error> {

        // let aa = sui_client
        //     .read_api()
        //     .get_object_with_options(
        //         ticks_skip_list_id.clone(),
        //         SuiObjectDataOptions::new().with_type()
        //     )
        //     .await?;

        // println!("ticks skip_list: {:#?}", aa);

        let skip_list_dynamic_field_infos = sui_client
            .read_api()
            .pages(
                GetDynamicFieldsRequest {
                    object_id: ticks_skip_list_id.clone(), // We can make this consuming if it saves time
                    cursor: None,
                    limit: None,
                }
            )
            .items()
            .try_collect::<Vec<DynamicFieldInfo>>()
            .await?;

        // println!("skip_list_dynamic_field_infos len: {}", skip_list_dynamic_field_infos.len());
        // println!("skip_list_dynamic_field_infos: {:#?}", skip_list_dynamic_field_infos);

        let node_object_type = format!("0xbe21a06129308e0495431d12286127897aff07a8ade3970495a4404d97f9eaaa::skip_list::Node<{}::tick::Tick>", self.package_id);

        let node_object_ids = skip_list_dynamic_field_infos
            .into_iter()
            .filter(|dynamic_field_info| {
                node_object_type == dynamic_field_info.object_type
            })
            .map(|tick_dynamic_field_info| {
                tick_dynamic_field_info.object_id
            })
            .collect::<Vec<ObjectID>>();

        let node_object_responses = sui_sdk_utils::get_object_responses(sui_client, &node_object_ids).await?;

        // println!("{:#?}", node_object_responses);

        let tick_index_to_tick = node_object_responses
            .into_iter()
            .map(|node_object_response| {
                let fields = sui_sdk_utils::read_fields_from_object_response(&node_object_response).context("Missing fields.")?;

                let node_fields = sui_move_value::get_struct(&fields, "value")?;

                let tick_fields = sui_move_value::get_struct(&node_fields, "value")?;

                // println!("tick_fields: {:#?}", tick_fields);

                // println!("1");

                let index = sui_move_value::get_number(
                    &sui_move_value::get_struct(
                        &tick_fields, 
                        "index"
                    )?, 
                    "bits"
                )? as i32;

                // println!("2");

                let sqrt_price = u128::from_str(
                    &sui_move_value::get_string(&tick_fields,"sqrt_price")?
                )?;

                // println!("3");

                let liquidity_net = u128::from_str(
                    &sui_move_value::get_string(
                        &sui_move_value::get_struct(
                            &tick_fields, 
                            "liquidity_net"
                        )?, 
                        "bits"
                    )?
                )? as i128;

                // println!("4");

                let liquidity_gross = u128::from_str(
                    &sui_move_value::get_string(&tick_fields, "liquidity_gross")?
                )?;

                // println!("5");

                let fee_growth_outside_a = u128::from_str(
                    &sui_move_value::get_string(&tick_fields,"fee_growth_outside_a")?
                )?;

                // println!("6");

                let fee_growth_outside_b = u128::from_str(
                    &sui_move_value::get_string(&tick_fields,"fee_growth_outside_b")?
                )?;

                // println!("7");

                // println!("tick_fields: {:#?}", tick_fields);

                let tick = cetus_pool::tick::Tick{
                    index,
                    sqrt_price,
                    liquidity_net,
                    liquidity_gross,
                    fee_growth_outside_a,
                    fee_growth_outside_b,
                };

                Ok((index, tick))
            })
            .collect::<Result<BTreeMap<i32, cetus_pool::tick::Tick>, anyhow::Error>>()?;

        Ok(tick_index_to_tick)

    }
}

#[async_trait]
impl Exchange for Cetus {
    fn package_id(&self) -> &ObjectID {
        self.package_id()
    }

    // Cetus has us query for events
    async fn get_all_markets(&self, sui_client: &SuiClient) -> Result<Vec<Box<dyn Market>>, anyhow::Error> {
        self.get_all_markets(sui_client).await
    }

    async fn get_pool_id_to_object_response(&self, sui_client: &SuiClient, markets: &[Box<dyn Market>]) -> Result<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error> {
        self.get_pool_id_to_object_response(sui_client, markets).await
    }
}

#[derive(Debug, Clone)]
struct CetusMarket {
    parent_exchange: Cetus,
    coin_x: TypeTag,
    coin_y: TypeTag,
    pool_id: ObjectID,
    coin_x_sqrt_price: Option<U64F64>, // In terms of y. x / y
    coin_y_sqrt_price: Option<U64F64>, // In terms of x. y / x
    computing_pool: Option<cetus_pool::Pool>
}

impl CetusMarket {
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

    async fn update_with_object_response(&mut self, sui_client: &SuiClient, object_response: &SuiObjectResponse) -> Result<(), anyhow::Error> {
        let fields = sui_sdk_utils::read_fields_from_object_response(object_response).context("Missing fields for object_response.")?;
        // println!("cetus fields: {:#?}", fields);
        let coin_x_sqrt_price = U64F64::from_bits(
            u128::from_str(
                &sui_move_value::get_string(&fields, "current_sqrt_price")?
            )?
        );

        let coin_y_sqrt_price = U64F64::from_num(1) / coin_x_sqrt_price;
        
        self.coin_x_sqrt_price = Some(coin_x_sqrt_price);
        self.coin_y_sqrt_price = Some(coin_y_sqrt_price);

        // println!("coin_x<{}>: {}", self.coin_x, self.coin_x_price.unwrap());
        // println!("coin_y<{}>: {}\n", self.coin_y, self.coin_y_price.unwrap());

        self.computing_pool = Some(self.parent_exchange.computing_pool_from_object_response(sui_client, object_response).await?);

        // println!("finised updating cetus pool");

        Ok(())
    }

    fn pool_id(&self) -> &ObjectID {
        &self.pool_id
    }

    fn package_id(&self) -> &ObjectID {
        &self.parent_exchange.package_id
    }

    // Better handling of computing pool being None
    fn compute_swap_x_to_y_mut(&mut self, amount_specified: u64) -> (u64, u64) {
        // println!("cetus compute_swap_x_to_y()");

        let swap_result = cetus_pool::swap_in_pool(
            self.computing_pool.as_mut().unwrap(),
            true,
            true,
            cetus_pool::tick_math::MIN_SQRT_PRICE_X64 + 1,
            amount_specified,
            0, // It's hard coded to 0 for now (replace with global config value)
            0,
            false
        );

        (swap_result.amount_in, swap_result.amount_out)
    }

    fn compute_swap_y_to_x_mut(&mut self, amount_specified: u64) -> (u64, u64) {
        // println!("cetus compute_swap_y_to_x()");

        let swap_result = cetus_pool::swap_in_pool(
            self.computing_pool.as_mut().unwrap(),
            false,
            true,
            cetus_pool::tick_math::MAX_SQRT_PRICE_X64 - 1,
            amount_specified,
            0, // It's hard coded to 0 for now (replace with global config value)
            0,
            false
        );

        (swap_result.amount_out, swap_result.amount_in)
    }

    // Better handling of computing pool being None
    fn compute_swap_x_to_y(&self, amount_specified: u64) -> (u64, u64) {
        // println!("cetus compute_swap_x_to_y()");

        let swap_result = cetus_pool::swap_in_pool(
            &mut self.computing_pool.clone().unwrap(),
            true,
            true,
            cetus_pool::tick_math::MIN_SQRT_PRICE_X64 + 1,
            amount_specified,
            0, // It's hard coded to 0 for now (replace with global config value)
            0,
            true
        );

        (swap_result.amount_in, swap_result.amount_out)
    }

    fn compute_swap_y_to_x(&self, amount_specified: u64) -> (u64, u64) {
        // println!("cetus compute_swap_y_to_x()");

        let swap_result = cetus_pool::swap_in_pool(
            &mut self.computing_pool.clone().unwrap(),
            false,
            true,
            cetus_pool::tick_math::MAX_SQRT_PRICE_X64 - 1,
            amount_specified,
            0, // It's hard coded to 0 for now (replace with global config value)
            0,
            true
        );

        (swap_result.amount_out, swap_result.amount_in)
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

    // async fn add_swap_to_programmable_trasaction(
    //     &self,
    //     transaction_builder: &TransactionBuilder,
    //     pt_builder: &mut ProgrammableTransactionBuilder,
    //     orig_coin: Coin,
    //     orig_type: &TypeTag,
    //     dest_type: &TypeTag,
    //     recipient: &SuiAddress,
    //     deadline: u64,  // Should be option for interface?
    //     amount_in: u128,
    // ) -> Result<()> {
    //     // Very rough but lets do thisss
    //     // We can't add to a result unless theres a function that exists..

    //     // swap_a_b and swap_b_c arguments
    //     // Arg0: &mut Pool<Ty0, Ty1, Ty2>
    //     let pool = SuiJsonValue::from_object_id(self.pool_id.clone());
    //     // Arg1: vector<Coin<Ty0 or Ty1>>
    //     // let coin = 

    //     let coin = SuiJsonValue::from_object_id(orig_coin);

    //     // let coins_orig = pt_builder.make_object_vec(vec![])

    //     // Arg2: u64
    //     let amount_specified = SuiJsonValue::move_value_to_json(
    //         MoveValue::U64(amount_in as u64)
    //     );
    //     // Arg3: u64
    //     let amount_threshold;
    //     // Arg4: u128
    //     let sqrt_price_limit = SuiJsonValue::move_value_to_json(
    //         MoveValue::U128(turbos_pool::math_tick::MAX_SQRT_PRICE_X64)
    //     );
    //     // Arg5: bool
    //     let is_exact_in = SuiJsonValue::move_value_to_json(
    //         MoveValue::Bool(true)
    //     );
    //     // Arg6: address
    //     let recipient = SuiJsonValue::move_value_to_json(
    //         MoveValue::Address(
    //             AccountAddress::from(recipient)
    //         )
    //     );
    //     // Arg7: u64, Needs to be based off of current clock time
    //     let deadline;
    //     // Arg8: &Clock
    //     let clock = SuiJsonValue::from_object_id(
    //         ObjectID::from_str(CLOCK)
    //     );

    //     Ok(())
    // }
}

#[async_trait]
impl Market for CetusMarket {
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

    fn compute_swap_x_to_y_mut(&mut self, amount_specified: u128) -> (u128, u128) {
        let result = self.compute_swap_x_to_y_mut(amount_specified as u64);

        (result.0 as u128, result.1 as u128)
    }

    fn compute_swap_y_to_x_mut(&mut self, amount_specified: u128) -> (u128, u128) {
        let result = self.compute_swap_y_to_x_mut(amount_specified as u64);

        (result.0 as u128, result.1 as u128)
    }

    fn compute_swap_x_to_y(&self, amount_specified: u128) -> (u128, u128) {
        let result = self.compute_swap_x_to_y(amount_specified as u64);

        (result.0 as u128, result.1 as u128)
    }

    fn compute_swap_y_to_x(&self, amount_specified: u128) -> (u128, u128) {
        let result = self.compute_swap_y_to_x(amount_specified as u64);

        (result.0 as u128, result.1 as u128)
    }

    fn viable(&self) -> bool {
        self.viable()
    }

}
