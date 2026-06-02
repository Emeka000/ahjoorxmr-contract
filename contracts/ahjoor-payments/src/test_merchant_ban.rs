#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env};

fn make_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn setup_ban<'a>() -> (Env, AhjoorPaymentsContractClient<'a>, Address, Address, Address, TokenClient<'a>, TokenAdminClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin, &admin, &0u32);
    // Set min_collateral to 0 so approve_merchant doesn't require a deposit in tests
    client.set_min_collateral(&0i128);

    (env, client, admin, merchant, token_addr, token_client, token_admin_client)
}

// ---------------------------------------------------------------------------
// Test: suspension — payments paused, expiry auto-lifts
// ---------------------------------------------------------------------------
#[test]
fn test_suspension_blocks_payments() {
    let (env, client, admin, merchant, token_addr, _token_client, token_admin_client) = setup_ban();

    // Approve merchant first
    client.approve_merchant(&merchant);
    assert_eq!(client.get_merchant_status(&merchant), MerchantStatus::Active);

    // Suspend for 3600 seconds
    client.suspend_merchant(&admin, &merchant, &make_hash(&env, 1), &3600u64);
    assert_eq!(client.get_merchant_status(&merchant), MerchantStatus::Suspended);

    // Payment should be blocked
    let customer = Address::generate(&env);
    token_admin_client.mint(&customer, &1000);
    let result = client.try_create_payment(&customer, &merchant, &100, &token_addr, &None, &None, &None);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: ban + appeal + approval with cooling-off period
// ---------------------------------------------------------------------------
#[test]
fn test_ban_appeal_approval_with_cooling_off() {
    let (env, client, admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    client.approve_merchant(&merchant);
    client.ban_merchant(&admin, &merchant, &make_hash(&env, 2));
    assert_eq!(client.get_merchant_status(&merchant), MerchantStatus::Banned);

    // Merchant submits appeal
    client.submit_appeal(&merchant, &make_hash(&env, 3));
    let appeal = client.get_merchant_appeal(&merchant).unwrap();
    assert_eq!(appeal.status, AppealStatus::Pending);
    assert_eq!(appeal.cooling_off_until, 0u64);

    // Admin approves - merchant enters cooling-off period
    client.approve_appeal(&admin, &merchant);
    
    // Merchant is still banned during cooling-off
    assert_eq!(client.get_merchant_status(&merchant), MerchantStatus::Banned);

    let appeal = client.get_merchant_appeal(&merchant).unwrap();
    assert_eq!(appeal.status, AppealStatus::ApprovedCoolingOff);
    assert!(appeal.cooling_off_until > 0);

    // Try to complete reinstatement before cooling-off expires - should fail
    let result = client.try_complete_reinstatement(&merchant);
    assert!(result.is_err());

    // Advance past cooling-off period (default 7 days = 604800 seconds)
    env.ledger().with_mut(|l| l.timestamp += 604801);

    // Now complete reinstatement should succeed
    client.complete_reinstatement(&merchant);
    assert_eq!(client.get_merchant_status(&merchant), MerchantStatus::Active);

    let appeal = client.get_merchant_appeal(&merchant).unwrap();
    assert_eq!(appeal.status, AppealStatus::ApprovedReinstated);
}

// ---------------------------------------------------------------------------
// Test: ban + appeal + rejection + cooldown
// ---------------------------------------------------------------------------
#[test]
fn test_ban_appeal_rejection_cooldown() {
    let (env, client, admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    let cooldown = 7200u64;
    client.set_appeal_rejection_cooldown(&admin, &cooldown);

    client.approve_merchant(&merchant);
    client.ban_merchant(&admin, &merchant, &make_hash(&env, 4));

    client.submit_appeal(&merchant, &make_hash(&env, 5));
    client.reject_appeal(&admin, &merchant);

    // Merchant still banned
    assert_eq!(client.get_merchant_status(&merchant), MerchantStatus::Banned);

    // Immediate re-appeal blocked by cooldown
    let result = client.try_submit_appeal(&merchant, &make_hash(&env, 6));
    assert!(result.is_err());

    // Advance past cooldown
    env.ledger().with_mut(|l| l.timestamp += cooldown + 1);

    // Now appeal is allowed
    client.submit_appeal(&merchant, &make_hash(&env, 7));
    let appeal = client.get_merchant_appeal(&merchant).unwrap();
    assert_eq!(appeal.status, AppealStatus::Pending);
}

// ---------------------------------------------------------------------------
// Test: one-active-appeal guard
// ---------------------------------------------------------------------------
#[test]
#[should_panic(expected = "An active appeal already exists")]
fn test_duplicate_appeal_rejected() {
    let (env, client, admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    client.approve_merchant(&merchant);
    client.ban_merchant(&admin, &merchant, &make_hash(&env, 8));

    client.submit_appeal(&merchant, &make_hash(&env, 9));
    // Second appeal while first is unresolved — should panic
    client.submit_appeal(&merchant, &make_hash(&env, 10));
}

// ---------------------------------------------------------------------------
// Test: reinstate_merchant clears ban
// ---------------------------------------------------------------------------
#[test]
fn test_reinstate_merchant() {
    let (env, client, admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    client.approve_merchant(&merchant);
    client.ban_merchant(&admin, &merchant, &make_hash(&env, 11));
    assert_eq!(client.get_merchant_status(&merchant), MerchantStatus::Banned);

    client.reinstate_merchant(&admin, &merchant);
    assert_eq!(client.get_merchant_status(&merchant), MerchantStatus::Active);
    assert!(client.is_merchant_approved(&merchant));
}

// ---------------------------------------------------------------------------
// Test: only banned merchant can submit appeal
// ---------------------------------------------------------------------------
#[test]
#[should_panic(expected = "Only banned merchants can submit an appeal")]
fn test_non_banned_merchant_cannot_appeal() {
    let (_env, client, _admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    client.approve_merchant(&merchant);
    // Merchant is active, not banned - should fail
    client.submit_appeal(&merchant, &make_hash(&_env, 12));
}

// ---------------------------------------------------------------------------
// Test: cooling-off period is enforced
// ---------------------------------------------------------------------------
#[test]
#[should_panic(expected = "Cooling-off period has not elapsed")]
fn test_cooling_off_period_enforced() {
    let (env, client, admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    client.approve_merchant(&merchant);
    client.ban_merchant(&admin, &merchant, &make_hash(&env, 13));
    client.submit_appeal(&merchant, &make_hash(&env, 14));
    client.approve_appeal(&admin, &merchant);

    // Try to complete reinstatement immediately - should panic
    client.complete_reinstatement(&merchant);
}

// ---------------------------------------------------------------------------
// Test: appeal status transitions correctly
// ---------------------------------------------------------------------------
#[test]
fn test_appeal_status_transitions() {
    let (env, client, admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    client.approve_merchant(&merchant);
    client.ban_merchant(&admin, &merchant, &make_hash(&env, 15));

    // Initial state: no appeal
    assert!(client.get_merchant_appeal(&merchant).is_none());

    // Submit appeal
    client.submit_appeal(&merchant, &make_hash(&env, 16));
    let appeal = client.get_merchant_appeal(&merchant).unwrap();
    assert_eq!(appeal.status, AppealStatus::Pending);

    // Approve appeal
    client.approve_appeal(&admin, &merchant);
    let appeal = client.get_merchant_appeal(&merchant).unwrap();
    assert_eq!(appeal.status, AppealStatus::ApprovedCoolingOff);

    // After cooling-off period
    env.ledger().with_mut(|l| l.timestamp += 604801);
    client.complete_reinstatement(&merchant);
    let appeal = client.get_merchant_appeal(&merchant).unwrap();
    assert_eq!(appeal.status, AppealStatus::ApprovedReinstated);
}

// ---------------------------------------------------------------------------
// Test: rejected appeal status
// ---------------------------------------------------------------------------
#[test]
fn test_rejected_appeal_status() {
    let (env, client, admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    client.approve_merchant(&merchant);
    client.ban_merchant(&admin, &merchant, &make_hash(&env, 17));
    client.submit_appeal(&merchant, &make_hash(&env, 18));
    client.reject_appeal(&admin, &merchant);

    let appeal = client.get_merchant_appeal(&merchant).unwrap();
    assert_eq!(appeal.status, AppealStatus::Rejected);
}

// ---------------------------------------------------------------------------
// Test: cannot approve already resolved appeal
// ---------------------------------------------------------------------------
#[test]
#[should_panic(expected = "Appeal already resolved or not pending")]
fn test_cannot_approve_resolved_appeal() {
    let (env, client, admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    client.approve_merchant(&merchant);
    client.ban_merchant(&admin, &merchant, &make_hash(&env, 19));
    client.submit_appeal(&merchant, &make_hash(&env, 20));
    client.approve_appeal(&admin, &merchant);

    // Try to approve again - should fail
    client.approve_appeal(&admin, &merchant);
}

// ---------------------------------------------------------------------------
// Test: cannot reject already resolved appeal
// ---------------------------------------------------------------------------
#[test]
#[should_panic(expected = "Appeal already resolved or not pending")]
fn test_cannot_reject_resolved_appeal() {
    let (env, client, admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    client.approve_merchant(&merchant);
    client.ban_merchant(&admin, &merchant, &make_hash(&env, 21));
    client.submit_appeal(&merchant, &make_hash(&env, 22));
    client.reject_appeal(&admin, &merchant);

    // Try to reject again - should fail
    client.reject_appeal(&admin, &merchant);
}

// ---------------------------------------------------------------------------
// Test: cannot submit second appeal while in cooling-off
// ---------------------------------------------------------------------------
#[test]
#[should_panic(expected = "An active appeal already exists")]
fn test_cannot_appeal_during_cooling_off() {
    let (env, client, admin, merchant, _token_addr, _token_client, _token_admin_client) = setup_ban();

    client.approve_merchant(&merchant);
    client.ban_merchant(&admin, &merchant, &make_hash(&env, 23));
    client.submit_appeal(&merchant, &make_hash(&env, 24));
    client.approve_appeal(&admin, &merchant);

    // Try to submit another appeal during cooling-off - should fail
    client.submit_appeal(&merchant, &make_hash(&env, 25));
}