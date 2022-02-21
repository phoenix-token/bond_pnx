use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    ext_contract,
};
use near_sdk::{collections::LookupMap, PromiseResult};
use near_sdk::{env, near_bindgen, AccountId, Balance, PanicOnDefault};
use near_sdk::{
    json_types::{ValidAccountId, U128, U64},
    PromiseOrValue,
};

mod constants;
mod errors;
mod state;

use crate::constants::*;
use crate::errors::*;
use crate::state::*;

const NO_DEPOSIT: u128 = 0;

near_sdk::setup_alloc!();

#[ext_contract(ext_ft)]
pub trait FungibleToken {
    fn ft_total_supply(&mut self) -> U128;
    fn mint(&mut self, account_id: ValidAccountId, amount: U128) -> U128;
}

#[ext_contract(ext_self)]
pub trait PhoenixBond {
    fn get_supply_callback(&mut self, token_payment: AccountId, sender: AccountId, amount: Balance);
    fn redeem_callback(
        &mut self,
        sender: AccountId,
        current_time: u64,
        token_payment: AccountId,
        mint_amount: u128,
    );
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct PhoenixBond {
    owner_id: AccountId,
    bond_data: LookupMap<AccountId, BondData>,
}

impl PhoenixBond {
    pub fn deposit(&mut self, token_payment: AccountId, sender: AccountId, amount: Balance) {
        let bond_data = self.bond_data.get(&token_payment).unwrap();

        ext_ft::ft_total_supply(&bond_data.token_pure, NO_DEPOSIT, CALLBACK_GAS).then(
            ext_self::get_supply_callback(
                token_payment,
                sender,
                amount,
                &env::current_account_id(),
                NO_DEPOSIT,
                CALLBACK_GAS,
            ),
        );
    }
}

#[near_bindgen]
impl PhoenixBond {
    #[init]
    pub fn new() -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            owner_id: env::current_account_id(),
            bond_data: LookupMap::new(StorageKeys::Bond),
        }
    }

    pub fn add_new_bond(
        &mut self,
        _token_payment: AccountId,
        _token_pure: AccountId,
        _treasury: AccountId,
        _bond_balance: U128,
        _control_variable: U128,
        _vesting_term: U64,
        _minimum_price: U128,
    ) {
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "{}",
            E05_OWNER
        );

        let bond_data = BondData {
            token_pure: _token_pure.into(),
            treasury: _treasury.into(),
            bond_balance: _bond_balance.into(),
            bond_holder: LookupMap::new(StorageKeys::BondHolder),
            terms: Terms {
                control_variable: _control_variable.into(),
                vesting_term: _vesting_term.into(),
                minimum_price: _minimum_price.into(),
                max_payout: MAX,
                fee: 0,
            },
            adjust: Adjust {
                add: true,
                rate: 0,
                target: MAX,
            },
            total_debt: 0,
            last_decay: env::block_timestamp(),
            bond_sold: 0,
            total_deposit: 0,
        };

        self.bond_data.insert(&_token_payment.into(), &bond_data);

        env::log(b"add new bond");
    }

    pub fn redeem(&mut self, token_payment: AccountId) {
        let sender = env::predecessor_account_id();
        let current_time = env::block_timestamp();

        let bond_data = self.bond_data.get(&token_payment).unwrap();

        let pending_payout: u128 =
            self.pending_payout(&sender, current_time.clone(), &token_payment);

        ext_ft::mint(
            sender.clone().try_into().unwrap(),
            pending_payout.into(),
            &bond_data.token_pure,
            0,
            CALLBACK_GAS,
        )
        .then(ext_self::redeem_callback(
            sender.clone(),
            current_time,
            token_payment,
            pending_payout,
            &env::current_account_id(),
            0,
            CALLBACK_GAS,
        ));
    }

    #[private]
    pub fn get_supply_callback(
        &mut self,
        token_payment: AccountId,
        sender: AccountId,
        amount: Balance,
    ) {
        assert_eq!(env::promise_results_count(), 1, " get_supply_callback");

        let mut bond_data = self.bond_data.remove(&token_payment).unwrap();

        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Failed => {
                env::log(format!("get supply of {} fail", bond_data.token_pure).as_bytes());
            }
            PromiseResult::Successful(result) => {
                let supply = near_sdk::serde_json::from_slice::<U128>(&result).unwrap();
                let bond_price = bond_data.get_bond_price(supply.into());

                env::log(format!("Bond price: {}", bond_price).as_bytes());

                let payout = amount * DECIMAL / bond_price;

                assert!(payout <= bond_data.bond_balance, " Bond too large");

                bond_data.total_debt += amount;

                let is_deposit = bond_data.bond_holder.contains_key(&sender);
                let mut bond_holder: Bond;
                if !is_deposit {
                    bond_holder = Bond {
                        value_remaining: amount,
                        payout_remaining: payout,
                        vesting_period: bond_data.terms.vesting_term,
                        last_time: env::block_timestamp(),
                        price_paid: bond_price,
                    }
                } else {
                    bond_holder = bond_data.bond_holder.remove(&sender).unwrap();
                    bond_holder.set_bond(
                        bond_holder.value_remaining + amount,
                        bond_holder.payout_remaining + payout,
                        bond_data.terms.vesting_term,
                        env::block_timestamp(),
                        bond_price,
                    )
                }
                bond_data.bond_balance = bond_data.bond_balance - payout;
                bond_data.bond_holder.insert(&sender, &bond_holder);
                bond_data.total_deposit = bond_data.total_deposit + amount;
                bond_data.last_decay = env::block_timestamp();
                bond_data.bond_sold = bond_data.bond_sold + payout;
                self.bond_data.insert(&token_payment, &bond_data);
            }
        }
    }

    #[private]
    pub fn redeem_callback(
        &mut self,
        sender: AccountId,
        current_time: u64,
        token_payment: AccountId,
        mint_amount: u128,
    ) {
        assert_eq!(env::promise_results_count(), 1, " redeem_callback");

        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Failed => {
                env::log(format!("Redeem fail").as_bytes());
            }
            PromiseResult::Successful(_result) => {
                let percent: u128 = self.percent_vested(&sender, current_time, &token_payment);
                let mut bond_data = self.bond_data.remove(&token_payment).unwrap();
                let mut bond_holder = bond_data.bond_holder.remove(&sender).unwrap();

                let mut value = bond_holder.value_remaining;

                if percent < 10000 {
                    value = value * percent / 10000;
                }

                bond_holder.set_bond(
                    bond_holder.value_remaining - value,
                    bond_holder
                        .payout_remaining
                        .checked_sub(mint_amount.into())
                        .unwrap(),
                    bond_data.terms.vesting_term,
                    current_time,
                    bond_holder.price_paid,
                );

                bond_data.bond_holder.insert(&sender, &bond_holder);
                bond_data.last_decay = env::block_timestamp();
                bond_data.total_debt = bond_data.total_debt - value;
                self.bond_data.insert(&token_payment, &bond_data);
            }
        }
    }

    pub fn percent_vested(
        &self,
        sender: &AccountId,
        current_time: u64,
        token_payment: &AccountId,
    ) -> u128 {
        let bond_data = self.bond_data.get(&token_payment).unwrap();
        let bond_holder = bond_data.bond_holder.get(&sender).unwrap();

        let time_since_last = current_time - bond_holder.last_time;
        let mut percent: u128 = 0;
        if bond_holder.vesting_period > 0 {
            percent = time_since_last as u128 * 10000 / bond_holder.vesting_period as u128;
        }
        percent
    }

    pub fn pending_payout(
        &self,
        sender: &AccountId,
        current_time: u64,
        token_payment: &AccountId,
    ) -> u128 {
        let bond_data = self.bond_data.get(&token_payment).unwrap();
        let bond_holder = bond_data.bond_holder.get(&sender).unwrap();
        let percent: u128 = self.percent_vested(&sender, current_time, token_payment);

        if percent >= 10000 {
            bond_holder.payout_remaining
        } else {
            bond_holder.payout_remaining * percent / 10000
        }
    }

    pub fn set_vesting_period(&mut self, token_payment: ValidAccountId, time_vesting: U64) {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "NO PERMISSION"
        );

        let mut bond_data = self
            .bond_data
            .remove(&token_payment.clone().into())
            .unwrap();
        bond_data.terms.vesting_term = time_vesting.into();

        self.bond_data
            .insert(&token_payment.clone().into(), &bond_data);
    }

    pub fn get_bond_price(&self, token_payment: &AccountId, token_pure_supply: U128) -> u128 {
        let bond_data = self.bond_data.get(&token_payment).unwrap();
        bond_data.get_bond_price(token_pure_supply.into())
    }

    pub fn get_bond_holder(&self, token_payment: ValidAccountId, sender: ValidAccountId) -> Bond {
        let bond_data = self.bond_data.get(&token_payment.into()).unwrap();
        bond_data.bond_holder.get(&sender.into()).unwrap()
    }

    pub fn get_total_deposit(&self, token_payment: ValidAccountId) -> u128 {
        let bond_data = self.bond_data.get(&token_payment.into()).unwrap();
        bond_data.total_deposit
    }

    pub fn get_bond_balance(&self, token_payment: ValidAccountId) -> u128 {
        let bond_data = self.bond_data.get(&token_payment.into()).unwrap();
        bond_data.bond_balance
    }
}

#[near_bindgen]
impl FungibleTokenReceiver for PhoenixBond {
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        env::log(format!("ft_on_transfer msg {}", msg).as_bytes());

        let token_payment = env::predecessor_account_id();
        let amount: Balance = amount.into();
        assert!(
            self.bond_data.contains_key(&token_payment),
            "{}",
            E06_TOKEN_PAYMENT
        );

        self.deposit(token_payment, sender_id.into(), amount);

        PromiseOrValue::Value(U128(0))
    }
}
