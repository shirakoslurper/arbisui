use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClient, HttpClientBuilder};
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use serde_json::Value;

use move_core_types::language_storage::StructTag;
use sui_json_rpc::{
    CLIENT_SDK_TYPE_HEADER, CLIENT_SDK_VERSION_HEADER, CLIENT_TARGET_API_VERSION_HEADER,
};
use sui_json_rpc_types::{
    ObjectsPage, SuiObjectDataFilter, SuiObjectDataOptions, SuiObjectResponse,
    SuiObjectResponseQuery,
};
// use sui_transaction_builder::{DataReader, 
//     // TransactionBuilder
// };
use sui_types::base_types::{ObjectID, ObjectInfo, SuiAddress};

use crate::apis::{CoinReadApi, EventApi, GovernanceApi, QuorumDriverApi, ReadApi};
use crate::error::{Error, SuiRpcResult};

use crate::transaction_builder::{
    DataReader,
    TransactionBuilder
};

use governor::DefaultDirectRateLimiter;

pub mod apis;
pub mod error;
pub mod transaction_builder;
pub mod programmable_transaction_sui_json;

pub const SUI_COIN_TYPE: &str = "0x2::sui::SUI";
const WAIT_FOR_TX_TIMEOUT_SEC: u64 = 60;

// Provides a non-OpenRPC supporting SuiClientBuilder
// apis copied in as traits like ReadAPI cannot be implemented
// without the types present in the crate.
// We'll use sui_sdk::sui_client_config and 
// sui_sdk::wallet_context as they have been defined

pub struct SuiClientBuilder {
    request_timeout: Duration,
    max_concurrent_requests: usize,
    ws_url: Option<String>,
    // max_requests_per_second: usize
}

impl Default for SuiClientBuilder {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(60),
            max_concurrent_requests: 256,
            ws_url: None,
            // max_requests_per_second: 50
        }
    }
}

impl SuiClientBuilder {
    pub fn request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }

    pub fn max_concurrent_requests(mut self, max_concurrent_requests: usize) -> Self {
        self.max_concurrent_requests = max_concurrent_requests;
        self
    }

    pub fn ws_url(mut self, url: impl AsRef<str>) -> Self {
        self.ws_url = Some(url.as_ref().to_string());
        self
    }

    // pub fn max_requests_per_second(mut self, max_requests_per_second: usize) -> Self {
    //     self.max_requests_per_second = max_requests_per_second;
    //     self
    // }

    pub async fn build(self, http: impl AsRef<str>, rate_limiter: &Arc<DefaultDirectRateLimiter>) -> SuiRpcResult<SuiClient> {
        let client_version = env!("CARGO_PKG_VERSION");
        let mut headers = HeaderMap::new();
        headers.insert(
            CLIENT_TARGET_API_VERSION_HEADER,
            // for rust, the client version is the same as the target api version
            HeaderValue::from_static(client_version),
        );
        headers.insert(
            CLIENT_SDK_VERSION_HEADER,
            HeaderValue::from_static(client_version),
        );
        headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("rust"));

        let ws = if let Some(url) = self.ws_url {
            Some(
                WsClientBuilder::default()
                    .max_request_body_size(2 << 30)
                    .max_concurrent_requests(self.max_concurrent_requests)
                    .set_headers(headers.clone())
                    .request_timeout(self.request_timeout)
                    .build(url)
                    .await?,
            )
        } else {
            None
        };

        let http = HttpClientBuilder::default()
            .max_request_body_size(2 << 30)
            .max_concurrent_requests(self.max_concurrent_requests)
            .set_headers(headers.clone())
            .request_timeout(self.request_timeout)
            .build(http)?;

        let rpc = RpcClient { http, ws };
        let api = Arc::new(rpc);
        let read_api = Arc::new(ReadApi::new(api.clone(), rate_limiter.clone()));
        let quorum_driver_api = QuorumDriverApi::new(api.clone(), rate_limiter.clone());
        let event_api = EventApi::new(api.clone(), rate_limiter.clone());
        let transaction_builder = TransactionBuilder::new(read_api.clone());
        let coin_read_api = CoinReadApi::new(api.clone(), rate_limiter.clone());
        let governance_api = GovernanceApi::new(api.clone(), rate_limiter.clone());

        Ok(SuiClient {
            api,
            transaction_builder,
            read_api,
            coin_read_api,
            event_api,
            quorum_driver_api,
            governance_api,
            // rate_limiter
        })
    }

    fn parse_methods(server_spec: &Value) -> Result<Vec<String>, Error> {
        let methods = server_spec
            .pointer("/methods")
            .and_then(|methods| methods.as_array())
            .ok_or_else(|| {
                Error::DataError(
                    "Fail parsing server information from rpc.discover endpoint.".into(),
                )
            })?;

        Ok(methods
            .iter()
            .flat_map(|method| method["name"].as_str())
            .map(|s| s.into())
            .collect())
    }
}

/// Use [SuiClientBuilder] to build a SuiClient
#[derive(Clone)]
pub struct SuiClient {
    api: Arc<RpcClient>,
    transaction_builder: TransactionBuilder,
    read_api: Arc<ReadApi>,
    coin_read_api: CoinReadApi,
    event_api: EventApi,
    quorum_driver_api: QuorumDriverApi,
    governance_api: GovernanceApi,
    // rate_limiter: &'a Arc<DefaultDirectRateLimiter>,
}

pub(crate) struct RpcClient {
    http: HttpClient,
    ws: Option<WsClient>,
}

impl Debug for RpcClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RPC client. Http: {:?}, Websocket: {:?}",
            self.http, self.ws
        )
    }
}

impl SuiClient {
    pub fn transaction_builder(&self) -> &TransactionBuilder {
        &self.transaction_builder
    }
    pub fn read_api(&self) -> &ReadApi {
        &self.read_api
    }
    pub fn coin_read_api(&self) -> &CoinReadApi {
        &self.coin_read_api
    }
    pub fn event_api(&self) -> &EventApi {
        &self.event_api
    }
    pub fn quorum_driver_api(&self) -> &QuorumDriverApi {
        &self.quorum_driver_api
    }
    pub fn governance_api(&self) -> &GovernanceApi {
        &self.governance_api
    }
}

#[async_trait]
impl DataReader for ReadApi {
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        object_type: StructTag,
    ) -> Result<Vec<ObjectInfo>, anyhow::Error> {
        self.rate_limiter().until_ready().await;

        let mut result = vec![];
        let query = Some(SuiObjectResponseQuery {
            filter: Some(SuiObjectDataFilter::StructType(object_type)),
            options: Some(
                SuiObjectDataOptions::new()
                    .with_previous_transaction()
                    .with_type()
                    .with_owner(),
            ),
        });

        let mut has_next = true;
        let mut cursor = None;

        while has_next {
            let ObjectsPage {
                data,
                next_cursor,
                has_next_page,
            } = self
                .get_owned_objects(address, query.clone(), cursor, None)
                .await?;
            result.extend(
                data.iter()
                    .map(|r| r.clone().try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
            cursor = next_cursor;
            has_next = has_next_page;
        }
        Ok(result)
    }

    async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: SuiObjectDataOptions,
    ) -> Result<SuiObjectResponse, anyhow::Error> {
        self.rate_limiter().until_ready().await;

        Ok(self.get_object_with_options(object_id, options).await?)
    }

    async fn get_reference_gas_price(&self) -> Result<u64, anyhow::Error> {
        self.rate_limiter().until_ready().await;

        Ok(self.get_reference_gas_price().await?)
    }
}