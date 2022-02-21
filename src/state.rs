use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::serde::Serialize;
use near_sdk::{env, AccountId, Balance, BorshStorageKey};

use crate::constants::*;
use crate::errors::*;

#[derive(BorshStorageKey, BorshSerialize)]
pub enum StorageKeys {
    BondHolder,
    Bond,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Bond {
    pub value_remaining: u128,
    pub payout_remaining: u128,
    pub vesting_period: u64,
    pub last_time: u64,
    pub price_paid: u128,
}

impl Bond {
    pub fn set_bond(
        &mut self,
        _value_remaining: u128,
        _payout_remaining: u128,
        _vesting_period: u64,
        _last_time: u64,
        _price_paid: u128,
    ) {
        self.value_remaining = _value_remaining;
        self.payout_remaining = _payout_remaining;
        self.last_time = _last_time;
        self.price_paid = _price_paid;
        self.vesting_period = _vesting_period;
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Terms {
    pub control_variable: u128,
    pub vesting_term: u64,
    pub minimum_price: u128,
    pub max_payout: u128,
    pub fee: u128,
}

impl Terms {
    pub fn set_terms(
        &mut self,
        _control_variable: u128,
        _vesting_term: u64,
        _minimum_price: u128,
        _max_payout: u128,
        _fee: u128,
    ) {
        assert!(_vesting_term >= 432000, "{}", E01_VESTING_TIME);
        assert!(_max_payout >= 1000, "{}", E02_MAX_PAYOUT);
        assert!(_fee >= 10000, "{}", E03_FEE);

        self.control_variable = _control_variable;
        self.vesting_term = _vesting_term;
        self.minimum_price = _minimum_price;
        self.max_payout = _max_payout;
        self.fee = _fee;
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Adjust {
    pub add: bool,
    pub rate: u128,
    pub target: u128,
}

impl Adjust {
    pub fn set_adjust(&mut self, _add: bool, _rate: u128, _target: u128) {
        self.add = _add;
        self.rate = _rate;
        self.target = _target;
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct BondData {
    pub token_pure: AccountId,
    pub treasury: AccountId,
    pub bond_balance: Balance,
    pub bond_holder: LookupMap<AccountId, Bond>,
    pub terms: Terms,
    pub adjust: Adjust,
    pub total_debt: Balance,
    pub last_decay: u64,
    pub bond_sold: u128,
    pub total_deposit: u128,
}
impl BondData {
    /* ------- Action ----------*/
    pub fn set_adjust(&mut self, _add: bool, _rate: u128, _target: u128) {
        assert!(
            _rate >= (self.terms.vesting_term * 30 / 1000).into(),
            "{}",
            E04_RATE
        );

        self.adjust.set_adjust(_add, _rate, _target);
    }

    pub fn get_bond_price(&self, token_pure_supply: u128) -> u128 {
        let debt_ratio = self.current_debt() * DECIMAL / token_pure_supply;

        let mut price: u128 = 0;

        if debt_ratio > 0 {
            price = (self.terms.control_variable * debt_ratio + DECIMAL) * 100 / DECIMAL;
        }

        if price < self.terms.minimum_price {
            price = self.terms.minimum_price;
        }

        price
    }

    pub fn current_debt(&self) -> u128 {
        self.total_debt - self.debt_decay()
    }

    pub fn debt_decay(&self) -> u128 {
        let current_block_timestamp = env::block_timestamp() as u128;

        let blocks_since_last = current_block_timestamp - self.last_decay as u128;

        let mut decay = self.total_debt * blocks_since_last / self.terms.vesting_term as u128;

        if decay > self.total_debt {
            decay = self.total_debt;
        }

        decay
    }

    /* ------- View ----------*/
    // pub fn view_bond_data(&self) -> Option<BondDepository> {
    //     self.
    // }

    // pub fn view_bond_price(&self, token_pure_supply: u128) -> u128 {
    //     let debt_ratio = self.current_debt() * DECIMAL / token_pure_supply;

    //     let mut price: u128 = 0;

    //     if debt_ratio > 0 {
    //         price = (self.terms.control_variable * debt_ratio + DECIMAL) * 100 / DECIMAL;
    //     }

    //     if price < self.terms.minimum_price {
    //         price = self.terms.minimum_price;
    //     }

    //     price
    // }
}
