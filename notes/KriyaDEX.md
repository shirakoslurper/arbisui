
```
struct Pool<phantom Ty0, phantom Ty1> has key {
	id: UID,
	token_y: Balance<Ty1>,
	token_x: Balance<Ty0>,
	lsp_supply: Supply<LSP<Ty0, Ty1>>,
	lsp_locked: Balance<LSP<Ty0, Ty1>>,
	lp_fee_percent: u64,
	protocol_fee_percent: u64,
	protocol_fee_x: Balance<Ty0>,
	protocol_fee_y: Balance<Ty1>,
	is_stable: bool,
	scaleX: u64,
	scaleY: u64,
	is_swap_enabled: bool,
	is_deposit_enabled: bool,
	is_withdraw_enabled: bool
}
```

```
struct PoolUpdatedEvent has copy, drop {
	pool_id: ID,
	lp_fee_percent: u64,
	protocol_fee_percent: u64,
	is_stable: bool,
	scaleX: u64,
	scaleY: u64
}

struct LiquidityAddedEvent has copy, drop {
	pool_id: ID,
	liquidity_provider: address,
	amount_x: u64,
	amount_y: u64,
	lsp_minted: u64
}

struct LiquidityRemovedEvent has copy, drop {
	pool_id: ID,
	liquidity_provider: address,
	amount_x: u64,
	amount_y: u64,
	lsp_burned: u64
}

struct SwapEvent<phantom Ty0> has copy, drop {
	pool_id: ID,
	user: address,
	reserve_x: u64,
	reserve_y: u64,
	amount_in: u64,
	amount_out: u64
}

struct ConfigUpdatedEvent has copy, drop {
	protocol_fee_percent_uc: u64,
	lp_fee_percent_uc: u64,
	protocol_fee_percent_stable: u64,
	lp_fee_percent_stable: u64,
	is_swap_enabled: bool,
	is_deposit_enabled: bool,
	is_withdraw_enabled: bool,
	admin: address
}
```

### Building transactions entirely client side
this could probably save us an insane amount of time in tick to trade

For kriya we should also be aware of the stable swap variant

flat_map skips results that are errs

The problem of most skipped pools while satisfying all cycles