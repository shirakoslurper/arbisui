
I don't think I'm entirely understanding how this is working.

```
struct SkipList<phantom Ty0: store> has store, key {
	id: UID,
	head: vector<OptionU64>,
	tail: OptionU64,
	level: u64,
	max_level: u64,
	list_p: u64,
	size: u64,
	random: Random
}

struct Node<Ty0: store> has store {
	score: u64,
	nexts: vector<OptionU64>,
	prev: OptionU64,
	value: Ty0
}

struct Item has drop, store {
	n: u64,
	score: u64,
	finded: OptionU64
}
```


### Figuring out indices and scores and what they all mean

One thing to note is that `score` is a field of `Node` and `Item` (which does not seem to be used).

Okie let's start somewhere a bit simpler. With `borrow_node(skip_list: &SkipList, x: u64)`:
- It's seems that nodes of a `SkipList` are stored as dynamic fields associated with the `SkipList`'s `ObjectID`.
- There is also `x` which identifies the unique node. Some type of index.
- In `swap_in_pool` we use the first "score" to call `tick_borrow_tick_for_swap()` which in turn calls `skip_list::borrow_node()` and `skip_list::prev_score` and `skip_list::next_score`.

`next_score(node: &Node<T>)`:
- returns the first element of the `node.nexts` vector of `Option<u64>`s.
- The next element at the lowest level.

> The length of nexts is probably equivalent to the level of the node + 1.

`next_score(node: &Node<T>)`:
- returns the score at `next.prev`

I think that the `SkipList` impl for Cetus may differ from the Rust lib's version in that it does not require that an "index" be less than or equal to the length of the existing `SkipList`.

The Cetus equivalent of that check is the `contains()` function that checks if the `SkipList` contains a node with that score. The call it makes to check is `dynamic_field::exists_with_type<u64, Node<Ty0>>(&UID, u64): bool`. There is no length check.

I'm thinking we should opt for a DS that uses a`SkipList` that is more of a map, where the indices do not have to be sequential. Actually, huh, a `SkipMap` is a bit of an alternative to a `BTreeMap`. We can rely on a `BTreeMap` for ordering.... They serve the same purpose of providing unique, sorted keys. And it shouldn't be a stretch to fetch all the node of the `SkipList` and insert them in the `BTreeMap` especially given that the node's score is stored in the node itself (perhaps for redundancy but I am thankful).

We can benchmark to choose between `BTreeMap` and `SkipMap`. The latter is useful when we need concurrent writes without mutual exclusion. I don't think we're really gonna be doing much writing after loading all the nodes into the `BTreeMap`. All we need to insert and lookup is score and value.

Hmm. Our ticks range from negative to positive (`i32`) but score is strictly positive (`u32`). We apply the same addition of tick bound to both negative and positive tick indexes. We've just shifted the range into the positives.

### Testing swap in pool
We're not getting the right ticks. We have to drop using score (and use index instead) since we moved from a `SkipList` to `BTreeMap`. 

Ok seems like our problem comes from the difference in `current_sqrt_price` and `target_sqrt_price` we are giving to our "compute_swap_step" functions. This trickles down into the delta calculations which in turn effect what we get for `amount_in` and `amount_out`.

But the expected post swap tick is closer than the target tick so i don't think it's an issue with `current_sqrt_price` and `target_sqrt_price`. Our turbos version is simple hitting the 2 word boundary limit before discovering the next tick.

Ahh the issue seems to be with `get_delta_down`... since that is what give us our `amount_out`.

Oh beside wrong numerator denominator stuff there seems to be an issue with casting U256 to U64. It completely changes what the number is.


### Activity
https://suiexplorer.com/object/0xce7bceef26d3ad1f6d9b6f13a953f053e6ed3ca77907516481ce99ae8e588f2b

Gets a lot of activity but the library doesn't?
OK, 0x1ea is the right address. But it's a library. It's not often called directly. 

Pools seem to be deployed to their own addresses?
https://suiexplorer.com/object/0x2e041f3fd93646dcc877f783c1f2b7fa62d30271bdef1f21ef002cebf857bded
Although pools are technically objects..
Ahhh nvm I get it. We grab resource and pools and updates etc. etc. emits help us catch all the uses of the libraries we miss on the explorer etc. etc.


The cast to u64.... It disturbs me...

