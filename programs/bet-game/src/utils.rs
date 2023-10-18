use crate::Bet;
use crate::BetState;
use anchor_lang::solana_program::clock::{Clock, UnixTimestamp};
use anchor_lang::{prelude::*, system_program};

pub const MINIMUM_TIME_BEFORE_EXPIRY: UnixTimestamp = 120;
pub const MAXIMUN_CLAIMABLE_PERIOD: UnixTimestamp = 300;
pub fn get_unix_timestamp() -> UnixTimestamp {
    Clock::get().unwrap().unix_timestamp
}
pub fn validate_enter_bet(bet: &Bet) -> bool {
    bet.bet_2.is_none() && (bet.expiry_time - MINIMUM_TIME_BEFORE_EXPIRY > get_unix_timestamp())
}

pub fn validate_claim_bet(bet: &Bet) -> bool {
    match bet.state {
        BetState::Created => {
            let current_timestamp = get_unix_timestamp();
            let time_passed_since_expiry = current_timestamp - bet.expiry_time;
            time_passed_since_expiry > 0 && time_passed_since_expiry <= MAXIMUN_CLAIMABLE_PERIOD
        }
        _ => false,
    }
}

pub fn validate_close_bet(bet: &Bet, user_key: Pubkey) -> bool {
    match bet.state {
        BetState::Created => bet.bet_1.player == user_key,
        BetState::Started => {
            ((bet.bet_1.player == user_key)
                || (bet.bet_2.is_some() && bet.bet_2.as_ref().unwrap().player == user_key))
                && (get_unix_timestamp() > bet.expiry_time + MAXIMUN_CLAIMABLE_PERIOD)
        }
        BetState::Player1won => bet.bet_1.player == user_key,
        BetState::Player2won => bet.bet_2.as_ref().unwrap().player == user_key,
        BetState::Draw => {
            (bet.bet_1.player == user_key)
                || (bet.bet_2.is_some() && bet.bet_2.as_ref().unwrap().player == user_key)
        }
    }
}
