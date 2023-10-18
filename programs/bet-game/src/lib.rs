use anchor_lang::solana_program::clock::UnixTimestamp;
use anchor_lang::{prelude::*, system_program};
use pyth_sdk_solana::load_price_feed_from_account_info;
// This is your program's public key and it will update
// automatically when you build the project.
declare_id!("5Afkbo4PhAZGZfsyQx99wk78pPgZzMAxAQCLZCgjBVSE");
// player 1 creates a bet
//all can see the bet
//player 2 also bets some number on this creaated bet
//whoever is close after the end time wins and claims the betAmount
mod utils;
use crate::utils::*;
//constants
pub const MASTER_SEED: &[u8] = b"master";
pub const BET_SEED: &[u8] = b"bet";
pub const MINIMUM_TIME_BEFORE_EXPIRY: UnixTimestamp = 120;
pub const MAXIMUN_CLAIMABLE_PERIOD: UnixTimestamp = 300;

#[program]
mod bet_game {

    use super::*; //just in case we split the files
    pub fn create_master(_ctx: Context<CreateMaster>) -> Result<()> {
        Ok(())
    }
    pub fn create_bet(
        ctx: Context<CreateBet>,
        price: f64,
        amount: u64,
        duration: u32, //in seconds
        pyth_price_key: Pubkey,
    ) -> Result<()> {
        let master = &mut ctx.accounts.master;
        let bet = &mut ctx.accounts.bet;
        master.last_bet_id += 1;
        bet.id = master.last_bet_id;
        bet.amount = amount;
        bet.pyth_price_key = pyth_price_key;
        bet.expiry_time = get_unix_timestamp() + duration as i64;
        bet.bet_1 = BetPrediction {
            player: ctx.accounts.player.key(),
            price_predicted: price,
        };
        //transfer the bet_amount to the third-party pda and then whenever the bet ends ,,transfer the amount to the winner account
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.player.to_account_info(),
                    to: bet.to_account_info(),
                },
            ),
            bet.amount,
        )?;
        bet.state = BetState::Created;
        Ok(())
    }

    pub fn enter_bet(
        ctx: Context<EnterBet>,
        price: f64, //2nd player's price prediction
    ) -> Result<()> {
        let bet = &mut ctx.accounts.bet;
        bet.bet_2 = Some(BetPrediction {
            price_predicted: price,
            player: ctx.accounts.player.key(),
        });
        bet.state = BetState::Started;

        //again transfer amount to bet pda
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.player.to_account_info(),
                    to: bet.to_account_info(),
                },
            ),
            bet.amount,
        )?;

        Ok(())
    }

    pub fn claim_bet(ctx: Context<ClaimBet>) -> Result<()> {
        let bet = &mut ctx.accounts.bet;
        let pyth_account = &(ctx.accounts.pyth_key);
        let feed = load_price_feed_from_account_info(pyth_account)
            .map_err(|_| error!(BetError::InvalidPythAccount));
        let prize = bet.amount.checked_mul(2).unwrap();
        **bet.to_account_info().try_borrow_mut_lamports()? -= prize;
        //gett price data from the feed
        let price_data = feed?.get_price_unchecked();
        require!(price_data.price <= f64::max as i64, BetError::PriceTooMuch);
        let pyth_price = price_data.price as f64;
        msg!("pth price is {}", pyth_price);
        //real price =pyth_price *10
        //check which bet is the closer one to the real price
        let multiplier = 10f64.powi(-price_data.expo);
        let adjusted_bet_1 = bet.bet_1.price_predicted * multiplier;
        let adjusted_bet_2 = bet.bet_2.as_ref().unwrap().price_predicted * multiplier;
        let difference_1 = (adjusted_bet_1 - pyth_price).abs();
        let difference_2 = (adjusted_bet_2 - pyth_price).abs();

        if difference_1 < difference_2 {
            bet.state = BetState::Player1won;
            **ctx
                .accounts
                .player_1
                .to_account_info()
                .try_borrow_mut_lamports()? += prize;
        } else if difference_1 > difference_2 {
            bet.state = BetState::Player2won;
            **ctx
                .accounts
                .player_2
                .to_account_info()
                .try_borrow_mut_lamports()? += prize;
        } else {
            let draw_amount = bet.amount;
            bet.state = BetState::Draw;
            **ctx
                .accounts
                .player_1
                .to_account_info()
                .try_borrow_mut_lamports()? += draw_amount;

            **ctx
                .accounts
                .player_2
                .to_account_info()
                .try_borrow_mut_lamports()? += draw_amount;
        };

        Ok(())
    }

    pub fn close_bet(_ctx: Context<CloseBet>) -> Result<()> {
        Ok(())
    }
}
#[derive(Accounts)]
pub struct CreateMaster<'info> {
    // We must specify the space in order to initialize an account.
    // First 8 bytes are default account discriminator,
    // next 8 bytes come from NewAccount.data being type u64.
    //seeds are used to create the pubkey
    #[account(
        init,
        payer = payer,
        space = 8 + 8,
        seeds=[MASTER_SEED],
        bump
        )]
    pub master: Account<'info, Master>,
    #[account(mut)]
    pub payer: Signer<'info>, //comes from prelude
    pub system_program: Program<'info, System>, //comes from system program
}

