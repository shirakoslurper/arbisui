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
use sui_sdk::rpc_types::{EventFilter, SuiEvent, SuiMoveValue, SuiObjectResponse};
 
use move_core_types::language_storage::{StructTag, TypeTag};
use std::collections::{BTreeMap, HashMap};

use crate::markets::{Exchange, Market};
use crate::sui_sdk_utils::{self, sui_move_value};

// const GLOBAL: &str = "0xdaa46292632c3c4d8f31f23ea0f9b36a28ff3677e9684980e4438403a67a3d8f";
// const POOLS: &str = "0xf699e7f2276f5c9a75944b37a0c5b5d9ddfd2471bf6242483b03ab2887d198d0";

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
                                    coin_x,
                                    coin_y,
                                    pool_id,
                                    coin_x_sqrt_price: None,
                                    coin_y_sqrt_price: None,
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
}

#[derive(Debug, Clone)]
struct CetusMarket {
    coin_x: TypeTag,
    coin_y: TypeTag,
    pool_id: ObjectID,
    coin_x_sqrt_price: Option<U64F64>, // In terms of y. x / y
    coin_y_sqrt_price: Option<U64F64>, // In terms of x. y / x
}

#[async_trait]
impl Market for CetusMarket {
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
        let coin_x_sqrt_price = U64F64::from_bits(
            u128::from_str(
                &sui_move_value::get_string(&fields, "sqrt_price")?
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

    fn compute_swap_x_to_y(&mut self, amount_specified: u128) -> (u128, u128) {
        (0, 0)
    }

    fn compute_swap_y_to_x(&mut self, amount_specified: u128) -> (u128, u128) {
        (0, 0)
    }

}
