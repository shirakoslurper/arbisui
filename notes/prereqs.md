"multicall" = programmable transactions
- build transaction block
send transaction api = json rpc
- send transactionblock via json rpc
events api = websocket


endpoint = blast

How can we do multiple atomic transactions?
Can we do this with the trnsaction builder?

notes: 
- ["I think it's hard to claim that most transactions will use only owned objects, since that depends on who is using the platform (which also may change over time--e.g.g Eth was almost all native token transfers at the beginning, but looks very different today!), the relative costs of each mode of operation, and so on. The important observation is that _many_ important use-cases (fungible token transfers, NFT transfers, mass NFT minting, ...) only need single-owner objects and can get by without full consensus. As you point out, many DeFi use-cases required shared objects (and yes, both depositing liquidity and doing swaps will use shared objects)."](https://discord.com/channels/916379725201563759/955861929346355290/994769080139657276)
- Sui's mempool consensus engiens are narwhal and tusk
	- Narwhal ensures the availability of data submitted to consensus
	- Bullshark agrees on a speicifc ordering of this data
	- Seems like the transactions that compete for resources are ordered in blocks
- Our defi transactions *will* be subject to ordering via Narwhal and Bullshark
- Single owner transactions will not be subject to this consensus stuff - they'll be causally ordered so FIFO with advantage give nto faster movers

Oh fuck ok so:
Calling move-call returns a programmable transaction builder in the transaction data. But its a finished programmable transaction builder.

`<TransactionBuilder>.single_move_call()` does exaclty the same thing as `<ProgrammableTransactionBuilder>`

I terms of the other things we'll need - probably can amultate regular `TransactionBuilder` functions that return `TransactionData`.

```
let mut builder = ProgrammableTransactionBuilder::new();
self.single_move_call(
	&mut builder,
	package_object_id,
	module,
	function,
	type_args,
	call_args,
)
.await?;
let pt = builder.finish();
let input_objects = pt
	.input_objects()?
	.iter()
	.flat_map(|obj| match obj {
		InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => Some(*id),
		_ => None,
	})
	.collect();
let gas_price = self.0.get_reference_gas_price().await?;
let gas = self
	.select_gas(signer, gas, gas_budget, input_objects, gas_price)
	.await?;

Ok(TransactionData::new(
	TransactionKind::programmable(pt),
	signer,
	gas,
	gas_budget,
	gas_price,
))
```

Ok first lets get some data.
Start w/:
- turbos
- anime swap.
- cetus
- movex
- suidex
- aftermath
- flowex
- interlink minadex
- if sui turns out anything like solana, deepbook will be important

used nmap found port 53 open - had to force to a port due to some bug in `SuiClient`. Wait 443 (https) works.
```
(base) asaphbay@Asaphs-MBP ~ % curl --location --request POST https://sui-devnet.blastapi.io:443/ac087eaa-c296-445e-bf12-203a06e4011f --header 'Content-Type: application/json' --data-raw '{"jsonrpc":"2.0", "id":1,"method":"sui_getTotalTransactionBlocks"}'; echo

{"jsonrpc":"2.0","result":"4414818","id":1}
```

Ok. we're having a bit of difficultly building here. Seeing that we're getting a server error 400, the issue probably has to do with one of the await methods. The culprit in `build` seems to be `get_server_info`. Seeig that we don't provide `ws`, there is only one culprit: `let rpc_spec: Value = http.request("rpc.discover", rpc_params![]).await?;`.

`http` here is `jsonrpsee`'s [`HttpClient`](https://docs.rs/jsonrpsee-http-client/0.18.1/jsonrpsee_http_client/struct.HttpClient.html). The string provided is the method name.

After researching, I found that `rpc.discover` is a method name that is provided by JSON-RPC APIs that support the OpenRPC specification. Our rpc doesn't support open RPC (it's private).

Ok we'll copy over most of client builder into out own implementaation of SUI Client builder where we skip this step.

Ok, seems like the info is used to instantiate RpcClient.

It seems that `rpc_spec` from above provide with it: server version and rpc methods.

We found the following:
```
pub(crate) struct RpcClient {
    http: HttpClient,
    ws: Option<WsClient>,
    info: ServerInfo,
}

struct ServerInfo {
    rpc_methods: Vec<String>,
    subscriptions: Vec<String>,
    version: String,
}
```

```
impl SuiClient {
    pub fn available_rpc_methods(&self) -> &Vec<String> {
        &self.api.info.rpc_methods
    }

    pub fn available_subscriptions(&self) -> &Vec<String> {
        &self.api.info.subscriptions
    }

    pub fn api_version(&self) -> &str {
        &self.api.info.version
    }

    pub fn check_api_version(&self) -> SuiRpcResult<()> {
        let server_version = self.api_version();
        let client_version = env!("CARGO_PKG_VERSION");
        if server_version != client_version {
            return Err(Error::ServerVersionMismatch {
                client_version: client_version.to_string(),
                server_version: server_version.to_string(),
            });
        };
        Ok(())
    }
}
```

It seems that serverinfo is purely aesthetic and for information purposes so we can skip this entirely.

> When you add a git dependency, using a path in a repo will cause it to freak out. Simply specify the name of the package and Cargo will look for it

We run into another issue - "only traits defined in the current crate can be implemented for types defined outside of the crate".

So we can't implement any of the traits from `sui_sdk::apis` since they aren't defined in our crate. So let's copy them over into our crate.

Also a lot of the APIs we use in Sui client builder are private to the `sui_sdk` crate.

> We can't have shared dependencies fro workspace members: "Workspaces don't exist as far as individual crates are concerned. Every crate must be able to exist independently, as if there was no workspace. Relative path dependencies are a convenience, but could be anywhere on a filesystem independent of the workspace."

YES. IT FUCKING WORKS.

# arb spotter
First we want to identify arbs and print them.
We'll have to get every new block and operate on the info present there.

Ideally we want to be able to make concurrent requests. We want infomation about all the markets at once.

Alcibiades' version:
- `loop_blocks`
	- creates new market graph
	- creates new bundle generator
	- subsribes to new heads (every new block)
		- what's the equivalent of a new block?
	- runs a loop based on `'blocks: while block_subscription.next().await.is_some()`

Lets try to replicate this loop to start.

# Loop
While we can't subcsribe to new block we can subscribe to packages. The information of the event doesn't matter so much as the event cueing us to act.

We should then take a list of packages - of the dexes we will be arbing and subscribe to them.

We subscribe with:
```
<SuiClient>.event_api().subscribe_event(<sui_json_rpc_types::EventFilter>)
```