#[derive(Accounts)]
pub struct CreateBet<'info> {
    #[account(
        init,
        payer = player,
        space = 8 + 8 + 32 + 8 + 8 + 32 + 8 + 1 + 32 + 8 + 1,
        seeds=[BET_SEED,&(master.last_bet_id+1).to_le_bytes()],
        bump
        )]
    pub bet: Account<'info, Bet>,
    //now get the account with tat seed phrase and pub key
    #[account(
        mut,        
        seeds=[MASTER_SEED],
        bump
        )]
    pub master: Account<'info, Master>,

    #[account(mut)]
    pub player: Signer<'info>, //comes from prelude
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EnterBet<'info> {
    #[account(
        mut,
        seeds=[BET_SEED,&bet.id.to_le_bytes()],
        bump,
        constraint=validate_enter_bet(&*bet) @BetError::CannotEnter,
        )]
    pub bet: Account<'info, Bet>,

    #[account(mut)]
    pub player: Signer<'info>, //comes from prelude

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimBet<'info> {
    #[account(
        mut,
        seeds=[BET_SEED,&bet.id.to_le_bytes()],
        bump,
        constraint=validate_claim_bet(&*bet) @BetError::CannotClaim,
    )]
    pub bet: Account<'info, Bet>,
    #[account(address=bet.pyth_price_key @BetError::InvalidPythKey)]
    pub pyth_key: AccountInfo<'info>,

    #[account(address=bet.bet_1.player)]
    pub player_1: AccountInfo<'info>,

    #[account(address=bet.bet_2.as_ref().unwrap().player)]
    pub player_2: AccountInfo<'info>,

    #[account(mut)]
    pub signer: Signer<'info>, //comes from prelude

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CloseBet<'info> {
    #[account(
        mut,
        seeds=[BET_SEED,&bet.id.to_le_bytes()],
        bump,
        close=player,//the one signing can close this account
        constraint=validate_close_bet(&*bet,player.key()) @BetError::CannotClose,
    )]
    pub bet: Account<'info, Bet>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct Master {
    pub last_bet_id: u64,
}

#[account]
pub struct Bet {
    pub id: u64,     //unique for every user
    pub amount: u64, //cost to stor this bet in lamports
    pub state: BetState,
    pub bet_1: BetPrediction,
    pub bet_2: Option<BetPrediction>,
    pub pyth_price_key: Pubkey, //key to fetch from oracle price feeds
    pub expiry_time: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct BetPrediction {
    pub player: Pubkey,
    pub price_predicted: f64,
}
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq)]
pub enum BetState {
    Player1won,
    Player2won,
    Draw,
    Created,
    Started,
}

#[error_code]
pub enum BetError {
    #[msg("Cannot enter")]
    CannotEnter,
    #[msg("Cannot claim")]
    CannotClaim,
    #[msg("Invalid pyth key")]
    InvalidPythKey,
    #[msg("Invaliid pyth account")]
    InvalidPythAccount,
    #[msg("give a smaller price ")]
    PriceTooMuch,
    #[msg("cannot close")]
    CannotClose,
}