Is there an implicit limit on `amount_in` and `amount_out` I'm missing? 

Is this unsafe? In terms of `amount_out` it might give us less than we expected...
Wherever we cast to u64 we could get some undesired behavior...

Hmm implicit boundaries make no sense? fack. Ok. I'll just build it one-to-one while chekcing that the intermediate non-casted values are correct.

For U64 to be a sensible type to use: 
- $L(P_u - P_l) \le$ `u64::MAX` must hold and
- $\frac{L(P_u - P_l)}{P_u * P_l} \le$ `u64::MAX` must hold
The top can be determined by the maximum possible `liquidity` amount times the range of `sqrt_price`. 

### Rate limit exceeded
when requesting coin metadata but thats for human readability only.

also when fetching tick for the pools. each pool has its own chunked requests but if the number of ticks per pool is significantly lower than 50, then we are wasting requests.

we should implement a way to join requests for ticks across pools.

hmm but we also need to find a workaround for running into rate limits. we should prevent them.

We want the following qualities. 
- keeps a timer? limits calls to 50 per second. otherwise we shift calls to the next 
- for path dependent calls, calls of similar stages are batched (where they can be batched)
	- `multi_get_objects()`
	- `multi_get_transaction_blocks()`
	- `try_multi_get_past_objects()`


> This means we need a pretty big redesign. 


### `GlobalConfig`
How do we know which `GlobalConfig` object we are using?

## "next_initialized_tick" being `None`
Cetus aborts if the tick is none (in the iteration after the tick is fetched). How do we prevent this?

I'm guessing that our code is supposed to prevent us from reaching the next iteration.

Which means we should run into one of the two terminations conditions:
- `amount_remaining > 0`
- `compute_swap_state.current_sqrt_price != sqrt_price_limit`

Oh is it a liquidity issue? Can't swap in a 0 liquidity pool but I think we do that check earlier?

Ah, ok let's try filtering by liquidity!.

Ah we didnt consider running out of liquidity mid swap - having it initially and running out later.

Put in a hacky solution that allows fro partial success swaps though we should reconsider....

9200313606762638866
10431100134944025

## request level batching

## filtering
Should be done pre-update.

It the updating process that makes the request...
We can't filter for liquidity before the first update....
Best is to get around the rate limit?

Yeah because it might end up being a problem regardless.

Perhaps i can get around the rate limit by making another account...
lmaool.

Retrying might be the fastest reactive method...
Requires the least in terms of what we know about the endpoint.

Ahh a rate limiting library lmwo.

Call counter + delay but seems somewhat not so efficient. 

Semaphore seems to be the play:
https://github.com/encode/httpx/issues/815

Requires writing a custom transport... AGASHFHSD

https://news.ycombinator.com/item?id=8761919

How would we write such a wrapper? What would require the least rewriting?

We could wrap it in a struct that returns the client if it is available and blocks otherwise.
It would be hard to get the number of calls though.
Aghh

The biggest issue is call counting innit. We have to do a fulll wrap to call count correctly....?

What about counting the number of created futures?
It might be best to just rewrite the sui sdk's client - we're using a custom one anyways with paging. Can't properly count calls without the inner representation.

I feel like `RateLimiter` (held by the sui-sdk client) should be a trait and *not* a struct. This would allow for all different types of rate limiting shenanigans (say if the server implementation changes...).

But for now lets stick with a struct - `RequestFrequencyRateLimiter`.
This should use a timer of sorts + a counter (thinking modular addition).

Hmm the sui-sdk client would be the highest place we can put the counter - we can properly count the calls + we don't need to go any lower.

We make the calls from the individual api structs:
- `ReadApi`
- `CoinReadApi`
- `EventApi`
- `QuorumDriverApi`
- `GovernanceApi`

Ok so these APIs are stored as fields of sui client:
```
pub struct SuiClient {
	api: Arc<RpcClient>,
	transaction_builder: TransactionBuilder,
	read_api: Arc<ReadApi>,
	coin_read_api: CoinReadApi,
	event_api: EventApi,
	quorum_driver_api: QuorumDriverApi,
	governance_api: GovernanceApi,
}
```

If we can wrap `Arc<RpcClient>` then we greatly reduce the work we need to do + less likely to introduce errors.

However it is the `http` field of `RpcClient` on which functions are called to make requests so it would be somewhat incorrect.

