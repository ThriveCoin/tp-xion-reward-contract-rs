use cosmwasm_std::{
    entry_point, to_binary, Addr, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, 
    Response, StdError, StdResult, Uint128
};
use cw_storage_plus::{Item, Map};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct State {
    pub owner: Addr
}

pub const STATE: Item<State> = Item::new("state");
pub const BALANCES: Map<&Addr, Uint128> = Map::new("balances");
pub const TOKEN_DENOM: Item<String> = Item::new("token_denom");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InstantiateMsg {
    pub token_denom: String
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ExecuteMsg {
    Deposit {},
    Reward {
        recipient: String,
        amount: Uint128,
        reason: String
    },
    RewardBulk {
        recipients: Vec<String>,
        amounts: Vec<Uint128>,
        reasons: Vec<String>
    },
    Withdraw {
        amount: Uint128
    },
    UpdateOwnership {
        new_owner: String
    },
    SetTokenDenom {
        denom: String
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum QueryMsg {
    GetBalance { address: String },
    GetTokenDenom {}
}

fn validate_owner(deps: Deps, info: &MessageInfo) -> StdResult<()> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.owner {
        return Err(StdError::generic_err("Unauthorized: Only the owner can call this"));
    }
    Ok(())
}

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg
) -> StdResult<Response> {
    let state = State {
        owner: info.sender.clone()
    };

    STATE.save(deps.storage, &state)?;
    TOKEN_DENOM.save(deps.storage, &msg.token_denom)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", info.sender.to_string())
        .add_attribute("token_denom", msg.token_denom))
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg
) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Deposit {} => execute_deposit(deps, env, info),
        ExecuteMsg::Reward {
            recipient,
            amount,
            reason,
        } => execute_reward(deps, info, recipient, amount, reason),
        ExecuteMsg::RewardBulk {
            recipients,
            amounts,
            reasons,
        } => execute_reward_bulk(deps, info, recipients, amounts, reasons),
        ExecuteMsg::Withdraw { amount } => execute_withdraw(deps, info, amount),
        ExecuteMsg::UpdateOwnership { new_owner } => update_ownership(deps, info, new_owner),
        ExecuteMsg::SetTokenDenom { denom } => set_token_denom(deps, info, denom)
    }
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetBalance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::GetTokenDenom {} => to_binary(&query_token_denom(deps)?)
    }
}

fn query_balance(deps: Deps, address: String) -> StdResult<Uint128> {
    let addr = deps.api.addr_validate(&address)?;
    let balance = BALANCES.may_load(deps.storage, &addr)?.unwrap_or(Uint128::zero());
    Ok(balance)
}

fn query_token_denom(deps: Deps) -> StdResult<String> {
    TOKEN_DENOM.load(deps.storage)
}

pub fn execute_deposit(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo
) -> StdResult<Response> {
    let denom = TOKEN_DENOM.load(deps.storage)?;
    let amount = info
        .funds
        .iter()
        .find(|coin| coin.denom == denom)
        .map(|coin| coin.amount)
        .unwrap_or(Uint128::zero());

    if amount.is_zero() {
        return Err(StdError::generic_err("Deposit amount must be greater than zero"));
    }

    let current_balance = BALANCES
        .may_load(deps.storage, &info.sender)?
        .unwrap_or(Uint128::zero());
    BALANCES.save(deps.storage, &info.sender, &(current_balance + amount))?;

    Ok(Response::new()
        .add_attribute("action", "deposit")
        .add_attribute("sender", info.sender.to_string())
        .add_attribute("amount", amount))
}

pub fn execute_reward(
    deps: DepsMut,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
    reason: String
) -> StdResult<Response> {
    validate_owner(deps.as_ref(), &info)?;

    let recipient_addr = deps.api.addr_validate(&recipient)?;
    let current_balance = BALANCES
        .may_load(deps.storage, &recipient_addr)?
        .unwrap_or(Uint128::zero());
    BALANCES.save(deps.storage, &recipient_addr, &(current_balance + amount))?;

    Ok(Response::new()
        .add_attribute("action", "reward")
        .add_attribute("recipient", recipient_addr.to_string())
        .add_attribute("amount", amount)
        .add_attribute("reason", reason)
        .add_event(cosmwasm_std::Event::new("Reward")
            .add_attribute("recipient", recipient_addr.to_string())
            .add_attribute("amount", amount.to_string())
            .add_attribute("reason", reason)))
}

pub fn execute_withdraw(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
) -> StdResult<Response> {
    if amount.is_zero() {
        return Err(StdError::generic_err("Withdraw amount must be greater than zero"));
    }

    let denom = TOKEN_DENOM.load(deps.storage)?;
    let current_balance = BALANCES
        .may_load(deps.storage, &info.sender)?
        .unwrap_or(Uint128::zero());

    if amount > current_balance {
        return Err(StdError::generic_err("Insufficient balance"));
    }

    let bank_msg = CosmosMsg::Bank(BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: vec![Coin { denom, amount }]
    });

    BALANCES.save(deps.storage, &info.sender, &(current_balance - amount))?;

    Ok(Response::new()
        .add_message(bank_msg)
        .add_attribute("action", "withdraw")
        .add_attribute("amount", amount)
        .add_event(cosmwasm_std::Event::new("Withdrawal")
            .add_attribute("sender", info.sender.to_string())
            .add_attribute("amount", amount.to_string())))
}

pub fn update_ownership(
    deps: DepsMut,
    info: MessageInfo,
    new_owner: String,
) -> StdResult<Response> {
    validate_owner(deps.as_ref(), &info)?;

    let new_owner_addr = deps.api.addr_validate(&new_owner)?;
    STATE.save(deps.storage, &State { owner: new_owner_addr.clone() })?;

    Ok(Response::new()
        .add_attribute("action", "update_ownership")
        .add_attribute("new_owner", new_owner_addr.to_string()))
}

pub fn set_token_denom(
    deps: DepsMut,
    info: MessageInfo,
    denom: String,
) -> StdResult<Response> {
    validate_owner(deps.as_ref(), &info)?;
    TOKEN_DENOM.save(deps.storage, &denom)?;

    Ok(Response::new()
        .add_attribute("action", "set_token_denom")
        .add_attribute("denom", denom))
}
