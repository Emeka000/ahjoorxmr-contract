#![cfg(test)]

use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    vec, Address, Env, IntoVal,
};

#[test]
fn test_rosca_flow_with_time_locks() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AhjoorContract);
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env.register_stellar_asset_contract(admin.clone());
    let token_client = TokenClient::new(&env, &token_admin);
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);
    for u in [&user1, &user2, &user3] {
        token_admin_client.mint(u, &1000);
    }

    let members = vec![&env, user1.clone(), user2.clone(), user3.clone()];
    let duration = 3600u64;
    let amount = 100i128;

    client.init(
        &admin,
        &members,
        &amount,
        &token_admin,
        &duration,
        &PayoutStrategy::RoundRobin,
        &None,
    );

    env.ledger().set_timestamp(100);
    client.contribute(&user1);
    assert_eq!(token_client.balance(&user1), 900);

    env.ledger().set_timestamp(3601);
    let result = client.try_contribute(&user2);
    assert!(result.is_err());

    client.close_round();

    let (round, paid, deadline, _) = client.get_state();
    assert_eq!(round, 1);
    assert_eq!(paid.len(), 0);
    assert_eq!(deadline, 7201);

    env.ledger().set_timestamp(4000);
    client.contribute(&user1);
    assert_eq!(token_client.balance(&user1), 800);
}

#[test]
#[should_panic(expected = "Cannot close: Deadline has not passed yet")]
fn test_cannot_close_early() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, AhjoorContract);
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let members = vec![&env, Address::generate(&env)];

    client.init(
        &admin,
        &members,
        &100,
        &Address::generate(&env),
        &3600,
        &PayoutStrategy::RoundRobin,
        &None,
    );

    env.ledger().set_timestamp(500);
    client.close_round();
}

#[test]
fn test_on_time_contribution() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AhjoorContract);
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);
    let token_client = TokenClient::new(&env, &token_admin);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    token_admin_client.mint(&user1, &1000);
    let members = vec![&env, user1.clone(), user2.clone()];

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &PayoutStrategy::RoundRobin,
        &None,
    );

    env.ledger().set_timestamp(1000);
    client.contribute(&user1);

    assert_eq!(token_client.balance(&user1), 900);
    let (_, paid, _, _) = client.get_state();
    assert!(paid.contains(&user1));
}

#[test]
#[should_panic(expected = "Contribution failed: Round deadline has passed")]
fn test_late_contribution_rejection() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AhjoorContract);
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env.register_stellar_asset_contract(admin.clone());
    let user1 = Address::generate(&env);
    let members = vec![&env, user1.clone()];

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &PayoutStrategy::RoundRobin,
        &None,
    );

    env.ledger().set_timestamp(3601);
    client.contribute(&user1);
}

#[test]
fn test_admin_close_round() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AhjoorContract);
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env.register_stellar_asset_contract(admin.clone());
    let members = vec![&env, Address::generate(&env)];

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &PayoutStrategy::RoundRobin,
        &None,
    );

    env.ledger().set_timestamp(3601);
    client.close_round();

    let (round, _, _, _) = client.get_state();
    assert_eq!(round, 1);
}

// --- NEW STRATEGY-SPECIFIC TESTS ---

#[test]
fn test_admin_assigned_strategy_execution() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, AhjoorContract);
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env.register_stellar_asset_contract(admin.clone());
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let members = vec![&env, user1.clone(), user2.clone()];

    // Reverse the order: user2 should get paid first
    let custom_order = vec![&env, user2.clone(), user1.clone()];

    token_admin_client.mint(&user1, &100);
    token_admin_client.mint(&user2, &100);

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &PayoutStrategy::AdminAssigned,
        &Some(custom_order),
    );

    client.contribute(&user1);
    client.contribute(&user2);

    let token_client = TokenClient::new(&env, &token_admin);
    // User2 contributed 100, but was the recipient of the pot (200)
    assert_eq!(token_client.balance(&user2), 200);
}

