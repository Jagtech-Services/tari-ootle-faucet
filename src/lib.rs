// Tari Ootle native-$TARI faucet template.
//
// Unlike the toy upstream `TestFaucet` example (which mints a brand-new fake "FAUCET"
// fungible resource out of thin air), this component never mints anything. It is a thin
// `Vault` wrapper that must be funded externally with a real `Bucket` of the network's
// native $TARI resource (see `tari_template_lib::constants::TARI_TOKEN`), and simply hands
// out coins from that vault on request. See the top-level README.md for the full rationale,
// deployment workflow, and — most importantly — the off-chain rate-limiting assumption that
// any real dispense service built on top of this component must honor.
use tari_template_lib::prelude::*;

#[template]
mod tari_faucet_template {
    use super::*;

    /// A faucet component that dispenses the network's real native $TARI resource.
    ///
    /// The component holds a single [`Vault`] of native $TARI. It never mints any resource:
    /// every coin it ever dispenses was deposited into it first, either at construction time
    /// (via [`Self::new`]) or later via [`Self::refill`]. This makes it safe to use with the
    /// real native token instead of a throwaway test token.
    pub struct TariFaucet {
        /// Holds the native $TARI balance that this faucet dispenses from.
        vault: Vault,
    }

    impl TariFaucet {
        /// Creates a new faucet component, funding its vault with `initial_deposit`.
        ///
        /// `initial_deposit` must be a `Bucket` of the native $TARI resource (e.g. withdrawn
        /// from the deploying account's own balance in the same transaction — see the README
        /// for a sample manifest). This constructor does not mint anything; it only moves the
        /// caller-supplied bucket into a fresh vault owned by the new component.
        ///
        /// The component is created with `AccessRules::allow_all()`, meaning every method
        /// below (including `take_free_coins`) is callable by anyone who can submit a signed
        /// manifest call against this component's address. See the "Off-chain rate-limiting
        /// assumption" section of the README before exposing this component's address to the
        /// public.
        pub fn new(initial_deposit: Bucket) -> Component<Self> {
            Component::new(Self {
                vault: Vault::from_bucket(initial_deposit),
            })
            .with_access_rules(AccessRules::allow_all())
            .create()
        }

        /// Withdraws `amount` of native $TARI from the vault and returns it as a `Bucket`.
        ///
        /// # No on-chain rate limiting
        ///
        /// This method implements **no on-chain rate limiting or access control** beyond the
        /// component-wide `AccessRules::allow_all()` set in [`Self::new`]. Anyone who can
        /// submit a signed manifest call against this component may call `take_free_coins`
        /// for any `amount` up to the current vault balance, as many times as they like, in
        /// as many transactions as they like.
        ///
        /// Rate limiting — per-IP or per-requester-address cooldowns, CAPTCHA/anti-abuse
        /// checks, maximum-amount-per-day caps, etc. — is entirely the responsibility of an
        /// **off-chain** HTTP dispense service that holds the signing key for an account
        /// authorized to call this method, and that gatekeeps every call it makes on this
        /// component's behalf. This component's address must never be handed out for direct
        /// public manifest submission; only the gatekeeping service should ever call it.
        ///
        /// This is a deliberate design choice to keep the on-chain component simple, not an
        /// oversight.
        pub fn take_free_coins(&mut self, amount: Amount) -> Bucket {
            self.vault.withdraw(amount)
        }

        /// Deposits `bucket` into the vault, topping up the faucet's native $TARI balance.
        ///
        /// Lets the operator refill the faucet later via a simple manifest call (see the
        /// README for an example), without needing to redeploy or reconstruct the component.
        pub fn refill(&mut self, bucket: Bucket) {
            self.vault.deposit(bucket);
        }

        /// Returns the current native $TARI balance held by the faucet's vault.
        ///
        /// Read-only; intended for off-chain monitoring/alerting, e.g. a cron job that polls
        /// this method to decide when the vault needs refilling.
        pub fn vault_balance(&self) -> Amount {
            self.vault.balance()
        }
    }
}
