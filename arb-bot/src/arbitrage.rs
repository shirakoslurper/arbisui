//hello

use anyhow::{anyhow, Context};

use custom_sui_sdk::SuiClient;
use custom_sui_sdk::programmable_transaction_sui_json::ProgrammableTransactionArg;
use custom_sui_sdk::transaction_builder::{
    TransactionBuilder,
    // ProgrammableObsArg
};

use ethnum::I256;

use move_core_types::language_storage::TypeTag;

use shared_crypto::intent::Intent;

use sui_keys::keystore::{Keystore, AccountKeystore};
use sui_sdk::rpc_types::{
    SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponseOptions,
    SuiExecutionStatus
};
use sui_sdk::SUI_COIN_TYPE;
use sui_sdk::types::{
    base_types::{SuiAddress, ObjectID},
    transaction::TransactionData,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::Transaction,
    quorum_driver_types::ExecuteTransactionRequestType
};

use std::fmt::{Debug, Error, Formatter};
use std::str::FromStr;

use crate::markets::Market;
use crate::market_graph::MarketGraph;

#[derive(Debug, Clone)]
pub struct OptimizedResult<'a> {
    pub path: Vec<DirectedLeg<'a>>,
    pub amount_in: u128,
    pub amount_out: u128,
    pub profit: I256
}

#[derive(Clone)]
pub struct DirectedLeg<'a> {
    pub x_to_y: bool,
    pub market: &'a Box<dyn Market>,
}

#[derive(Clone)]
pub struct DirectedLegResult<'a> {
    pub x_to_y: bool,
    pub market: &'a Box<dyn Market>,
    pub amount_in: u128,
    pub amount_out: u128,
}

impl<'a> Debug for DirectedLeg<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        f
        .debug_struct("DirectedLeg")
        .field("x_to_y", &self.x_to_y)
        .field("market.coin_x()", &self.market.coin_x())
        .field("market.coin_y()", &self.market.coin_y())
        .field("market.coin_x_price()", &self.market.coin_x_price())
        .field("market.coin_y_price()", &self.market.coin_y_price())
        .field("market.pool_id()", &self.market.pool_id())
        .finish()
    }
}

// pub fn search() {

// } -> Result<Optimized, Result >

// For a single path
// Objective is maximizing profit
pub fn optimize_starting_amount_in<'a>(
    path: &'a [TypeTag], 
    market_graph: &'a MarketGraph<'a>
) -> Result<OptimizedResult<'a>, anyhow::Error> {

    // let expanded_paths = 
    let mut starting_amount_in = 0;
    let mut delta = 0;

    let mut expanded_paths = Vec::<Vec::<DirectedLeg>>::new();
    // println!("Expanded paths: {:#?}", expanded_paths);
    expanded_paths.push(vec![]);
    // println!("Expanded paths: {:#?}", expanded_paths);

    for pair in path[..].windows(2) {
        let orig = &pair[0];
        let dest = &pair[1];

        let orig_to_dest_markets = market_graph
            .graph
            .edge_weight(&orig, &dest)
            .unwrap();

        let mut expanded_paths_extended = Vec::<Vec::<DirectedLeg>>::new();


        for expanded_path in expanded_paths {
            for (_, market_info) in orig_to_dest_markets.iter() {
                let mut expanded_path_extended = expanded_path
                    .clone();

                let x_to_y = (orig, dest) == (market_info.market.coin_x(), market_info.market.coin_y());

                expanded_path_extended.push(
                        DirectedLeg {
                            x_to_y,
                            market: &market_info.market
                        }
                    );

                expanded_paths_extended.push(expanded_path_extended);
            }
        }

        expanded_paths = expanded_paths_extended;
    }

    // println!("Expanded paths: {:#?}", expanded_paths);

    // Golden section search:
    // - for unimodal functions
    // - does not get caught in local extrema

    let mut optimized_results = Vec::new();

    let gr_num = 121393u128;
    let gr_den = 75025u128;

    for expanded_path in expanded_paths {
        let mut a = 0u128;
        let mut b = u64::MAX as u128;

        let mut c = b - (((b - a) * gr_den) / gr_num);
        let mut d = a + (((b - a) * gr_den) / gr_num);

        if b < a {
            println!("b: {}, a: {}", b , a);
        }

        while (I256::from(b) - I256::from(a)).abs() > 1 {
            let amount_out_c = amount_out(&expanded_path, c)?;
            let amount_out_d = amount_out(&expanded_path, d)?;
            let profit_c = I256::from(amount_out_c) - I256::from(c);
            let profit_d = I256::from(amount_out_d) - I256::from(d);

            if profit_c > profit_d {
                b = d;
            } else {
                a = c;
            }

            c = b - (((b - a) * gr_den) / gr_num);
            d = a + (((b - a) * gr_den) / gr_num);
        }

        let optimized_amount_in = (b + a) / 2;
        let optimized_amount_out = amount_out(&expanded_path, optimized_amount_in)?;
        let optimized_profit = I256::from(optimized_amount_out) - I256::from(optimized_amount_in);

        optimized_results.push(
            OptimizedResult{
                path: expanded_path,
                amount_in: optimized_amount_in,
                amount_out: optimized_amount_out,
                profit: optimized_profit
            }
        )
    }
    
    // println!("optimized_results: {:#?}", optimized_results);

    let first_optimized_result = optimized_results.pop().context("optimized_results is empty")?;

    let profit_maximized_result = optimized_results
        .into_iter()
        .fold(
            first_optimized_result,
            |pmr, optimized_result| {
                if pmr.profit > optimized_result.profit {
                    pmr
                } else {
                    optimized_result
                }
            }
        );

    // if profit_maximized_result.profit > 0 {
        // println!("{} HOP: max_profit = {}", profit_maximized_result.path.len(), profit_maximized_result.profit);
    // }

    Ok(profit_maximized_result)
}