#[test]
#[should_panic(expected = "Custom order length mismatch")]
fn test_invalid_admin_order_validation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, AhjoorContract);
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let members = vec![&env, Address::generate(&env), Address::generate(&env)];
    let bad_order = vec![&env, Address::generate(&env)]; // Too short

    client.init(
        &admin,
        &members,
        &100,
        &Address::generate(&env),
        &3600,
        &PayoutStrategy::AdminAssigned,
        &Some(bad_order),
    );
}

#[test]
fn test_round_robin_e2e_all_rounds() {
    let env = Env::default();
    env.mock_all_auths();
    // FIX: Removed & and used register
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    // FIX: Use v2 and get the address
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);
    let token_client = TokenClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let members = vec![&env, u1.clone(), u2.clone()];

    // FIX: Mint 2000 to cover multiple contributions and payouts
    for u in [&u1, &u2] {
        token_admin_client.mint(u, &2000);
    }

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &PayoutStrategy::RoundRobin,
        &None,
    );

    // ROUND 0: u1 should get the payout
    client.contribute(&u1);
    client.contribute(&u2);
    // Math: 2000 (start) - 100 (spent) + 200 (pot) = 2100
    assert_eq!(token_client.balance(&u1), 2100);

    // ROUND 1: u2 should get the payout
    client.contribute(&u1);
    client.contribute(&u2);
    // Math: 2000 (start) - 100 (spent R0) - 100 (spent R1) + 200 (pot R1) = 2000
    assert_eq!(token_client.balance(&u2), 2000);
}

#[test]
fn test_admin_assigned_e2e_all_rounds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);
    let token_client = TokenClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let members = vec![&env, u1.clone(), u2.clone()];

    for u in [&u1, &u2] {
        token_admin_client.mint(u, &2000);
    }

    // Strategy: Admin Assigned (Reverse the order: u2 then u1)
    let custom_order = vec![&env, u2.clone(), u1.clone()];
    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &PayoutStrategy::AdminAssigned,
        &Some(custom_order),
    );

    // ROUND 0: u2 should get the payout first
    client.contribute(&u1);
    client.contribute(&u2);
    assert_eq!(token_client.balance(&u2), 2100);

    // ROUND 1: u1 should get the payout second
    client.contribute(&u1);
    client.contribute(&u2);
    assert_eq!(token_client.balance(&u1), 2000);
}

#[test]
fn test_verify_contract_events() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);
    
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    token_admin_client.mint(&user1, &1000);
    token_admin_client.mint(&user2, &1000);

    let members = vec![&env, user1.clone(), user2.clone()];
    let amount = 100i128;

    // 1. Verify ContractInitialized
    client.init(&admin, &members, &amount, &token_admin, &3600, &PayoutStrategy::RoundRobin, &None);
    
    let last_event = env.events().all().last().unwrap();
    assert_eq!(last_event.0, contract_id);
    // Topics check
    assert_eq!(last_event.1, vec![&env, symbol_short!("init").into_val(&env)]);
    // Data check: Convert Val -> (u32, i128)
    let init_data: (u32, i128) = soroban_sdk::FromVal::from_val(&env, &last_event.2);
    assert_eq!(init_data, (2u32, amount));

    // 2. Verify ContributionReceived
    client.contribute(&user1);
    
    let contribution_event = env.events().all().last().unwrap();
    assert_eq!(contribution_event.1, vec![&env, symbol_short!("contrib").into_val(&env), user1.clone().into_val(&env), 0u32.into_val(&env)]);
    // Data check: Val -> i128
    let contrib_amt: i128 = soroban_sdk::FromVal::from_val(&env, &contribution_event.2);
    assert_eq!(contrib_amt, amount);

    // 3. Verify RoundCompleted and RoundReset
    client.contribute(&user2);
    
    let all_events = env.events().all();
    let reset_event = all_events.get(all_events.len() - 1).unwrap();
    let payout_event = all_events.get(all_events.len() - 2).unwrap();

    // RoundCompleted check: Val -> (Address, i128)
    let payout_data: (Address, i128) = soroban_sdk::FromVal::from_val(&env, &payout_event.2);
    assert_eq!(payout_data, (user1.clone(), 200i128));

    // RoundReset check: Val -> u32
    let reset_round: u32 = soroban_sdk::FromVal::from_val(&env, &reset_event.2);
    assert_eq!(reset_round, 0u32);
}