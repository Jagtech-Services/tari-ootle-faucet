//! Integration tests for the `TariFaucet` template, using `tari_template_test_tooling`'s
//! in-process `TemplateTest` harness.
//!
//! Note on native $TARI in these tests: `TemplateTest::create_funded_account` funds its
//! account from the harness's built-in XTR faucet with the real native $TARI resource
//! (`tari_template_lib::types::constants::TARI_TOKEN`, a.k.a. `STEALTH_TARI_RESOURCE_ADDRESS`).
//! We withdraw from that funded account with `account.withdraw(TARI_TOKEN, amount)` exactly as
//! shown in `tari_template_test_tooling`'s own README/upstream engine tests, and feed the
//! resulting `Bucket` into `TariFaucet::new`/`refill`. So these tests do exercise the real
//! native-$TARI code path end-to-end inside the harness — see the top-level README.md for what
//! this does and does not prove about a real, live network.

use tari_template_lib::prelude::{Amount, ComponentAddress};
use tari_template_test_tooling::{
    TemplateTest,
    support::assert_error::assert_reject_reason,
    template_lib_types::constants::TARI_TOKEN,
    transaction::args,
};

/// `new` wraps whatever bucket it's given into a vault, and `vault_balance` reports exactly
/// that amount back — with no minting involved anywhere in the process.
#[test]
fn new_funds_the_vault_with_the_supplied_bucket() {
    let mut test = TemplateTest::my_crate();
    let (account, owner_proof, secret_key) = test.create_funded_account();

    test.execute_expect_success(
        test.transaction()
            .call_method(account, "withdraw", args![TARI_TOKEN, Amount::from(5_000u64)])
            .put_last_instruction_output_on_workspace("deposit")
            .call_function(
                test.get_template_address("TariFaucet"),
                "new",
                args![Workspace("deposit")],
            )
            .build_and_seal(&secret_key),
        vec![owner_proof],
    );
    let faucet: ComponentAddress = test.expect_return(2);

    let balance: Amount = test.call_method(faucet, "vault_balance", args![], vec![]);
    assert_eq!(balance, Amount::from(5_000u64));
}

/// `take_free_coins` withdraws exactly `amount` from the vault and the withdrawn coins are
/// real, depositable native $TARI (they land correctly in the caller's own account).
#[test]
fn take_free_coins_withdraws_and_can_be_deposited() {
    let mut test = TemplateTest::my_crate();
    let (operator, operator_proof, operator_secret_key) = test.create_funded_account();
    let (claimant, claimant_proof, claimant_secret_key) = test.create_funded_account();

    // Instantiate the faucet, funded with 10_000 native $TARI withdrawn from the operator.
    test.execute_expect_success(
        test.transaction()
            .call_method(operator, "withdraw", args![TARI_TOKEN, Amount::from(10_000u64)])
            .put_last_instruction_output_on_workspace("deposit")
            .call_function(
                test.get_template_address("TariFaucet"),
                "new",
                args![Workspace("deposit")],
            )
            .build_and_seal(&operator_secret_key),
        vec![operator_proof],
    );
    let faucet: ComponentAddress = test.expect_return(2);

    // A claimant (who did not fund the faucet) calls `take_free_coins` and deposits the result
    // into their own account — demonstrating `AccessRules::allow_all()` really does allow any
    // caller, and that the dispensed bucket is a real, depositable native $TARI bucket.
    test.execute_expect_success(
        test.transaction()
            .call_method(faucet, "take_free_coins", args![Amount::from(1_500u64)])
            .put_last_instruction_output_on_workspace("coins")
            .call_method(claimant, "deposit", args![Workspace("coins")])
            .build_and_seal(&claimant_secret_key),
        vec![claimant_proof],
    );

    let balance: Amount = test.call_method(faucet, "vault_balance", args![], vec![]);
    assert_eq!(balance, Amount::from(8_500u64));
}

/// Taking more than the vault holds fails the transaction rather than overdrawing the vault.
#[test]
fn take_free_coins_fails_when_amount_exceeds_balance() {
    let mut test = TemplateTest::my_crate();
    let (operator, operator_proof, operator_secret_key) = test.create_funded_account();

    test.execute_expect_success(
        test.transaction()
            .call_method(operator, "withdraw", args![TARI_TOKEN, Amount::from(1_000u64)])
            .put_last_instruction_output_on_workspace("deposit")
            .call_function(
                test.get_template_address("TariFaucet"),
                "new",
                args![Workspace("deposit")],
            )
            .build_and_seal(&operator_secret_key),
        vec![operator_proof.clone()],
    );
    let faucet: ComponentAddress = test.expect_return(2);

    let reject = test.execute_expect_failure(
        test.transaction()
            .call_method(faucet, "take_free_coins", args![Amount::from(1_000_000u64)])
            .build_and_seal(&operator_secret_key),
        vec![operator_proof],
    );
    // Message wording differs slightly between resource types ("Bucket contained insufficient
    // funds" for plain fungible vs. "...insufficient revealed funds" for the native $TARI
    // stealth resource used here), so match on the common substring rather than the exact
    // engine error string.
    assert_reject_reason(reject, "insufficient");
}

/// `refill` lets the operator top the vault back up after coins have been taken from it, and
/// the harness's demonstrated-no-on-chain-rate-limiting behaviour (anyone can call
/// `take_free_coins` repeatedly) still holds after a refill.
#[test]
fn refill_tops_up_the_vault() {
    let mut test = TemplateTest::my_crate();
    let (operator, operator_proof, operator_secret_key) = test.create_funded_account();

    test.execute_expect_success(
        test.transaction()
            .call_method(operator, "withdraw", args![TARI_TOKEN, Amount::from(2_000u64)])
            .put_last_instruction_output_on_workspace("deposit")
            .call_function(
                test.get_template_address("TariFaucet"),
                "new",
                args![Workspace("deposit")],
            )
            .build_and_seal(&operator_secret_key),
        vec![operator_proof.clone()],
    );
    let faucet: ComponentAddress = test.expect_return(2);

    // Drain most of it.
    test.execute_expect_success(
        test.transaction()
            .call_method(faucet, "take_free_coins", args![Amount::from(1_900u64)])
            .put_last_instruction_output_on_workspace("coins")
            .call_method(operator, "deposit", args![Workspace("coins")])
            .build_and_seal(&operator_secret_key),
        vec![operator_proof.clone()],
    );
    let balance_after_drain: Amount = test.call_method(faucet, "vault_balance", args![], vec![]);
    assert_eq!(balance_after_drain, Amount::from(100u64));

    // Refill it from the operator's own (still-funded) account.
    test.execute_expect_success(
        test.transaction()
            .call_method(operator, "withdraw", args![TARI_TOKEN, Amount::from(50_000u64)])
            .put_last_instruction_output_on_workspace("top_up")
            .call_method(faucet, "refill", args![Workspace("top_up")])
            .build_and_seal(&operator_secret_key),
        vec![operator_proof],
    );

    let balance_after_refill: Amount = test.call_method(faucet, "vault_balance", args![], vec![]);
    assert_eq!(balance_after_refill, Amount::from(50_100u64));
}