To subscribe with multiple [event filters](https://github.com/MystenLabs/sui/blob/main/crates/sui-json-rpc-types/src/sui_event.rs) at once we should likely go for the `Any` variant since we want to trigger on any of the dex package updates:
```
pub enum EventFilter {
    /// Query by sender address.
    Sender(SuiAddress),
    /// Return events emitted by the given transaction.
    Transaction(
        ///digest of the transaction, as base-64 encoded string
        TransactionDigest,
    ),
    /// Return events emitted in a specified Package.
    Package(ObjectID),
    /// Return events emitted in a specified Move module.
    MoveModule {
        /// the Move package ID
        package: ObjectID,
        /// the module name
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        module: Identifier,
    },
    /// Return events with the given move event struct name
    MoveEventType(
        #[schemars(with = "String")]
        #[serde_as(as = "SuiStructTag")]
        StructTag,
    ),
    MoveEventField {
        path: String,
        value: Value,
    },
    /// Return events emitted in [start_time, end_time] interval
    #[serde(rename_all = "camelCase")]
    TimeRange {
        /// left endpoint of time interval, milliseconds since epoch, inclusive
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "BigInt<u64>")]
        start_time: u64,
        /// right endpoint of time interval, milliseconds since epoch, exclusive
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "BigInt<u64>")]
        end_time: u64,
    },

    All(Vec<EventFilter>),
    Any(Vec<EventFilter>),
    And(Box<EventFilter>, Box<EventFilter>),
    Or(Box<EventFilter>, Box<EventFilter>),
}
```
`All` and `Any` seem to be the extensions of `And` and `Or`.

We should start by constructing this event filter.

> Creating our market graph should use the same information (in terms of package ids).
> There's such a heterogeneity amongst these markets however (in terms of implementation) that we should be careful.
> We'll want to abstract everything out to an interface (traits) so its easy to deal with them. Do note the degree of individuality of each of these markets will need to expresses on our (the client) side.
> 
> This also means we're gonna want to tie in the package ids with our implementations of the interface for each of these markets. I think that I would prefer not tying the exchnages into our `MarketGraph` implementation and instead supply them?

The `Package` variant of `EventFilter` wraps an `ObjectId` we describe in [`sui_types::base_types`](https://github.com/MystenLabs/sui/blob/main/crates/sui-types/src/base_types.rs). It has `FromStr` implemented.

> Hmm, but would this mean that we have to rely on the contracts emitting swap events? Or would they emit them by default? 

The `Exchange` trait. This will be our interface for each exchange. We require `package_id()` so that we can use it to subscribe to all package events. We pass our `Exchanges` into `loop_blocks()`.

We should build out our individual markets starting with the given exchange. We'll have `TokenExchange` acutally implement exchange. We have it as a trait for now as I suspect some implementation details for future functions might differ. We'll see. We can always encapsulate exchange behaviors in a struct.

Since we can cue our actions now, we want to fetch market data!

# fetching market data and defining the market graph
Fetching data and creating our market representing structures will go hand in hand - we want this to go quick!

We also want to be as exchange and market implementation agnostic as possible!

Lets do a more flexible version of this:
```
let v2_markets: Vec<UniswapV2Pair> = uniswap::UniswapV2Pair::get_all_markets(transport)
	.await
	.unwrap();
let v2_market_count = v2_markets.len();
info!("Gathered {} Uniswap V2 Like Markets", v2_market_count);
```

Where we'll implement `get_all_markets` for each of our exchanges - another reason why I think exchange package id and implementations should go in hand is that the implementation of get all markets willl differ per package and thus per package id.

> Thinking about it - tying price calculation and reserves together is not very sensical. An orderbook would have no notion of reserves. Most people will probably end up using a version of an AMM out of familiarity and ease, though. We should maintain flexibility though.

> Remember: you have to declare a module in libe before using it.
> `pub mod markets` theb
> `pub use markets::*`

**I've realized we need to revise how we think about the Exchange trait**
It shouldn't be implemented over some `CoinExchange` struct that captures all token exchange behaviors or there would be no reason in using a traitm but rather over a variety of `<SomeSpecificCoinExchange> struct`.

> At the moment: `impl Trait` only allowed in function and inherent method return types, not in trait method return types.
> 
> Our workaround is using a boxed trait object: 

```
trait A {
    fn new() -> Box<dyn A>;
}
```

**Getting all pools**
According to [this Discord post](https://discord.com/channels/916379725201563759/1006322742620069898/1097022525105520790), it seems that there is a registry object.
[This DEX implementation](https://github.com/interest-protocol/sui-defi/blob/main/dex/sources/core.move) makese it seems likely that pools willl be sotred and indexable from a given storage object.
```
assert!(!object_bag::contains(&storage.pools, type), ERROR_POOL_EXISTS);
////
  object_bag::add(
	&mut storage.pools,
	type,
	Pool {
	  id: pool_id,
	  k_last: _k,
	  lp_coin_supply: supply,
	  balance_x: coin::into_balance<X>(coin_x),
	  balance_y: coin::into_balance<Y>(coin_y),
	  decimals_x: 0,
	  decimals_y: 0,
	  is_stable: false,
	  observations: init_observation_vector(),
	  timestamp_last: current_timestamp,
	  balance_x_cumulative_last: utils::calculate_cumulative_balance((coin_x_value as u256), current_timestamp, 0),
	  balance_y_cumulative_last: utils::calculate_cumulative_balance((coin_y_value as u256), current_timestamp, 0),
	}
  );
```

[Sam says to watch for events.](https://discord.com/channels/916379725201563759/925108748551323649/1053830001256050729) But only we apply all changes (such as individual pool fees if they exist it might be safer to get the objects at an address as long as we are given that address.

**Flameswap Pool storage**
All flameswap pools are stored in a `Bag` type.
```
struct [Global](https://explorer.sui.io/object/0x6b84da4f5dc051759382e60352377fea9d59bc6ec92dc60e0b6387e05274415f?module=implements) has key {
	id: UID,
	has_paused: bool,
	controller: address,
	beneficiary: address,
	pools: Bag,
	users: Table<address, UserInfo>,
	update_cd: u64,
	coin_map: Table<TypeName, CoinInfo>
}
```
Ok so we'll want the `ObjectID` for `Global` and `Bag`. 

[Querying a `PoolRegistry`](https://discord.com/channels/916379725201563759/955861929346355290/1078684767828054069).

`get_dynamic_fields` is to Objects what `get_owned_fields` is to wallets.

There a two types of dynamic fields:
- dynamic fields
	- can store any value with the store ability
- dynamic object fields
	- objects must have the key ability and
	- `id: UID` as the first field

**`get_object_with_options` and `get_dynamic_fields`**
`get_object_with_options` doesn't return anything for pools although `get_dynamic_fields` does.

`get_dynamic_fields` returns a [`DynamicFieldPage`](https://github.com/MystenLabs/sui/blob/main/crates/sui-json-rpc-types/src/lib.rs):
```
pub type DynamicFieldPage = Page<DynamicFieldInfo, ObjectID>;

/// `next_cursor` points to the last item in the page;
/// Reading with `next_cursor` will start from the next item after `next_cursor` if
/// `next_cursor` is `Some`, otherwise it will start from the first item.
#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]

pub struct Page<T, C> {
    pub data: Vec<T>,
    pub next_cursor: Option<C>,
    pub has_next_page: bool,
}
```

It holds [`DynamicFieldInfo`](https://github.com/MystenLabs/sui/blob/main/crates/sui-types/src/dynamic_field.rs):
```
pub struct DynamicFieldInfo {
    pub name: DynamicFieldName,
    #[schemars(with = "Base58")]
    #[serde_as(as = "Readable<Base58, _>")]
    pub bcs_name: Vec<u8>,
    pub type_: DynamicFieldType,
    pub object_type: String,
    pub object_id: ObjectID,
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
}
```

The cursor points at the next page. Either there's a limit defined by the limit argument we pass to `get_dynamic_fields()` and/or an in-built limit.

**flameswap structs**
```
struct LP <phantom Ty0, phantom Ty1> has drop, store {
	dummy_field: bool
}

struct Pool <phantom Ty0, phantom Ty1> has store {
	global: ID,
	coin_x: Balance<Ty0>,
	fee_coin_x: Balance<Ty0>,
	coin_y: Balance<Ty1>,
	fee_coin_y: Balance<Ty1>,
	lp_supply: Supply<[LP]
}

struct Global has key {
	id: UID,
	has_paused: bool,
	controller: address,
	beneficiary: address,
	pools: Bag,
	users: Table<address, UserInfo>,
	update_cd: u64,
	coin_map: Table<TypeName, CoinInfo>
}

struct UserInfo has copy, drop, store {
	point: u64,
	last_update_at: u64,
	update_count: u64,
	extra_point: u256
}

struct CoinInfo has copy, drop, store {
	fees: vector<FeeInfo>,
	total_fee_rate: u64
}

struct FeeInfo has copy, drop, store {
	self_coin_as_fee: bool,
	fee_rate: u64,
	receiver: address
}
```

We can grab the type parameters to get the pairs.
`SuiObjectResponse.data.unwrap().type_.unwrap()` then match with `Struct` -> match with `MoveObjectType` -> match with `Other` -> get `StructTag.type_params` etc. etc.

### parsing the response
```
pub struct SuiObjectResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<SuiObjectData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SuiObjectResponseError>,
}
```
```
pub struct SuiObjectData {
    pub object_id: ObjectID,
    /// Object version.
    #[schemars(with = "AsSequenceNumber")]
    #[serde_as(as = "AsSequenceNumber")]
    pub version: SequenceNumber,
    /// Base64 string representing the object digest
    pub digest: ObjectDigest,
    /// The type of the object. Default to be None unless SuiObjectDataOptions.showType is set to true
    #[schemars(with = "Option<String>")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ObjectType>,
    // Default to be None because otherwise it will be repeated for the getOwnedObjects endpoint
    /// The owner of this object. Default to be None unless SuiObjectDataOptions.showOwner is set to true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<Owner>,
    /// The digest of the transaction that created or last mutated this object. Default to be None unless
    /// SuiObjectDataOptions.showPreviousTransaction is set to true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_transaction: Option<TransactionDigest>,
    /// The amount of SUI we would rebate if this object gets deleted.
    /// This number is re-calculated each time the object is mutated based on
    /// the present storage gas price.
    #[schemars(with = "Option<BigInt<u64>>")]
    #[serde_as(as = "Option<BigInt<u64>>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_rebate: Option<u64>,
    /// The Display metadata for frontend UI rendering, default to be None unless SuiObjectDataOptions.showContent is set to true
    /// This can also be None if the struct type does not have Display defined
    /// See more details in <https://forums.sui.io/t/nft-object-display-proposal/4872>
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<DisplayFieldsResponse>,
    /// Move object content or package content, default to be None unless SuiObjectDataOptions.showContent is set to true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<SuiParsedData>,
    /// Move object content or package content in BCS, default to be None unless SuiObjectDataOptions.showBcs is set to true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcs: Option<SuiRawData>,
}
```
```
pub enum ObjectType {
    /// Move package containing one or more bytecode modules
    Package,
    /// A Move struct of the given type
    Struct(MoveObjectType),
}
```
```
/// Wrapper around StructTag with a space-efficient representation for common types like coins
/// The StructTag for a gas coin is 84 bytes, so using 1 byte instead is a win.
/// The inner representation is private to prevent incorrectly constructing an `Other` instead of
/// one of the specialized variants, e.g. `Other(GasCoin::type_())` instead of `GasCoin`
pub struct MoveObjectType(MoveObjectType_);

/// Even though it is declared public, it is the "private", internal representation for
/// `MoveObjectType`
#[derive(Eq, PartialEq, PartialOrd, Ord, Debug, Clone, Deserialize, Serialize, Hash)]
pub enum MoveObjectType_ {
    /// A type that is not `0x2::coin::Coin<T>`
    Other(StructTag),
    /// A SUI coin (i.e., `0x2::coin::Coin<0x2::sui::SUI>`)
    GasCoin,
    /// A record of a staked SUI coin (i.e., `0x3::staking_pool::StakedSui`)
    StakedSui,
    /// A non-SUI coin type (i.e., `0x2::coin::Coin<T> where T != 0x2::sui::SUI`)
    Coin(TypeTag),
    // NOTE: if adding a new type here, and there are existing on-chain objects of that
    // type with Other(_), that is ok, but you must hand-roll PartialEq/Eq/Ord/maybe Hash
    // to make sure the new type and Other(_) are interpreted consistently.
}
```
We'll be working heavily with `StructTag`s which comes from `use move_core_types::language_storage::StructTag`. But we have a wrapper that allows us to stay outside of `move_core_types`. We can operate in `base_types` alone. `MoveObjectType` comes with the following function:
```
pub fn type_params(&self) -> Vec<TypeTag> {
	match &self.0 {
		MoveObjectType_::GasCoin => vec![GAS::type_tag()],
		MoveObjectType_::StakedSui => vec![],
		MoveObjectType_::Coin(inner) => vec![inner.clone()],
		MoveObjectType_::Other(s) => s.type_params.clone(),
	}
}
```

> Options behind references acnnot be matched to patterns?


How should we represent individual coins? By address? Or by `StructTag`? I'm thinking `StructTag` if it has `Eq` implemented - and it does. `Hash` too.

It might be easier to work with the wrappers from `sui-types`.
Actually - What type representation will we need to call `swap`?

## Goal: Call dex functions so we know what we need.
https://github.com/MystenLabs/sui/blob/d34cb60175192b9abf5cb9e8b0381a7fd66d69ee/crates/sui-sdk/examples/tic_tac_toe.rs

We got three dexes: flameswap, cetus, bluemove
```
    /// Will fail to generate if given an empty ObjVec
    pub fn move_call(
        &mut self,
        package: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        call_args: Vec<CallArg>,
    ) -> anyhow::Result<()> {
        let arguments = call_args
            .into_iter()
            .map(|a| self.input(a))
            .collect::<Result<_, _>>()?;
        self.command(Command::move_call(
            package,
            module,
            function,
            type_arguments,
            arguments,
        ));
        Ok(())
    }

    pub fn programmable_move_call(
        &mut self,
        package: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        arguments: Vec<Argument>,
    ) -> Argument {
        self.command(Command::MoveCall(Box::new(ProgrammableMoveCall {
            package,
            module,
            function,
            type_arguments,
            arguments,
        })))
    }
```

```
    pub async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: &str,
        function: &str,
        type_args: Vec<SuiTypeTag>,
        call_args: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let mut builder = ProgrammableTransactionBuilder::new();
        self.single_move_call(
            &mut builder,
            package_object_id,
            module,
            function,
            type_args,
            call_args,
        )
        .await?;
        let pt = builder.finish();
        let input_objects = pt
            .input_objects()?
            .iter()
            .flat_map(|obj| match obj {
                InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => Some(*id),
                _ => None,
            })
            .collect();
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(signer, gas, gas_budget, input_objects, gas_price)
            .await?;

        Ok(TransactionData::new(
            TransactionKind::programmable(pt),
            signer,
            gas,
            gas_budget,
            gas_price,
        ))
    }
```

> On laziness. Calling async functions without await is lazy (in a good way). We can write an iterator to lazily consume paginated ? Wait nvm thats with blocking functions. 

We're dominated by Cetus and Turbos. Volume is so low, the best we can hope for is high slippage.

24 hr volume is about 4M - If we take 0.2% every day thats not bad. 

### SuiEvent
The `parsed_json` field is of type [`serde_json::Value`](https://docs.rs/serde_json/latest/serde_json/value/enum.Value.html).
```
pub enum Value {
	Null,
	Bool(bool),
	Number(Number),
	String(String),
	Array(Vec<Value>),
	Object(Map<String, Value>),
}
```

For a Cetus pool created event, the `parsed_json` field looks a little like this:
```
Object {
    "coin_type_a": String("dbe380b13a6d0f5cdedd58de8f04625263f113b3f9db32b3e1983f49e2841676::coin::COIN"),
    "coin_type_b": String("5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN"),
    "pool_id": String("0xccf8fe1a4ae49e60757e807e4750b595062631ae2d19d33458d30e9e467631d4"),
    "tick_spacing": Number(60),
}
```

Updating the market graph and our dex API.
- We want to select the markets from all markets
- We want to do as few calls as possible - batch em if we can. 
- It seems that we're dealing with two Uni V3 like CLMMs
- This complicates things
	- Reserves won't be enough to calculate price.
	- Determined by current tick
	- Every pool has its own fee so we'll have to grab that too
	- Calculating price impact/optimizing volume will be dependent on liquidity at ticks
	- We won't want to query when optimizing (time and query resources)
	- Best to grab the tick info we can grab.
- hmmm ok i think I'lll benefit from refactoring with pool variants (so I don't reuse logic esp if these are almost identical)
```
pub struct Pool {
    pub address: Address,
    pub token_0: Address,
    pub token_1: Address,
    pub swap_fee: U256,
    pub pool_variant: PoolVariant,
}
```
Seems like reasonable info to hold for a pool. Pool variant can tell use how to optimize volume.

Rusty sando does bin search thru the EVM but doesnt seem enitrely efficient (does the equivalent of getting all tick data and working with it though.) It's rough but its simple.

Given throughput, however, this might not really be fast enough.

`get_all_markets` just gives us the bare minimum info about markets to acqurie additiona; info about those markets. This would be the `pool_id` and `coin_x` and `coin_y` (which are arguably not as necessary as `pool_id`).

But before we go onto optimization an all that - price should be enough to tell us if there is a cycle.

Seems like for Cetus we focus on sqrt price.

We should end up doing something like [this](https://docs.uniswap.org/sdk/v2/guides/pricing) - fetching pair data and calculating mid price client side.

sqrtratio should be sqrt (token_2 / token_1). we multiply by itself to get the ratio.

The Cetus Github is not up to date with what is posted on-chain:
```
struct Pool<phantom Ty0, phantom Ty1> has store, key {
	id: UID,
	coin_a: Balance<Ty0>,
	coin_b: Balance<Ty1>,
	tick_spacing: u32,
	fee_rate: u64,
	liquidity: u128,
	current_sqrt_price: u128,
	current_tick_index: I32,
	fee_growth_global_a: u128,
	fee_growth_global_b: u128,
	fee_protocol_coin_a: u64,
	fee_protocol_coin_b: u64,
	tick_manager: TickManager,
	rewarder_manager: RewarderManager,
	position_manager: PositionManager,
	is_pause: bool,
	index: u64,
	url: String
}
```

We're using 128 bits to represent sqrt price. Judging from the sdk we're using 64 bits to store the fractional part. 

```
static priceToSqrtPriceX64(price: Decimal, decimalsA: number, decimalsB: number): BN {
	return MathUtil.toX64(price.mul(Decimal.pow(10, decimalsB - decimalsA)).sqrt())
}

static sqrtPriceX64ToPrice(sqrtPriceX64: BN, decimalsA: number, decimalsB: number): Decimal {
	return MathUtil.fromX64(sqrtPriceX64)
	    .pow(2)
	    .mul(Decimal.pow(10, decimalsA - decimalsB))
}
```

Seems like the X64 (fixed pint representation is dependent on decimals).
Basically $\sqrt{P} ^ 2 \cdot 10 ^ {(d_a - d_ b)}$.
But we have to make sure to get that initial sqrtPrice conversion right (sqrtpricex64 / (x^64)).

We also need those decimals.

Given $P$ and knowing the decimals $a$ and $b$ (for the respective coins) we can calculate the price of $A$ in terms of $B$ with:
$${\sqrt{P}} ^ 2 \cdot 10^{a-b}$$

### An aside on Uni V3
Unlike uni v2, uni v3 doesn't track virtual reserves with variables $x$ and $y$. Instead it tracks two variables - liquidity ($L = xy$) and sqrt price ($\sqrt{P} = \sqrt{\frac{y}{x}}$).
We can compute the virtual reserves from these two variables:
$$x = \frac{L}{\sqrt{P}}$$
$$y = L \cdot \sqrt{P}$$
According to the Uni V3 whitepaper, these are convenient as only one of $L$ or $\sqrt{P}$ change at a time.
Price chances when swapping within a tick and liquidity changes when crossing a tick (or when minting or burning liquidity).

### Getting price
Recall
```
pub content: Option<SuiParsedData>
```

```
pub enum SuiParsedData {
    // Manually handle generic schema generation
    MoveObject(SuiParsedMoveObject),
    Package(SuiMovePackage),
}
```

### Implementing price/rates and cycle searches
We should prefer working with fixed point numbers for precision.
We shouldn't have to look up cycles every run.
Find all cycles then evaluate *only the cycles* every block (limited length).

Ok the rates we use when evaulating profitability will be dependent on the direction of the cycle (evaluated in both directions).

So price should be like:
- price_a_in_b
- price_b_in_a (reciprocal of the above)

We can used the fixed crate for native support of fixed point arithmetic (can save on weird conversions and can maintain precision).

OK WE HAVE PRICES. LET'S TRY TO FIND POTENTIAL ARBS NOW.

Let's try to refactor so that getting prices is separate from getting markets.

Maybe should look a little like this:
- Get all markets
- Insert pointers to markets as edges (w/ interior mutability) so that we can mutate the values of the edges?
- Update prices for all edges.
- Or perhaps we should nest price and market info (token pairs, pool info) in an edge struct.
	- Especially since price is directional!
	- We have to think how this might work with a reserve dependent pool
		- Our API should not be aware of out graph implementation
		- We'll only return info - no mutating info in place
			- Make more sense anyways if we're batching
			- May have to defer to a (fast hashmap however)
- Oh shit we could have done get multi object (limited number of rpc calls)
	- Ok so batching for sure
	- Especially since we'll be doing this between every block
	- So it's likely better to zip and map.
- We also have to think about how we'll be applying the changes to the graph edges
	- There are coinstraints regarding what we can use for graph edges

### Boxing trait objects
Trait objects are unsized so the solution is to Box them, since the size of a pointer is known.
Hence:
```
pub struct TokenMarkets {
    pub markets: Vec<Box<dyn Market>>,
}
```
in `mev_bundle_generator`.

### Object ID
```
pub struct ObjectID(
    #[schemars(with = "Hex")]
    #[serde_as(as = "Readable<HexAccountAddress, _>")]
    AccountAddress,
);
```

### Graph Design Decision

Pruning:
We can look for all cycles (non-arb) and omit the cycles that do not include our base currency.
This would 

We're gonna go with a directed graph, this plays better with finding arb cycles and rates.
Why should we do this?
What would finding an arb cycle look like if we didn't do this?
- Say we found A to B to A
- We wouldn't initially know which coin is `coin_x` or `coin_y` for a given market, and would have to run a couple comparisons to get the right rates (direction dependent).
- If we embedded the market id with the price in a directed edge that would give use exactly what we need.

>**The rate should let us know how much of the the dest token we can get with the origin token. So amount of origin * rate should given us amount of dest. So in the rate, the origin token should be the denominator.**

Node lookup sounds like it will be helpful so we'll go for a `GraphMap` variant.
We'll go with `DiGraphMap` since we want it to be directed.

We want to only grab the graph that consists of components connected to base currency (node). We should be able to do this with a recursive search. Simple. Should be built in. LMAO. This is DFS buddy.

Hmm wait a second:
The `address` field of `StructTag` is the address of the module in which the coin is defined. (Can multiple coins be defined in the same module?).

### Choosing our node type
We'll want something that truly describes the coin. Something we can pass a type argument when making our calls. Addresses extracted from `StructTags` are NOT enough.

Let's find out what a call to swap looks like.

The function signature for `transaction_builder::<TransactionBuilder>.move_call(...)` is:
```
pub async fn move_call(
	&self,
	signer: SuiAddress,
	package_object_id: ObjectID,
	module: &str,
	function: &str,
	type_args: Vec<SuiTypeTag>,
	call_args: Vec<SuiJsonValue>,
	gas: Option<ObjectID>,
	gas_budget: u64,
) -> anyhow::Result<TransactionData> {...}
```

It seems that coins as nodes would work best if they were [`SuiTypeTag`s](https://github.com/MystenLabs/sui/blob/a57b2c800ccefeb47baa89b89445ea7785ae0b16/crates/sui-json-rpc-types/src/sui_transaction.rs#L38) or even a type convertible to one.

We can create a `SuiTypeTag` from a `TypeTag`.

Ahh I forgot that the nodes for graph maps basically have to be reference for any type that is not an integer.

> ["Anticipating the debate on whether graphs' should contain additional data or simply focus on structure, petgraph follows the latter idiom."](https://github.com/petgraph/petgraph/issues/325)

It seems that it would be appropriate to store the data separately in a `MarketGraph` field so that it is not destroyed. It's not really good to store data and a reference to it in the same struct though.

### Rethinking our use of graphs
Graphs should just give structure to data relationships.
We should decide the graphs relationship to the `Market` trait and the arbitrage strategy.

I would say that my current idea for how to implement graph is attempting to be as clean and pure as possible.
It also is more closely related to the arbitrag strategy.
We should call it the directed market graph (rate dependent on market).

An edge weight should contain info that:
- Allows us to interact with the underlying exchange of the market (via `Trait`)
	- So it seems that we should store the trait object or at least a reference to it.


```
struct DirectedMarketInfo {
	market: Box<impl Market>,
	price: U64F64
}
```

The price reflects the direction of edge.
We should be able to use the `market` field to generate a call.
But before that we'll need `get_amount_in` and `get_amount_out` functions.
How will we go about this?
- Using an `token_in` and `token_out` parameter for the 
> We should revisit this after we find cycles.

### parallel edges
We'll likely have multiple markets for each pair, so we'll have parallel edges (or something amounting to parallel edges).

Since we can't have parallel edges with a `GraphMap` we would likely have to store far more information in an edge. Finding cycles would be a little harder since there could be multiple cycles in a "cycle" (purely consisting of a vertex set).

### The issue with Bellman Ford
It only finds a single negative weight cycle.
Seems like it would be best to:
- grab all cycles up to a max length
- multiply prices along all directed cycles to assess whether an arb is viable (computer multiplication is faster than log + addition)

### How `bundle_generator` finds arbs
It starts with a bunch of origin tokens.
It grabs the edges adjacent to these origin tokens.
The grabs the edge weight for each edge (`Vec<Box<dyn Market>>`)
Looks for the best ask and best bid market from these markets.

Ultimately it is a single hop only arb machine.

Best rates aren't necessarily the best route. We might have a great rate but tiny possible volume. We want to maximize profit.

For now however, lets just list ALL possible arbs.

### Saving cycle searches
We can look up all cycles up to a certain length and store them to save time. Type of memoization. Evalutation of profitability and optimization (multiplication and basic combinatorics should be much more efficient than some minimum cycle algo).

If we do this then we can probably skip the parallel edges thing. The most "true" representation of a Graph may not be the most efficient.

### Directed
Given a cycle, as long as we query the edge preserving the order, we should be able to easily grab all the correct directional prices.

Now that I think about it it might be most appropriate to store coins in an ordered manner (a tuple perhaps) hahahah.

### Memoizing call generation
If it saves us time then yes ofc. Though I think it may not be easy since calls will vary quite a bit.

### Segregating single load and refreshed info
Initially loaded:
- info needed to get pool fields
Refreshed:
- stored under pool fields

Separating these functionalities by trait should give us the optionality of letting either one or two structs implement these.

The functions that get us thinking about whether we can afford to keep the info in separate structs are `get_tokens_in()` and `get_tokens_out()`.  These are dependent on the inner prices and fees.... So we'll keep everything under one trait.

The issue now is with having an `update()` function. If we watch to do batch updates we have to pass arguments but the arguments may be incredibly different depending on the pool variant (think v3's sqrt price and liquidity vs v2's reserves). 

With a single update we could just take `SuiClient` and receive and apply the changes.

With batch updates it gets a hell of a lot harder.

Think the solution has something to do with generics.

### Using UngraphMap and preserving ordering
In terms of returning prices there is definitely something we can do in terms of generics.

### Clever ways of returning prices
We can use xored hashes. The are order-independent and thus perfect for sets. + xoring maintains the most close to uniform distribution among the xored elements.

### "Cloning" trait objects
As they are defined, trait objects nor boxed trait objects cannot be cloned. (`Box<T>` requires `T` to implement clone).

`Clone` returns `Self` ans `Self` must be sized `dyn` Trait objects have unknown size by definition.

We can use a [workaround](https://stackoverflow.com/questions/30353462/how-to-clone-a-struct-storing-a-boxed-trait-object), but we can also just use `Rc`.

### Ah mistake. Fields should not just be indexable by coin pair.
Wait pool id looks like it'll work better lmaoooo.

### Remember interior mutability? And why we do not want to use `<Rc>.get_mut()` and stuff like that?

From the Rust Programming Language:

> "Via immutable references, `Rc<T>` allows you to share data between multiple parts of your program for reading only."

Key words: "reading only".

> "You cannot borrow a reference-counting pointer as mutable; this is because one of the guarantees it provides is only possible if the structure is read-only."

We cannot use `RefCell` to wrap a trait object because a value wrapped by `RefCell` remains on the stack and must be known at compile time (`dyn Trait` isn't). Remember that `RefCell` wraps a value, not a pointer.

Actually the above is somewhat wrong/right.

>"RefCell stores its contents inline, without any pointer indirection. So `RefCell<dyn Trait>` is a dynamically-sized type, just like `dyn Trait` itself. This means you can't store it directly in a variable, since variables in Rust must have a size known at compile time.
>
>Instead, it has to be behind some sort of pointer type, such as a reference (`&RefCell<T>`), raw pointer (`*const RefCell<T>`), or smart pointer (`Box<RefCell<T>>` or `Rc<RefCell<T>>`). This works, for example:

```
let cell = RefCell::new(A {}); let x: &RefCell<dyn Test> = &cell;
```

As for `RefCell<dyn Trait>` we have to ["unsize" our types](https://www.reddit.com/r/rust/comments/ezex1z/cast_from_an_rcrefcellboxt_to_a_rcrefcellboxdyn/) as the compiler will typically complain and beg you to size the type within `RefCell`.

### humongous pains regarding interior mutability and graphs only being able to take references

The reference to the `TypeTag` we can get is only temporary due to the nature of `borrow_mut()`. I do think we can possibly do some lazy evalutation instead.

It's too much of a pain. Let just spit our `TypeTag`s along our `Vec<Rc<RefCell<dyn Market>>>`

I get the feeling things would be a LOT easier if I just stuck with `UnGraphMap` and `Box<dyn Market>` so we'd only need to store one of a market per edge pair.

### The AGHHH frustrating restrospect of my interior mutability tirade
I though i would be efficient by updating the market that multiple edges point to.

But since i was iterating over edges and applying changes by edge...
I was repeating updates. It was no more efficient (speed wise) than using a separate `Box<dyn Market>` (deeply copied) for each edge element.

The memory sacrifices are worth saving the headache of dealing wiht interior mutability I would say.

WAIT. I was justified. The issue was with cloning `dyn Market`.
We'll try `dyn-clone`.

### directed
Save us time to not look for cycles in a directed graph since we can basically find them in the undirected graph and just reverse their directions.
It's not like we're saving any direction sensitive information anyways.

> We'll keep it in mind though as a possible efficiency boost.

### WHERE IS THE USDC
it hasn't been labeled properly but its ther

Eyballed these stablecoin pairs:
```
coin_x<0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN>: 1.0001721994722193569
coin_y<0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN>: 0.99982783017533358275
```
and 
```
coin_x<0xb231fcda8bbddb31f2ef02e6161444aec64a514e2c89279584ac9806ce9cf037::coin::COIN>: 1.0004730171997856966
coin_y<0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN>: 0.99952720643970027303
```
and
```
coin_x<0xe32d3ebafa42e6011b87ef1087bbc6053b499bf6f095807b9013aff5a6ecd7bb::coin::COIN>: 1.00058716855941451395
coin_y<0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN>: 0.99941317600518515135
```
and
```
coin_x<0xcf72ec52c0f8ddead746252481fb44ff6e8485a39b803825bde6b00d77cdb0bb::coin::COIN>: 1.000044721045697514
coin_y<0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN>: 0.99995528095418497746
```

### Simple Arb with `UnGraphMap` vs `DiGraphMap`
For `UnGraphMap`, since we have undirected edges, simple arbs (single edge) will not appear as cycles. So we will need to find the simple arbs separate of all the other cycles.

For `DiGraphMap`, a simple cycle discovery algorithm should be able to find simple arbs, too.

I think a that a modification of `all_simple_paths` could possibly be used.

We could even do something like:
- finding all adjacent nodes to our origin node
- for each of the above nodes find all simple paths to them
- simply add our origin node to the end of all of the given paths

But lets see if we can modify `all_simple_paths` (would likely be more efficient).
- I'm guessing cycles are disqualified do to the origin vertex being in the visited set.

### Turbos
https://suiexplorer.com/object/0x91bfbc386a41afcfd9b2533058d7e915a1d3829089cc268ff4333d54d6339ca1?module=pool_factory

We have a `PoolCreatedEvent`  in `pool_factory`:
```
struct PoolCreatedEvent has copy, drop {
	account: address,
	pool: ID,
	fee: u32,
	tick_spacing: u32,
	fee_protocol: u32,
	sqrt_price: u128
}
```

We also have this to work with:
```
struct PoolConfig has store, key {
	id: UID,
	fee_map: VecMap<String>
	fee_protocol: u32,
	pools: Table<ID, PoolSimpleInfo>
}
```
^ This should give us something a bit more reliable than querying `PoolCreatedEvent`s.

### Tight rates at end of block
These imply that people are aiming to be the last transaction in a block, arbing away the opportunity left by the second to last transaction.

### Now we want volume and profit.
Instinct says binary search.
We'll need a `get_amount_out()` and a `get_amount_in` function.
Though `get_amount_out()` is better is suppose for optimizing amounts in.

Ahh the benefits of storing `sqrt_price` is that we use it in calculation and multiplying to get price is much faster than getting the sqrt.

### It's a good thing I made updating the elements of the graph independent from our web connections.
This way I can also push delta updates and place myself in between transactions. Though then we'll have to be VERY fast.

### Hmm birectional cycles
Looks like we're not getting cycles in both directions??
AH the 2 hop ones are identical in both directions so havin another one would be duplication HAHAHAHAH.

### Needing decimal points
Our thing says that the rate for Sui to SuiShib is 772.
On turbos it says rate is 764726.

Sui has 9 decimals.
Turbos has 6 decimals.

We can shift 3 the decimal point for ours 3 right. Which is how many fewer SuiShib has than Sui.

Is this difference important? I don't think so. More so for exchanges and sites quoting for human friendliness.

### tick
Cetus:
```
struct TickManager {
	tick_spacing: u32,
	ticks: SkipList<Tick>
}

struct Tick has copy, drop, store {
	index: I32,
	sqrt_price: u128,
	liquidity_net: I128,
	liquidity_gross: u128,
	fee_growth_outside_a: u128,
	fee_growth_outside_b: u128,
	points_growth_outside: u128,
	rewards_growth_outside: vector<u128>
}
```

Turbos:
```
struct Tick has store, key {
	id: UID,
	liquidity_gross: u128,
	liquidity_net: I128,
	fee_growth_outside_a: u128,
	fee_growth_outside_b: u128,
	reward_growths_outside: vector<u128>,
	initialized: bool
}
```

### pool
cetus:
```
struct Pool<phantom Ty0, phantom Ty1> has store, key {
	id: UID,
	coin_a: Balance<Ty0>,
	coin_b: Balance<Ty1>,
	tick_spacing: u32,
	fee_rate: u64,
	liquidity: u128,
	current_sqrt_price: u128,
	current_tick_index: I32,
	fee_growth_global_a: u128,
	fee_growth_global_b: u128,
	fee_protocol_coin_a: u64,
	fee_protocol_coin_b: u64,
	tick_manager: TickManager,
	rewarder_manager: RewarderManager,
	position_manager: PositionManager,
	is_pause: bool,
	index: u64,
	url: String
}
```

turbos:
```
struct Pool<phantom Ty0, phantom Ty1, phantom Ty2> has store, key {
	id: UID,
	coin_a: Balance<Ty0>,
	coin_b: Balance<Ty1>,
	protocol_fees_a: u64,
	protocol_fees_b: u64,
	sqrt_price: u128,
	tick_current_index: I32,
	tick_spacing: u32,
	max_liquidity_per_tick: u128,
	fee: u32,
	fee_protocol: u32,
	unlocked: bool,
	fee_growth_global_a: u128,
	fee_growth_global_b: u128,
	liquidity: u128,
	tick_map: Table<I32, u256>,
	deploy_time_ms: u64,
	reward_infos: vector<PoolRewardInfo>,
	reward_last_updated_time_ms: u64
}
```

### swap
cetus:
```
struct CalculatedSwapResult has copy, drop, store {
	amount_in: u64,
	amount_out: u64,
	fee_amount: u64,
	fee_rate: u64,
	after_sqrt_price: u128,
	is_exceed: bool,
	step_results: vector<SwapStepResult>
}

struct SwapStepResult has copy, drop, store {
	current_sqrt_price: u128,
	target_sqrt_price: u128,
	current_liquidity: u128,
	amount_in: u64,
	amount_out: u64,
	fee_amount: u64,
	remainder_amount: u64
}

```

turbos:
```
struct ComputeSwapState has copy, drop {
	amount_a: u128,
	amount_b: u128,
	amount_specified_remaining: u128,
	amount_calculated: u128,
	sqrt_price: u128,
	tick_current_index: I32,
	fee_growth_global: u128,
	protocol_fee: u128,
	liquidity: u128,
	fee_amount: u128
}
```

### Cetus

```
struct TickManager {
	tick_spacing: u32,
	ticks: SkipList<Tick>
}

struct Tick has copy, drop, store {
	index: I32,
	sqrt_price: u128,
	liquidity_net: I128,
	liquidity_gross: u128,
	fee_growth_outside_a: u128,
	fee_growth_outside_b: u128,
	points_growth_outside: u128,
	rewards_growth_outside: vector<u128>
}

struct Pool<phantom Ty0, phantom Ty1> has store, key {
	id: UID,
	coin_a: Balance<Ty0>,
	coin_b: Balance<Ty1>,
	tick_spacing: u32,
	fee_rate: u64,
	liquidity: u128,
	current_sqrt_price: u128,
	current_tick_index: I32,
	fee_growth_global_a: u128,
	fee_growth_global_b: u128,
	fee_protocol_coin_a: u64,
	fee_protocol_coin_b: u64,
	tick_manager: TickManager,
	rewarder_manager: RewarderManager,
	position_manager: PositionManager,
	is_pause: bool,
	index: u64,
	url: String
}

struct SwapResult has copy, drop {
	amount_in: u64,
	amount_out: u64,
	fee_amount: u64,
	ref_fee_amount: u64,
	steps: u64
}

struct CalculatedSwapResult has copy, drop, store {
	amount_in: u64,
	amount_out: u64,
	fee_amount: u64,
	fee_rate: u64,
	after_sqrt_price: u128,
	is_exceed: bool,
	step_results: vector<SwapStepResult>
}

struct SwapStepResult has copy, drop, store {
	current_sqrt_price: u128,
	target_sqrt_price: u128,
	current_liquidity: u128,
	amount_in: u64,
	amount_out: u64,
	fee_amount: u64,
	remainder_amount: u64
}
```

### Turbos

```
struct Tick has store, key {
	id: UID,
	liquidity_gross: u128,
	liquidity_net: I128,
	fee_growth_outside_a: u128,
	fee_growth_outside_b: u128,
	reward_growths_outside: vector<u128>,
	initialized: bool
}

struct Pool<phantom Ty0, phantom Ty1, phantom Ty2> has store, key {
	id: UID,
	coin_a: Balance<Ty0>,
	coin_b: Balance<Ty1>,
	protocol_fees_a: u64,
	protocol_fees_b: u64,
	sqrt_price: u128,
	tick_current_index: I32,
	tick_spacing: u32,
	max_liquidity_per_tick: u128,
	fee: u32,
	fee_protocol: u32,
	unlocked: bool,
	fee_growth_global_a: u128,
	fee_growth_global_b: u128,
	liquidity: u128,
	tick_map: Table<I32, u256>,
	deploy_time_ms: u64,
	reward_infos: vector<PoolRewardInfo>,
	reward_last_updated_time_ms: u64
}

struct ComputeSwapState has copy, drop {
	amount_a: u128,
	amount_b: u128,
	amount_specified_remaining: u128,
	amount_calculated: u128,
	sqrt_price: u128,
	tick_current_index: I32,
	fee_growth_global: u128,
	protocol_fee: u128,
	liquidity: u128,
	fee_amount: u128
}
```


### Tick spacing

Cetus on the other hand, likely wouldn't. Seems like `SkipList.next()` returns `Option`.
`SkipList.remove()` is called in `decrease_liquidity()`.

Hmm perhaps we'll need two diff impls AGHFDHGJ.

### TickBitMap
Turbos implements a version of this.

Consists of a mapping of `u16` to `u256`. One `u256` is a word, a 256 bit array. The mapping as a whole is utilized as a very big array. We can index into this array. The bits of the array represent ticks and whether they are initialized.

There is a function to get a tick (combination of `u16` and index into 256 but array) given a sqrt price.
Tick is represented by `u24` basically `u16` and the 8 bits for the index into the 256 bit array (2^8).

SEPARATE from this, there exists a mapping of `u24` to tick info.

Turbos' tick bit map:
```
tick_map: Table<I32, u256>,
```

There's no mapping of index to `Tick`. Seems like there's something to do with dynamic fields (works in place of a map).

```
Call dynamic_field::borrow_mut<I32, Tick>(&mut UID, I32): &mut Tick

public fun borrow<Name: copy + drop + store, Value: store>(
    object: &UID,
    name: Name,
): &Value;
```

The `UID` is the pool `UID`.
Seems like we can use `get_dynamic_fields` to get `Tick`s for Turbos.

`BTreeMap` should accommodate both Turbos and Cetus is my intuition.
Its like if we combined the functionality of the tick bit map and dynamic fields (can't do ordered lookup with dynamic fields as givens).

### Swap
What do we do if we run out of liquidity? 

>"The current price moves during swapping. It moves from one price range to another, but it must always stay within a price range–otherwise, trading is not possible.
>
>Of course, price ranges can overlap, so, in practice, the transition between price ranges is seamless. And it’s not possible to hop over a gap–a swap would be completed partially. It’s also worth noting that, in the areas where price ranges overlap, price moves slower. This is due to the fact that supply is higher in such areas and the effect of demand is lower (recall from the introduction that high demand with low supply increases the price)."

We would just complete the swap partially. 

> "What happens when the current price range gets depleted during a trade? The price slips into the next price range. If the next price range doesn’t exist, the trade ends up fulfilled partially-we’ll see how this works later in the book."

```
// continue swapping as long as we haven't used the entire input/output and haven't reached the price limit
while (state.amountSpecifiedRemaining != 0 && state.sqrtPriceX96 != sqrtPriceLimitX96) {
	
	(state.sqrtPriceX96, step.amountIn, step.amountOut, step.feeAmount) = SwapMath.computeSwapStep(
	state.sqrtPriceX96,
	(zeroForOne ? step.sqrtPriceNextX96 < sqrtPriceLimitX96 : step.sqrtPriceNextX96 > sqrtPriceLimitX96)
		? sqrtPriceLimitX96
		: step.sqrtPriceNextX96,
	state.liquidity,
	state.amountSpecifiedRemaining,
	fee
);
	
	...
	
	// shift tick if we reached the next price
    if (state.sqrtPriceX96 == step.sqrtPriceNextX96) {
	    ...
    } else if (state.sqrtPriceX96 != step.sqrtPriceStartX96) {
		// recompute unless we're on a lower tick boundary (i.e. already transitioned ticks), and haven't moved
		state.tick = TickMath.getTickAtSqrtRatio(state.sqrtPriceX96);
	}

}
```

> "In the first case, the swap is done entirely within the range–this is the scenario we have implemented. In the second situation, we’ll consume the whole liquidity provided by the range and **will move to the next range** (if it exists)." referring to `ComputeSwapStep`

This part of `computeSwapStep`:
```
amountIn = zeroForOne ? SqrtPriceMath.getAmount0Delta(sqrtRatioTargetX96, sqrtRatioCurrentX96, liquidity, true) : SqrtPriceMath.getAmount1Delta(sqrtRatioCurrentX96, sqrtRatioTargetX96, liquidity, true);

    if (amountRemainingLessFee >= amountIn) sqrtRatioNextX96 = sqrtRatioTargetX96;
```

If the amount to be swapped in (calculated given target price, current price, and liquidity) is greater than the fee that is to be charged on the amount remaining to be swapped, then the next price will simply be set to the target price.

We passed in the price limit (default to max) as the target price. 

This should lead us to exiting the loop.

### The importance of precision
If we round in the wrong direction when resolving sqrt price to tick we'll get a completely different result regarding the liquidity available. Depends on tick.

Maybe we can try uh running it through the VM?

FACKKKK. stress. stress.

Ok I think for turbos we can assume a very very loyal adaptation of Uniswap.
Let go with that for now (skull emoji).

### muldiv and co.
muldiv prevents overflow on multiplication (I get the feeling we'll run into this way more often than we'd like)

How mul div?
```
public fun mul_div_floor(num1: u128, num2: u128, denom: u128): u128 {
	let r = full_mul(num1, num2) / (denom as u256);
	(r as u128)
}

public fun full_mul(num1: u128, num2: u128): u256 {
	(num1 as u256) * (num2 as u256)
}

public fun mul_div_ceil(num1: u128, num2: u128, denom: u128): u128 {
	let r = (full_mul(num1, num2) + ((denom as u256) - 1)) / (denom as u256);
	(r as u128)
}
```

Just use a larger type while calculating lmfao fucking bull shit HAHAHA.

### Loading info
We don't want to fetch all the positions too (also dynamic fields).
Thats just too f-in much.

Maybe there's someway of grabbing the I32 keys of the table and then the 

### Lack of precision
We don't get one when multiplying the price of x and the price of y on the same exchange. (Obviously, U64F64 isn't the most precise number format). The difference negligible though. We should allow and account for some margin of error.

It seems that we lose some more precision as we look for profitable paths by multiplying prices (sqrt_price^2).

```
sq then mult: 0.99999999999999999984
mult then sq: 0.9999999999999999999
sq then mult: 0.99999999999999999984
mult then sq: 0.9999999999999999999
sq then mult: 0.9999999999999999532
mult then sq: 0.9999999999999999999
sq then mult: 0.9999999999999999532
mult then sq: 0.9999999999999999999
sq then mult: 0.99999999999999974874
mult then sq: 0.9999999999999999999
sq then mult: 0.99999999999999974874
mult then sq: 0.9999999999999999999
sq then mult: 0.99999999999999999984
mult then sq: 0.9999999999999999999
sq then mult: 0.99999999999999999984
mult then sq: 0.9999999999999999999
sq then mult: 0.99999999999999997756
mult then sq: 0.9999999999999999999
sq then mult: 0.99999999999999997756
```

It seems that we lose much more precision when we square our sqrt prices first then multiply.
It would be strange though to offer a `sqrt_price()` function considering how different our exchanges may be (some may not hold `sqrt_price` by default)....

Perhaps we can offer some sort of lazy evaluation...?

We'll pass on this for now.

### Markets in edges should know who their parents are.
They need to reference their parents for their package id among other things.

### Computing pools
We can't just pass things through a simple function to compute. We need some sort of object to hold all the pool and tick data from the chain.

We'll call this a computing pool.

How should we manage computing pools?
Where should we store them?

Let's think of the relationships and semantics behind computing pools and some of the other things we've done.

One thing I'm unsure about is letting exchange specific functions take in a slice of an array of broader `Market` trait objects.

For example:
```
async fn get_pool_id_to_object_response(&self, sui_client: &SuiClient, markets: &[Box<dyn Market>]) -> Result<HashMap<ObjectID, SuiObjectResponse>, anyhow::Error> {
	
	let pool_ids = markets
		.iter()
		.map(|market| {
			*market.pool_id()
		})
		.collect::<Vec<ObjectID>>();
		
	sui_sdk_utils::get_object_id_to_object_response(sui_client, &pool_ids).await
}
```

The instantiation of the computing pool is tied to the parent exchange.

We implemented `update_with_object_response()` so we could apply changes to markets using market information we fetched async in batches. To work with batch requests.

Computing `Pool` information is also fetched based off of the above same information fetched in batches, except it makes its own requests based on that information (stuff like the pool fields and the address of the pool's dynamic fields as well as the address of the tick map's dynamic fields).

### `add_delta` fails and screws up the whole compute swap result operation
`add_delta` is designed to revert on overflow or underflow.
liquidity + delta should bottom out at 0. The absolute value of delta should be no larger than liquidity.

The problem arises from the folllowing:
```
if a_to_b {
	liquidity_net = -liquidity_net;
}

compute_swap_state.liquidity = math_liquidity::add_delta(
	compute_swap_state.liquidity,
	liquidity_net
);
```

We may not be calculating `liquidity_net` correctly on instantiation. Or perhaps not loading it correctly?

Perhaps is cause we're not updating the ticks in swap?

### `liquidity_net`
For a lower tick it represents the amount of liquidity to add.
For an upper tick is represents the amount of liquidity to remove.

Ahh for our initial test we only have one price range so we're not running any tick transitions.

Are the liquidity deltas not being properly applied????
Are we improperly grabbing the next tick?

Ok it seems like we have an issue with crossing uninitialized ticks. This ain't right (!!!!!) as `cross_tick` should ONLY be called if the next tick is initialized.

Seems like we have to vet `next_initialized_tick`.

Maybe the tick is initialized but the tick we're returning from `next_initialized_tick` is the wrong tick??

Think HARDER about how to find out what is wrong with this thing.

Wait a fucking second:
```
branch 1
next_initialized_tick() next_tick (word_pos, bit_pos): (-5, 232)
  next tick (-75480) initialized: true
    cross_tick() tick_next_index (word_pos, bit_pos): (-5, 234). spacing: 60)
    crossing uninitialized tick
```

We're differing on position???

The results we're seeing with running into multiple uninitialized ticks in a row from`next_initialized_tick` is not a problem. 

> When there’s no initialized tick in the current word, we’ll continue searching in an adjacent word in the next loop cycle.

WHAT IF ITS CAUSE WE DIDN'T LOAD ALL THE TICKS?!?!?!?!??! OH THE TERROR OH THE TERRRRORRR.

It would explain all of our issues.

Though to be sure we should test that `next_initialized_tick` works properly. We would be missing a LOT of ticks....

OKOK it seems that in between receiving ticks the tick and macking our map, we're dropping some ticks??

check if the tick is contained in the map and if it has a corresponding word in tick_map? agh but we'll make the tick map by default arghh.

We're getting both issues. 
- `next_initialized_tick` saying that a tick is initialized (based on the tick map) when it isn't among the initialized ticks
- `next_initialized_tick` saying that a tick isn't initialized (based on the tick map) when it is among the initialized ticks
	- This is the weirder one tbh... We don't cross ticks in this case...
	- Why is this happening??!!
	- Seems like it could be that a tick was inserted but not removed. It may still yet be uninitialized (the field).
```
next_initialized_tick() next_tick (word_pos, bit_pos): (-8, 240)
ticks contains 'uninitialized' tick -121020. initialized field: true
```

Ok. That not the fucking case lmao. False for some. True for some?

Seems like we're loading all the tick maps properly...
Not sure how to account for the inconsistency... Perhaps the tick being returned by next initialized tick is wrong (doesn't seems like its the case?)

Issue may be with loading the initialized ticks....

Things we can rule out as SOLE culprits of `initialized` inconsistency:
- Dropping ticks as we load them
	- There are cases where `next_initialized_tick` returns a tick and says it is uninitialized when in truth it is among the ticks as initialized + has complete fields.
	- If the issue was only dropping ticks then we would only get `next_initialized_tick` returning `initialized = true` while it is missing from ticks as it has not been loaded.
- One of the branches of `next_initialized_tick`
	- we get inconsistency with both branches so that can't be the case

The hardest to assess source of failure would be fetching and loading a tick map and ticks that reflect different states. 

OK THERE IS AN ISSUE
It seems that on SOME numbers it is properly stating that the tick is initialized but the tick it is returning is NOT the tick that should be returned.

Perhaps, it's an issue with insertion BUT the problems with the on-chain version makes me think this is not unique...

Everything is the same per iteration of the test BESIDES `word` from:
```
let word = pool.tick_map.entry(word_pos).or_insert(U256::from(0_u8));
```
and everything depending on it.

`mask` is consistent, too.
```
let mask = (U256::from(1_u8) << bit_pos) - 1 + (U256::from(1_u8) << bit_pos);
```

`masked`, `initialize`, the control flow dependent on initialized, and the ouput for `initialized = ture` however, are dependent on `word`.
```
let masked = *word & mask;
```

Intuitively, the greatest source of inconsistency looks like `math_bit::most_significant_bit`.

OK patching up the `most_significant_bit` functions seems to have fixed our issue in TESTING.
We're still getting the same errors in live runs ;-;

Ok so we've resolved the off by one issue with the returned tick_index (to some degree). But we're still getting missing from tick map?????

Okie i think it might be an issue with the semantics of bit shifting!!

OH WAIT COULD IT HAVE TO DO WITH ENDIANNESS?????

### sanity
start at the simplest sanity checks.
Are the number of initialized ticks the same as in the tick maps?

OK THERE IS THE SAME NUMBER. OK IT DOES SEEM LIKE A RESOLUTION ISSUE.
