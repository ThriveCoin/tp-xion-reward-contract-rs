use cosmwasm_std::{
    entry_point, to_json_binary, attr, Addr, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, 
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
        QueryMsg::GetBalance { address } => to_json_binary(&query_balance(deps, address)?),
        QueryMsg::GetTokenDenom {} => to_json_binary(&query_token_denom(deps)?)
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

fn reward_single(
    deps: DepsMut,
    recipient: String,
    amount: Uint128,
    _reason: String,
) -> StdResult<()> {
    let recipient_addr = deps.api.addr_validate(&recipient)?;
    BALANCES.update(deps.storage, &recipient_addr, |balance: Option<Uint128>| -> StdResult<_> {
        Ok(balance.unwrap_or_default() + amount)
    })?;
    Ok(())
}

pub fn execute_reward_bulk(
    mut deps: DepsMut,
    info: MessageInfo,
    recipients: Vec<String>,
    amounts: Vec<Uint128>,
    reasons: Vec<String>,
) -> StdResult<Response> {
    validate_owner(deps.as_ref(), &info)?;

    if recipients.len() != amounts.len() || recipients.len() != reasons.len() {
        return Err(cosmwasm_std::StdError::generic_err("Array lengths mismatch"));
    }

    for ((recipient, amount), reason) in recipients.iter().zip(amounts.iter()).zip(reasons.iter()) {
        reward_single(deps.branch(), recipient.clone(), *amount, reason.clone())?;
    }

    Ok(Response::new().add_attributes(vec![attr("action", "reward_bulk")]))
}

pub fn execute_reward(
    deps: DepsMut,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
    reason: String,
) -> StdResult<Response> {
    validate_owner(deps.as_ref(), &info)?;

    reward_single(deps, recipient.clone(), amount, reason.clone())?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "reward"),
        attr("recipient", recipient),
        attr("amount", amount.to_string()),
        attr("reason", reason),
    ]))
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

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{attr, coins};
    use cosmwasm_std::Uint128;

    const OWNER: &str = "owner";
    const USER: &str = "user";
    const DENOM: &str = "utoken";

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);

        let res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(
            res.attributes,
            vec![
                attr("action", "instantiate"),
                attr("owner", OWNER),
                attr("token_denom", DENOM),
            ]
        );

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.owner, Addr::unchecked(OWNER));

        let token_denom = TOKEN_DENOM.load(&deps.storage).unwrap();
        assert_eq!(token_denom, DENOM.to_string());
    }

    #[test]
    fn deposit_works() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let deposit_info = mock_info(USER, &coins(100, DENOM));
        let msg = ExecuteMsg::Deposit {};
        let res = execute(deps.as_mut(), mock_env(), deposit_info.clone(), msg).unwrap();

        assert_eq!(
            res.attributes,
            vec![
                attr("action", "deposit"),
                attr("sender", USER),
                attr("amount", "100"),
            ]
        );

        let balance = BALANCES.load(&deps.storage, &Addr::unchecked(USER)).unwrap();
        assert_eq!(balance, Uint128::new(100));
    }

    #[test]
    fn deposit_fails_for_zero_amount() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let deposit_info = mock_info(USER, &coins(0, DENOM));
        let msg = ExecuteMsg::Deposit {};
        let err = execute(deps.as_mut(), mock_env(), deposit_info, msg).unwrap_err();
        assert_eq!(err, StdError::generic_err("Deposit amount must be greater than zero"));
    }

    #[test]
    fn withdraw_works() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let deposit_info = mock_info(USER, &coins(200, DENOM));
        execute(
            deps.as_mut(),
            mock_env(),
            deposit_info.clone(),
            ExecuteMsg::Deposit {},
        )
        .unwrap();

        let withdraw_msg = ExecuteMsg::Withdraw {
            amount: Uint128::new(100),
        };
        let res = execute(deps.as_mut(), mock_env(), deposit_info.clone(), withdraw_msg).unwrap();

        assert_eq!(
            res.attributes,
            vec![attr("action", "withdraw"), attr("amount", "100")]
        );

        let balance = BALANCES.load(&deps.storage, &Addr::unchecked(USER)).unwrap();
        assert_eq!(balance, Uint128::new(100));
    }

    #[test]
    fn withdraw_fails_for_zero_amount() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let withdraw_msg = ExecuteMsg::Withdraw {
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), mock_env(), info.clone(), withdraw_msg).unwrap_err();
        assert_eq!(err, StdError::generic_err("Withdraw amount must be greater than zero"));
    }

    #[test]
    fn withdraw_fails_for_insufficient_balance() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let withdraw_msg = ExecuteMsg::Withdraw {
            amount: Uint128::new(100),
        };
        let err = execute(deps.as_mut(), mock_env(), info.clone(), withdraw_msg).unwrap_err();
        assert_eq!(err, StdError::generic_err("Insufficient balance"));
    }

    #[test]
    fn reward_bulk_fails_for_mismatched_lengths() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::RewardBulk {
            recipients: vec![USER.to_string()],
            amounts: vec![Uint128::new(100), Uint128::new(50)], // Mismatched length
            reasons: vec!["Reason1".to_string()],
        };

        let err = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert_eq!(err, StdError::generic_err("Array lengths mismatch"));
    }

    #[test]
    fn reward_works() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::Reward {
            recipient: USER.to_string(),
            amount: Uint128::new(50),
            reason: "Test reward".to_string(),
        };

        let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(
            res.attributes,
            vec![
                attr("action", "reward"),
                attr("recipient", USER),
                attr("amount", "50"),
                attr("reason", "Test reward"),
            ]
        );

        let balance = BALANCES.load(&deps.storage, &Addr::unchecked(USER)).unwrap();
        assert_eq!(balance, Uint128::new(50));
    }

    #[test]
    fn reward_bulk_works() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
    
        let msg = ExecuteMsg::RewardBulk {
            recipients: vec![USER.to_string(), "user2".to_string()],
            amounts: vec![Uint128::new(100), Uint128::new(50)],
            reasons: vec!["Reason1".to_string(), "Reason2".to_string()],
        };
    
        let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(res.attributes, vec![attr("action", "reward_bulk")]);
    
        let balance1 = BALANCES.load(&deps.storage, &Addr::unchecked(USER)).unwrap();
        assert_eq!(balance1, Uint128::new(100));
    
        let balance2 = BALANCES.load(&deps.storage, &Addr::unchecked("user2")).unwrap();
        assert_eq!(balance2, Uint128::new(50));
    }

    #[test]
    fn query_balance_works() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let deposit_info = mock_info(USER, &coins(100, DENOM));
        execute(
            deps.as_mut(),
            mock_env(),
            deposit_info.clone(),
            ExecuteMsg::Deposit {},
        )
        .unwrap();

        let balance = query_balance(deps.as_ref(), USER.to_string()).unwrap();
        assert_eq!(balance, Uint128::new(100));
    }

    #[test]
    fn update_ownership_works() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let new_owner = "new_owner";
        let msg = ExecuteMsg::UpdateOwnership {
            new_owner: new_owner.to_string(),
        };

        let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(
            res.attributes,
            vec![
                attr("action", "update_ownership"),
                attr("new_owner", new_owner),
            ]
        );

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.owner, Addr::unchecked(new_owner));
    }

    #[test]
    fn set_token_denom_works() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let new_denom = "utest";
        let res = set_token_denom(deps.as_mut(), info.clone(), new_denom.to_string()).unwrap();
        assert_eq!(
            res.attributes,
            vec![
                attr("action", "set_token_denom"),
                attr("denom", new_denom),
            ]
        );

        let token_denom = TOKEN_DENOM.load(&deps.storage).unwrap();
        assert_eq!(token_denom, new_denom);
    }

    #[test]
    fn set_token_denom_fails_for_unauthorized() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let unauthorized_info = mock_info(USER, &[]);
        let new_denom = "utest";
        let err = set_token_denom(deps.as_mut(), unauthorized_info, new_denom.to_string()).unwrap_err();
        assert_eq!(err, StdError::generic_err("Unauthorized: Only the owner can call this"));
    }

    #[test]
    fn query_token_denom_works() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let token_denom = query_token_denom(deps.as_ref()).unwrap();
        assert_eq!(token_denom, DENOM);
    }

    #[test]
    fn validate_owner_works() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            token_denom: DENOM.to_string(),
        };
        let info = mock_info(OWNER, &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let unauthorized_info = mock_info(USER, &[]);
        let err = validate_owner(deps.as_ref(), &unauthorized_info).unwrap_err();
        assert_eq!(err, StdError::generic_err("Unauthorized: Only the owner can call this"));
    }
}
