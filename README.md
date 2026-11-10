# tari-ootle-faucet

A [Tari Ootle](https://github.com/tari-project/tari-ootle) (L2 smart-contract layer) WASM
template implementing `TariFaucet`: a component that dispenses the network's **real native
$TARI resource**.

## 1. What this is

This is a from-scratch Ootle WASM template, built the same way `tari create` would scaffold
one from [`tari-project/wasm-template`](https://github.com/tari-project/wasm-template), but
hand-written to a specific, deliberately simple spec: a faucet for the network's actual native
token.

The upstream repo ships a toy example template (`TestFaucet`, in
`tari-project/tari-ootle/integration_tests/src/templates/faucet`) that looks superficially
similar but works completely differently under the hood:

```rust
// Upstream toy example — NOT what this repo does.
pub fn mint(initial_supply: Amount) -> Component<Self> {
    let coins = ResourceBuilder::public_fungible()
        .with_token_symbol("FAUCET")
        .initial_supply(initial_supply);
    Component::new(Self { vault: Vault::from_bucket(coins) })
        .with_access_rules(AccessRules::allow_all())
        .create()
}
```

That `mint` function calls `ResourceBuilder::public_fungible()...initial_supply(...)`, which
**creates a brand-new fungible resource out of thin air** — a fake token called `"FAUCET"`
that has no relationship to real $TARI and is worthless outside of that one test template.

`TariFaucet` (this repo) never mints anything. Its constructor,
[`TariFaucet::new`](src/lib.rs), takes a `Bucket` that the caller must supply, and simply
wraps it in a `Vault`:

```rust
pub fn new(initial_deposit: Bucket) -> Component<Self> {
    Component::new(Self { vault: Vault::from_bucket(initial_deposit) })
        .with_access_rules(AccessRules::allow_all())
        .create()
}
```

For this to be a faucet for the *real* native $TARI, the operator must pass a `Bucket` that
was itself withdrawn from an account holding real $TARI — see
[§3, Instantiating once funded](#3-instantiating-once-funded). The component only ever
redistributes funds it was explicitly given; it has no minting capability at all.

The component exposes four methods — see the full rustdoc in [`src/lib.rs`](src/lib.rs):

| Method | Purpose |
|---|---|
| `new(initial_deposit: Bucket) -> Component<Self>` | Constructor; funds the vault with the caller-supplied bucket. Does not mint. |
| `take_free_coins(&mut self, amount: Amount) -> Bucket` | Withdraws `amount` from the vault. **No on-chain rate limiting** — see §4. |
| `refill(&mut self, bucket: Bucket)` | Deposits `bucket` into the vault. Lets an operator top up the faucet later. |
| `vault_balance(&self) -> Amount` | Read-only balance, for off-chain monitoring. |

## 2. Build/publish via the `tari` CLI

The real workflow for an Ootle template, using
[`tari-project/tari-cli`](https://github.com/tari-project/tari-cli):

```sh
# Scaffold a *new* project (not needed here — this repo already is one). Internally this runs
# cargo-generate against tari-project/wasm-template.
tari create my-template

# Build the WASM binary for an existing template project. Internally this is roughly
# `cargo build-wasm`-style: it builds for the wasm32-unknown-unknown target.
# Requires: rustup target add wasm32-unknown-unknown
tari build .

# Publish the built WASM to a network via the wallet daemon's JSON-RPC API. Requires a wallet
# daemon API key (see below) with at least templates:read, templates:create, accounts:read,
# and transactions:read permissions.
tari publish . --api-key "$TARI_WALLET_DAEMON_API_KEY"
```

`tari publish` authenticates against a running `tari_ootle_walletd` instance's JSON-RPC API,
either via the `TARI_WALLET_DAEMON_API_KEY` environment variable or the `--api-key` flag. The
key must have been minted with (at least) the `templates:read`, `templates:create`,
`accounts:read`, and `transactions:read` permissions.

### Plain-cargo equivalent

If you aren't using the `tari` CLI, the build step is just:

```sh
rustup target add wasm32-unknown-unknown   # one-time, if not already installed
cargo build --release --target wasm32-unknown-unknown
```

This produces `target/wasm32-unknown-unknown/release/tari_ootle_faucet.wasm` — the same
artifact `tari build` would produce, ready to hand to `tari publish` or to a wallet daemon's
`templates.create` JSON-RPC call directly.

## 3. Instantiating once funded

`TariFaucet::new(bucket)` must be called with a real `Bucket` of native $TARI, but **no
separate "pre-funding" step is needed on-chain**: the withdrawal and the construction happen
in the same transaction. A manifest for this typically:

1. Withdraws `initial_amount` of `TARI` from the deploying account's own vault.
2. Puts that withdrawal's output (a `Bucket`) on the workspace.
3. Calls the template's `new` function, passing the workspace bucket as its argument.

See the manifest in [§5](#new-instantiation-manifest) for the exact instructions. The
resulting component is created with its vault already holding `initial_amount` of native
$TARI — there is no window where the component exists but is unfunded.

Note that native $TARI only reaches an account on Ootle in the first place via an L1→L2
**burn-claim** flow (the account owner burns Minotari on L1 and claims the equivalent balance
on L2). That flow is a wallet/base-layer concern and is entirely out of scope for this
template — this template only ever deals with $TARI that has *already* arrived in some
account's balance.

## 4. Off-chain rate-limiting assumption

> [!WARNING]
> `take_free_coins` has **no on-chain rate limiting or access control** beyond
> `AccessRules::allow_all()`. Anyone who can submit a signed manifest call against this
> component's address can call `take_free_coins` for **any amount up to the current vault
> balance**, as many times as they like, with no per-caller cooldown and nothing on-chain
> stopping them from draining the vault in a single transaction.

This is a **deliberate design choice**, not an oversight — it keeps the on-chain component
trivial and auditable. The tradeoff is that **all** rate limiting, abuse prevention, and
amount capping must live **off-chain**, in whatever service actually submits
`take_free_coins` calls on requesters' behalf. A real dispense service built on top of this
component MUST implement, before it ever calls `take_free_coins`:

- Per-requester (per-IP and/or per-account) cooldowns.
- CAPTCHA or equivalent anti-automation checks.
- A maximum amount per request and/or per time window.

Because of this, **this component's address must never be exposed for direct public manifest
submission**. Only the gatekeeping dispense service (which holds the signing key for whatever
account is authorized to call it) should ever submit a transaction calling
`take_free_coins` — end users should only ever interact with the dispense service's HTTP API,
never with the component directly.

## 5. Sample transaction manifests

Tari Ootle manifests are written in a small Rust-like DSL, compiled by
`tari_transaction_manifest::parse_manifest()`, and submitted to a wallet daemon over JSON-RPC
(`auth.request` to get a token, then `transactions.submit_manifest`).

### Taking free coins (dispense service → caller's account)

```rust
fn fee_main() {
    let account = var!["account"];
    account.pay_fee(1000);
}

fn main() {
    let faucet = var!["faucet"];
    let account = var!["account"];
    let amount = var!["amount"];

    let coins = faucet.take_free_coins(amount);
    account.deposit(coins);
}
```

Submitted via the wallet daemon's JSON-RPC `transactions.submit_manifest`:

```sh
curl -s -X POST http://127.0.0.1:9000/ \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "transactions.submit_manifest",
    "params": {
      "manifest": "fn fee_main() { let account = var![\"account\"]; account.pay_fee(1000); } fn main() { let faucet = var![\"faucet\"]; let account = var![\"account\"]; let amount = var![\"amount\"]; let coins = faucet.take_free_coins(amount); account.deposit(coins); }",
      "variables": {
        "faucet": "component_0000000000000000000000000000000000000000000000000000000000000001",
        "account": "component_0000000000000000000000000000000000000000000000000000000000000002",
        "amount": "100"
      },
      "max_fee": 2000,
      "dry_run": true
    }
  }'
```

### `new` instantiation manifest

Withdraws from the deploying account and passes the withdrawal straight into the template's
`new` function, in one transaction (see [§3](#3-instantiating-once-funded)):

```rust
use template_0000000000000000000000000000000000000000000000000000000000000000 as TariFaucet;

fn fee_main() {
    let account = var!["account"];
    account.pay_fee(2000);
}

fn main() {
    let account = var!["account"];
    let initial_amount = var!["initial_amount"];

    let deposit = account.withdraw(TARI, initial_amount);
    let faucet = TariFaucet::new(deposit);
}
```

(Replace the 64-hex-character address after `template_` with this template's real published
address, as returned by `tari publish` / `templates.create`.)

### `refill` manifest

Lets the operator top up an already-deployed faucet from their own account:

```rust
fn fee_main() {
    let account = var!["account"];
    account.pay_fee(1000);
}

fn main() {
    let faucet = var!["faucet"];
    let account = var!["account"];
    let top_up_amount = var!["top_up_amount"];

    let deposit = account.withdraw(TARI, top_up_amount);
    faucet.refill(deposit);
}
```

Manifests refer to the native token with the bare keyword `TARI` (as used above in
`account.withdraw(TARI, ...)`). In prose/user-facing text, call it **$TARI** on mainnet or
**tTARI** on testnet — never `XTR`.

> [!IMPORTANT]
> Per the manifest skill's documented flow (`auth.method` → `auth.request` →
> `submit_manifest` → `wait_result`), **a non-dry-run submission must always be summarized and
> confirmed with the human operator before it is sent.** This applies doubly to anything that
> calls `take_free_coins` on a live faucet component — always dry-run first, and never
> auto-submit a real `take_free_coins` or `refill` call without explicit operator sign-off.
> This rule is guidance for anyone building the off-chain dispense service described in §4.

## 6. What is and isn't verified

- `cargo build` (plain host target) — **ran successfully** in this sandbox.
- `cargo build --release --target wasm32-unknown-unknown` — **ran successfully**, producing
  `target/wasm32-unknown-unknown/release/tari_ootle_faucet.wasm`.
- `cargo test` — **ran successfully**: all 4 integration tests in [`tests/test.rs`](tests/test.rs)
  pass, using `tari_template_test_tooling::TemplateTest`'s in-process engine. These tests
  withdraw from `TemplateTest::create_funded_account()`'s account using
  `account.withdraw(TARI_TOKEN, amount)` (the real native $TARI resource constant,
  `tari_template_lib::types::constants::TARI_TOKEN`, which is an alias for
  `STEALTH_TARI_RESOURCE_ADDRESS`) and feed the resulting bucket into `TariFaucet::new` and
  `TariFaucet::refill` — so they do exercise the genuine native-$TARI code path, inside the
  harness's in-memory engine.
- **Nothing in this repo has been published to any real Tari Ootle network**, testnet or
  mainnet. No live `tari publish` was attempted, and no real wallet daemon was connected to.
  Everything above ran entirely offline, against the in-process `TemplateTest` harness and the
  local Rust toolchain.