Best to increment a counter for every function for every API.

The governor crate seems to be the most popular.
https://en.wikipedia.org/wiki/Generic_cell_rate_algorithm

> Reminder: we can't have self referential fields in Rust.

So the rate limiter will have to live outside of `SuiClient`.

Two idiomatic solutions:
- Store a reference to the `RateLimiter`
- Pass the `RateLimiter` every time we call a function that makes a request.
	- Eh: No canonical rate limiter.

The API structs have no notion of parents. But they are built in the same process that creates `SuiClient`. `SuiClient` doesn't need to store the rate limiter.

Hmm ok we got an issue. `TransactionBuilder` takes `ReadApi` when it is created. Unless we rewrite `TransactionBuilder` to take a `DataReader` with a lifetime....
We also can't pass in the rate limiter with the `DataReader` trait's functions 

But passing it in the function calls makes it a humongous pain...
Shared ownership? Can't really safely do shared ownership with mutability (acquiring mutable references).

https://medium.com/swlh/shared-mutability-in-rust-part-1-of-3-21dc9803c623

It seems that there might be another option:
- Another non-counted API type just to satisfy DataReader
- Interior mutability - mutating a type while it has multiple aliases (runtime checking).

> `governor`’s rate limiter state is not hidden behind an [interior mutability](https://doc.rust-lang.org/book/ch15-05-interior-mutability.html) pattern, and so it is perfectly valid to have multiple references to a rate limiter in a program. Since its state lives in [`AtomicU64`](https://doc.rust-lang.org/nightly/core/sync/atomic/struct.AtomicU64.html "struct core::sync::atomic::AtomicU64") integers (which do not implement [`Clone`](https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html "trait core::clone::Clone")), the rate limiters themselves can not be cloned.

Ok we didn't need it. We just needed to wrap `RateLimiter` in `Arc` and clone the `Arc`.

This, to me, is a weird result:
```
SIMPLE CYCLE (5 HOP) 
5 HOP CYCLE RATE: 1.5344598628982296699
max_profit: -10, maximizing_orig_amount: 10
MAX_PROFIT: -10
```

Some of these weirder result come as a result of not exiting through a break. Perhaps similar to an earlier issue where different amount ins result in the same amount out due to small step changes.

Best result:
```
SIMPLE CYCLE (2 HOP) 
2 HOP CYCLE RATE: 253.7988625923488020166
max_profit: 128219142961, maximizing_orig_amount: 8457689357
MAX_PROFIT: 128219142961
```

### Updating markets outside of the graph or within the graph?
While nodes must be references or primitive types, our edges are owned by the graph.
So our update function is a method of the `MarketGraph`.

## arbitrage.rs

Qualities:
- We want to store cycles!!!
- Market graphs are updated at the beginning of every call to the big `search()` function.
- Subtracting gas from profits should be added in post.
- It should be as fast as possible. Parallelized where it can be.
- We should deal with the weird step change bug - where different input amounts result in the same outputs and we don't get a `break` to a maximum and instead end up terminating incorrectly with the wrong max profit and input to get max profit amounts.

Steps:
- Start with a `MarketGraph` - we should not be making a new one every search cycle.
- Start with all cycles up to a desired length in that `MarketGraph` - pre-searched and cached to save compute.
	- Make sure the cycles are filtered to include Sui
	- Shift paths to start with Sui
	- Actually we specify the start and end destination with our given pathfinding algo.
- Update all markets
- Iterate through all paths and for each path:
	- Perform a binary search over the possible inputs
	- Gets:
		- Path including coins and markets!
		- Optimal amount in (of original coin) and of the coins for each leg?
			- Generally, the more info we have the better though (in terms of optionality later).
		- Profit with the optimal amount in (since we are maximizing profit).
		- The end amount out?
		- Specifying the amounts for each leg could doom us.
			- If we spent less than we get out that is fine.
			- But if we spend more than we get out thats an issue.
			- Whether we need this depends on how well we can chain calls together in a transaction with out `TransactionBuilder`
				- If we can be like: use amount out from this leg as the amount in for the next leg, then we are good.
				- Otherwise it would be better to specify the amounts per leg.
	- We'll have to be very anal about the binary search esp wrt to step sizes.
		- Due to how liquidity is provided and the xy = k thing, the more we move price / the more of and asset we want the more we'll have to trade in (with an increasing rate)
		- Basically the amounts we have trade in to move price the same amount (and get some fixed amount out) increases the further we get from the original price.
		- How do we choose a step size?
			- Explore!
			- We can use a different application of Binary search that searches over step size
	- We have to select which market to trade on (for a given pair)
		- One will be more profitable than the other
		- OR we can calculate max profit for all markets?
			- We would need to create these paths first and then calculate max profits for each of them
			- We would have to build out these trees (trees are more memory efficient).
				- Also if we make this recursive we can rely less on representing paths with a DS.
				- Just branch logically at decisions.
			- Some crazy combinatorial stuff would result I'm guessing.
		- We should prune any market with a rate less than 1.0

Is there a known problem for path selection?
Hmm yeah, for a cleaner `get_amount_out()` function we'll want to expand paths.

## Optimization
Golden section search to avoid getting stuck in a local minimum/maximum (bisection search might).
Our function is unimodal (monotonically increasing/decreasing up to a point then monotonically decreasing/increasing after that point) (only one minimum or maximum in the given interval).

> A root-finding method is a method for finding zeros or continuous functions.

https://en.wikipedia.org/wiki/Golden-section_search

> Unlike finding a zero, where two function evaluations with opposite sign are sufficient to bracket a root, when searching for a minimum, three values are necessary.

## Creating a non-mutable version of the calculate swap result function

This way we don't need mutable references to `dyn Market` to call `compute_swap_x_to_y()` and `compute_swap_y_to_x()` since they require mutable pools.

## Dry run with programmable transactions
We will be using `ProgrammableTransactionBuilder`
We will want to write exchange specific swap calls for each exchange.

To prevent a crazy rewrite, I think we're gonna have to make a custom `TransactionBuilder` with builtin `ProgrammableTransaction` support. (`TransactionBuilder` is based off of `ProgrammableTransactionBuilder` but the latter lacks a lot of creature comforts.)

***Mixing argument types***
- Mixing transaction results and "hardcoded" stuff (receiver address, initial amount in, coin type arguments, `sqrt_price_limit`, etc.)  may be difficult.
- Might be best to do `SuiJsonValue` to `Argument` resolution separately from building the transaction and using the lower level `ProgrammableTransactionBuilder` instead??
- Nyah but for resolving `SuiJsonValue` we need access to an async `DataReader` (in our case `ReadAPI`). Maybe we can wrap `SuiJsonValue`s and `Argument`s in an enum. 
- The issue is that when we resolve `SuiJsonArg` we need to provide all the arguments, at least with the given implementation of `sui_json::resolve_move_function_args()` and its subroutine `sui_json::resolve_call_args`.

### `sui_json::resolve_move_function_args()`
We can modify this to take a `combined_args` vector of both `SuiJsonValue` and `Argument` (wrapped in an enum).

### `sui_json::resolve_call_args()`
We zip the json args with the parameter types, enumerate them, and call `resolve_call_arg` while providing the enumerated index (this is only for debugging purposes).

### `sui_json::resolve_call_arg()`
only really necessary for `SuiJsonValue`

Back in `sui_transaction_builder::resolve_and_checks_json_args()` we match the `ResolvedCallArgs` and wrap their contents in `CallArg`s before calling `input` and transforming them into `Arguments`. We basically want our initial `Argument` inputs to go through this process undisturbed.

This way we have an ordered list of args.

## Transaction Context
Seems like entry functions have an implicit transaction context that we don't need to include as an input.

> Note the third argument to the `transfer` function representing `TxContext` does not have to be specified explicitly - it is a required argument for all functions callable from Sui and is auto-injected by the platform at the point of a function call. 
> https://docs.sui.io/testnet/build/cli-client

Clock is at. 0x0..06
Deadline is when to finish executing the transaction by:
- Ex:
	- Deadline: 1,689,664,270,994
	- Clock time: 1,689,682,490,871

## Adding programmable move calls!
mostly done!
- `programmable_move_call()` 
	- returns a `Result<Argument>`
	- takes a mixed vector of `SuiJsonVal` and `Argument`s
	- we've also implemented the necessary subroutines and structs for these
- `finish_building_programmable_transaction()`
	- consumes the `ProgrammableTransactionBuilder` and returns usable `TransactionData` all signed and everything

we still need to make the other functions programmable
- `split_coins()`
- `merge_coins()`
- etc.
- this will allow use to use the outputs of the former functions
- functions like `transfer_object()` that wrap `ProgrammableTransactionBuilder`'s `transfer_object()` can just be set aside in favor of the lower level function called directly on `ProgrammableTransactionBuilder`.

we should probably separate out the programmable stuff in the code physically


## filtering
Ooh maybe I can filter out all the cycles that are single exchange since those seem to be competitive.

## Why turbos `swap_a_b()` takes a vector of the same `Coin` object
What exactly is a `Coin` object?
Seems that every `Coin` object represents the type of coin and has an amount associated with it. Like individual chunks of cash.

What do we do with the vector?
We call `pool::merge_coins()` with the vector of `Coins` as an argument and it gives us one consolidated `Coin` object.

It seems that we call `MakeMoveVec()` before calling `MoveCall` though we could probably make that vector client side?
The vector only need a single input.
***We can't make a vector client side that we can pass as an argument. At least not easily lmao.***
The `Coin` objects we put in the vector must be from other function calls.

In the end its so that we can feed it non-consolidated amounts of a coin. Perhaps this could be useful?

[Cetus's router's](https://suiexplorer.com/object/0x2eeaab737b37137b94bfa8f841f92e36a153641119da3456dec1926b9960d9be?module=pool_script) `swap_a2b()` also takes a vector of coins!

Hmm, the router functions do not return amounts... And Turbos `swap()` can only be accessed by a friend (router or the like)...

Seems like we'll have to take the balance before and after a swap but implementing that math in a programmable transaction could be a bit.....
Well we can stick with the amounts per leg we have available!

Adding the programmable transaction functionality still benefits us: we can still chain calls in a single transaction.

## Grabbing coins
We need to use get_coins function from the sdk.
```
pub async fn get_coins(
	&self,
	owner: SuiAddress,
	coin_type: Option<String>,
	cursor: Option<ObjectID>,
	limit: Option<usize>,
) -> SuiRpcResult<CoinPage> {
	Ok(self
		.api
		.http
		.get_coins(owner, coin_type, cursor, limit)
		.await?)
}

pub async fn select_coins(
	&self,
	address: SuiAddress,
	coin_type: Option<String>,
	amount: u128,
	exclude: Vec<ObjectID>,
) -> SuiRpcResult<Vec<Coin>> {
	let mut total = 0u128;
	let coins = self
		.get_coins_stream(address, coin_type)
		.filter(|coin: &Coin| future::ready(!exclude.contains(&coin.coin_object_id)))
		.take_while(|coin: &Coin| {
			let ready = future::ready(total < amount);
			total += coin.balance as u128;
			ready
		})
		.collect::<Vec<_>>()
		.await;

	if total < amount {
		return Err(Error::InsufficientFund { address, amount });
	}
	Ok(coins)
}

```

We'll need to to this every step since the entry functions do not return a `Coin<Object>`. Huh but that `Coin` object we own might not exist...

We'll need an on-chain way of getting an owned coin object given the coin type...

Can we transfer an object that does not exist? Yes! Split coins creates an object! (In the example where we split coins and transfer them).
->
What about if the object is created by a function and transferred to you but the function returns nothing?
->
We'll need an on-chain way of getting an owned coin object given the coin type...

Per [Chapter 2 - Using Objects](https://docs.sui.io/build/programming-with-objects/ch2-using-objects) we can use `take_from_sender<T>` to take an object of type `<T>` from global storage. We should look into whether coin has periphery modules that support taking all the coins from a sender + taking all the coins until they meet a certain amount.
> ***Those are test only functions***

Ok so entry functions are only callable from transactions... but a programmable move call can return a value? Does that mean a `programmable_move_call()` can call public (non entry) function?

Hmm perhaps some clever things with flash swapping!!!.... Look at how a_b_c chains calls.. this way we only need the origin coin....

## Mergin coins

`mergeCoin()` in the typescript sdk merges the source coin into the destination coin....

A programmable merge coins is still better wince we don't have to merge coins with two separate calls... But if we're being hacky we can do separate calls anyways HAHAHA.

Also merge coins returns nothing so we'll keep that in mind for the merge coins method we're using. It uses [`Coin::join()`](https://github.com/MystenLabs/sui/blob/5c596d1734607db32e4ec516f6554b82ec67c497/crates/sui-framework/packages/sui-framework/sources/coin.move) on chain which returns nothing. 

```
    /// Consume the coin `c` and add its value to `self`.
    /// Aborts if `c.value + self.value > U64_MAX`
    public entry fun join<T>(self: &mut Coin<T>, c: Coin<T>) {
        let Coin { id, balance } = c;
        object::delete(id);
        balance::join(&mut self.balance, balance);
    }
```

```
pub async fn programmable_merge_coins(
	&self,
	builder: &mut ProgrammableTransactionBuilder,
	primary_coin: ProgrammableMergeCoinsArg,
	coin_to_merge: ProgrammableMergeCoinsArg,
	coin_type: SuiTypeTag,
) -> anyhow::Result<()> {
```

I'm guessing `coin_to_merge` and `c` are one and the same and that the other coin argument is the destination coin.


### the occasional panic during swap calculation
seems to happen on cetus since we filter for only cetus markets and got it
```
thread '<unnamed>' panicked at 'assertion failed: liquidity >= abs_directional_liquidity_net', arb-bot/src/cetus_pool.rs:493:13
```

**Testing whether its a loading or calculation issue:**
If we add the `net_liquidity` of all the ticks and it is not 0 and we panic, it is a loading issue. Otherwise it is a calculation issue.

Ok it seems like a calculation issue.

Ok it seems to occur when all net liquidity is zero...



8386636170

-8408939429

942562385189

942562385189

### Returning sqrt price so we don't get slipped disgustingly

This should be a priority

## Shared object
Works like a global for a module.

## Saving requests and speeding things up.
We'll monitor events and only update the pools which are involved.

So first we'll update all pools to give .
We'll update pools by ID given the events!
And then we'll only compute profitability for the cycles a pool is a part of!

## Transaction execution flow
We'll compute amount out every step based on how much of a coin we get to use as amount in. (This will be slow but it will generally be better for making sure every leg of the trade succeeds).
We'll send a dry run transaction to estimate gas.
We'll send a real transaction with update gas (roughly all the gas fields added omitting rebate).
We'll want to have a gas object with enough gas.

```
RESULT: DryRunTransactionBlockResponse {
    effects: V1(
        SuiTransactionBlockEffectsV1 {
            status: Success,
            executed_epoch: 101,
            gas_used: GasCostSummary {
                computation_cost: 800000,
                storage_cost: 15306400,
                storage_rebate: 14822280,
                non_refundable_storage_fee: 149720,
            },
            ...
```

```
balance_changes: [
        BalanceChange {
            owner: AddressOwner(
                0x02a212de6a9dfa3a69e22387acfbafbb1a9e591bd9d636e7895dcfc8de05f331,
            ),
            coin_type: Struct(
                StructTag {
                    address: 0000000000000000000000000000000000000000000000000000000000000002,
                    module: Identifier(
                        "sui",
                    ),
                    name: Identifier(
                        "SUI",
                    ),
                    type_params: [],
                },
            ),
            amount: -8408939429,
        },
```

## Events based
This would allows us to also get a better sense of how much total opportunity there is a day as it only calculates the opportunities as changes happen? idk idk.

Apply update to specific market in the `MarketGraph`.

CETUS:
- `OpenPositionEvent`
- `ClosePositionEvent`
- `AddliquidityEvent`
- `RemoveLiquidityEvent`
- `SwapEvent`
- `UpdateFeeRateEvent`

TURBOS:
- `SwapEvent`
- `MintEvent`
- `BurnEvent`
- `TogglePoolStatusEvent`
- `UpdateFeeProtocolEvent`

Each exchange should have a functions that creates event filters.

## Passing exchanges to loop blocks

Why we can't have a Vector of References to trait objects
> You misunderstand references, I'm 99% sure. Common mistake for newcomers from tracing GC-based languages is to assume that reference is something that points to heap-allocated object, GC-controlled object.
> But in Rust (like in C++) references are temporary objects which are used to access objects **owned by someone else**.
> Array of references is technically not invalid data structure but very rarely useful one.

Could somebody please explain why a value of type `Vec<&dyn Foo>` cannot be built from `Iterator<Item=&Bar>`?
> Because `&Bar` and `&dyn Foo` are different types. Sure, one can be converted into the other, but they _are still_ different types. You'll need to insert a conversion step.