pub fn amount_out(path: &[DirectedLeg], mut amount_in: u128) -> Result<u128, anyhow::Error> {

    for leg in path {
        let coin_x = leg.market.coin_x();
        let coin_y = leg.market.coin_y();

        if leg.x_to_y {
            if leg.market.viable() {
                if amount_in == 0 {
                    return Ok(0);
                }

                let (amount_x, amount_y) = leg.market.compute_swap_x_to_y(amount_in);
                amount_in = amount_y;
            } else {
                amount_in = 0;
            }

        } else {
            if leg.market.viable() {
                if amount_in == 0 {
                    return Ok(0);
                }

                let (amount_x, amount_y) = leg.market.compute_swap_y_to_x(amount_in);
                amount_in = amount_x;
            } else {
                amount_in = 0;
            }
        }
    }

    Ok(amount_in)
}

pub async fn execute_arb<'a>(
    sui_client: &SuiClient,
    optimized_result: OptimizedResult<'a>,
    signer_address: &SuiAddress,
    keystore: &Keystore
) -> Result<(), anyhow::Error> {
    // We'll want to work with the actual amounts out we get!
    let mut amount_in = optimized_result.amount_in;

    for leg in optimized_result.path {
        let mut dry_run_pt_builder = ProgrammableTransactionBuilder::new();

        let orig_coin_type = if leg.x_to_y {
            leg.market.coin_x()
        } else {
            leg.market.coin_y()
        };

        let orig_coin_string = format!("{}", orig_coin_type);

        // Yields SuiRpcResult<Vec<Coin>>
        let coins = sui_client
            .coin_read_api()
            .select_coins(
                signer_address.clone(),
                Some(orig_coin_string),
                amount_in,
                vec![]
            )
            .await?;

        println!("{:#?}", coins);

        // None if our orig_cion is Sui - we'll be splitting off of gas_coin
        let coin_object_ids = if *orig_coin_type == TypeTag::from_str(SUI_COIN_TYPE)? {
            None
        } else {
            Some(
                coins
                    .into_iter()
                    .map(|coin| {
                        coin.coin_object_id
                    })
                    .collect::<Vec<ObjectID>>()
            )
        };

        let predicted_amount_out = if leg.x_to_y {
            leg.market
                .compute_swap_x_to_y(amount_in).1
        } else {
            leg.market
                .compute_swap_y_to_x(amount_in).0
        };

        println!("predicted amount out: {}", predicted_amount_out);

        leg
            .market
            .add_swap_to_programmable_transaction(
                sui_client.transaction_builder(),
                &mut dry_run_pt_builder,
                coin_object_ids.clone(),
                leg.x_to_y,
                amount_in,
                predicted_amount_out,
                signer_address.clone()
            )
            .await?;

        let reference_gas_price = sui_client
            .read_api()
            .get_reference_gas_price()
            .await?
            * 10000;

        // Initial dry run transaction to get gas
        let dry_run_transaction = if coin_object_ids.is_some() {
            sui_client
            .transaction_builder()
            .finish_building_programmable_transaction(
                dry_run_pt_builder,
                signer_address.clone(),
                None,
                reference_gas_price
            )
            .await?
        } else {
            sui_client
            .transaction_builder()
            .finish_building_programmable_transaction_select_all_gas(
                dry_run_pt_builder,
                signer_address.clone(),
                reference_gas_price
            )
            .await?
        };

        let dry_run_result = sui_client
            .read_api()
            .dry_run_transaction_block(
                dry_run_transaction
            )
            .await?;

        let gcs = dry_run_result.effects.gas_cost_summary();
        let gas_budget = (gcs.computation_cost + gcs.storage_cost + gcs.non_refundable_storage_fee) * 10;

        // // println!("Gas Budget: {}", gas_budget);
        // // panic!();
        println!("DRY RUN RESULT: {:#?}", dry_run_result);
        panic!();

        let mut pt_builder = ProgrammableTransactionBuilder::new();

        leg
            .market
            .add_swap_to_programmable_transaction(
                sui_client.transaction_builder(),
                &mut pt_builder,
                coin_object_ids,
                leg.x_to_y,
                amount_in,
                predicted_amount_out,
                signer_address.clone()
            )
            .await?;

        // If our base coin is sui, select all gas
        // We gotta encapsulate a little better ...
        let transaction = if coin_object_ids.is_some() {
            sui_client
            .transaction_builder()
            .finish_building_programmable_transaction(
                pt_builder,
                signer_address.clone(),
                None,
                gas_budget
            )
            .await?
        } else {
            sui_client
            .transaction_builder()
            .finish_building_programmable_transaction_select_all_gas(
                pt_builder,
                signer_address.clone(),
                gas_budget
            )
            .await?
        };

        let signature = keystore.sign_secure(
            &signer_address,
            &transaction,
            Intent::sui_transaction()
        )?;

        let result = sui_client
            .quorum_driver_api()
            .execute_transaction_block(
                Transaction::from_data(
                    transaction,
                    Intent::sui_transaction(),
                    vec![signature]
                ),
                SuiTransactionBlockResponseOptions::full_content(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            ).await?;

        // println!("result: {:#?}", result);
            
        // Set amount_in for next leg
        if let Some(effects) = result.effects {
            match effects.into_status() {
                SuiExecutionStatus::Success => {
                    if let Some(balance_changes) = result.balance_changes {
                        let balance_change_a = balance_changes
                            .get(0)
                            .context("Balance change 0 missing.")?;
    
                        let balance_change_b = balance_changes
                            .get(1)
                            .context("Balance change 1 missing")?;
    
                        if (&balance_change_a.coin_type, &balance_change_b.coin_type) == (leg.market.coin_x(), leg.market.coin_y()) {
                            if leg.x_to_y {
                                amount_in = balance_change_b.amount as u128
                            } else {
                                amount_in = balance_change_a.amount as u128
                            }
                        } else if (&balance_change_b.coin_type, &balance_change_a.coin_type) == (leg.market.coin_x(), leg.market.coin_y()){
                            if leg.x_to_y {
                                amount_in = balance_change_a.amount as u128
                            } else {
                                amount_in = balance_change_b.amount as u128
                            }
                        } else {
                            return Err(anyhow!("Balance change coin types do not match leg's coin types."));
                        }
                    } else {
                        return Err(anyhow!("result.balance_changes missing."))
                    }
                },
                SuiExecutionStatus::Failure { error } => return Err(anyhow!(error)),
            };
    
        }
    }

    Ok(())
}