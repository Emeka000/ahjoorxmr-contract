#![cfg(test)]

use crate::{AhjoorPaymentsContract, AhjoorPaymentsContractClient, Error};
use ahjoor_token_whitelist::{TokenWhitelistContract, TokenWhitelistContractClient};
use soroban_sdk::{
    testutils::{Address as _},
    token, Address, Env, String,
};

fn create_token_contract<'a>(e: &Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
    let contract = e.register_stellar_asset_contract_v2(admin.clone());
    let contract_address = contract.address();
    let client = token::StellarAssetClient::new(e, &contract_address);
    (contract_address, client)
}

fn setup_test_env() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    AhjoorPaymentsContractClient<'static>,
    TokenWhitelistContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let (token_address, token_client) = create_token_contract(&env, &token_admin);
    token_client.mint(&customer, &10_000);

    // Setup payments contract
    let payments_contract_id = env.register(AhjoorPaymentsContract, ());
    let payments_client = AhjoorPaymentsContractClient::new(&env, &payments_contract_id);
    payments_client.initialize(&admin, &fee_recipient, &0);
    payments_client.set_merchant_open_mode(&true);

    // Setup whitelist contract
    let whitelist_contract_id = env.register(TokenWhitelistContract, ());
    let whitelist_client = TokenWhitelistContractClient::new(&env, &whitelist_contract_id);
    whitelist_client.initialize(&admin);

    // Connect payments contract to whitelist
    payments_client.set_token_whitelist_contract(&admin, &whitelist_contract_id);

    (
        env,
        admin,
        customer,
        merchant,
        token_address,
        whitelist_contract_id,
        payments_client,
        whitelist_client,
    )
}

#[test]
fn test_set_token_whitelist_contract() {
    let (
        _env,
        admin,
        _customer,
        _merchant,
        _token,
        whitelist_contract_id,
        payments_client,
        _whitelist_client,
    ) = setup_test_env();

    // Verify whitelist contract is set
    let stored_address = payments_client.get_token_whitelist_contract();
    assert_eq!(stored_address, Some(whitelist_contract_id));
}

#[test]
#[should_panic(expected = "Only admin can set token whitelist contract")]
fn test_set_token_whitelist_contract_unauthorized() {
    let (
        env,
        _admin,
        customer,
        _merchant,
        _token,
        whitelist_contract_id,
        payments_client,
        _whitelist_client,
    ) = setup_test_env();

    let unauthorized = Address::generate(&env);
    payments_client.set_token_whitelist_contract(&unauthorized, &whitelist_contract_id);
}

#[test]
fn test_payment_with_whitelisted_token() {
    let (
        _env,
        admin,
        customer,
        merchant,
        token,
        _whitelist_contract_id,
        payments_client,
        whitelist_client,
    ) = setup_test_env();

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Create payment should succeed
    let payment_id = payments_client.create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &None,
        &None,
        &None,
    );

    // Verify payment was created
    let payment = payments_client.get_payment(&payment_id);
    assert_eq!(payment.amount, 1000);
    assert_eq!(payment.token, token);
}

#[test]
fn test_payment_with_non_whitelisted_token_fails() {
    let (
        _env,
        _admin,
        customer,
        merchant,
        token,
        _whitelist_contract_id,
        payments_client,
        _whitelist_client,
    ) = setup_test_env();

    // Don't add token to whitelist

    // Create payment should fail
    let result = payments_client.try_create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &None,
        &None,
        &None,
    );

    assert!(result.is_err());
    // Check that it's the correct error
    assert_eq!(result.unwrap_err().unwrap(), Error::TokenNotAllowed.into());
}

#[test]
fn test_batch_payment_with_mixed_tokens() {
    let (
        env,
        admin,
        customer,
        merchant,
        token1,
        _whitelist_contract_id,
        payments_client,
        whitelist_client,
    ) = setup_test_env();

    // Create second token
    let token_admin2 = Address::generate(&env);
    let (token2, token2_client) = create_token_contract(&env, &token_admin2);
    token2_client.mint(&customer, &10_000);

    // Add only token1 to whitelist
    whitelist_client.add_token(&admin, &token1);

    // Create batch with mixed tokens
    let mut payments = soroban_sdk::Vec::new(&env);
    payments.push_back(crate::PaymentRequest {
        merchant: merchant.clone(),
        amount: 1000,
        token: token1.clone(),
        reference: None,
        metadata: None,
    });
    payments.push_back(crate::PaymentRequest {
        merchant: merchant.clone(),
        amount: 2000,
        token: token2.clone(), // Not whitelisted
        reference: None,
        metadata: None,
    });

    // Batch should fail due to non-whitelisted token
    let result = payments_client.try_create_payments_batch(&customer, &payments);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), Error::TokenNotAllowed.into());
}

#[test]
fn test_subscription_with_non_whitelisted_token_fails() {
    let (
        _env,
        _admin,
        customer,
        merchant,
        token,
        _whitelist_contract_id,
        payments_client,
        _whitelist_client,
    ) = setup_test_env();

    // Don't add token to whitelist

    // Create subscription should fail
    let result = payments_client.try_create_subscription(
        &customer,
        &merchant,
        &1000,
        &token,
        &86400, // 1 day
        &10,    // max charges
    );

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), Error::TokenNotAllowed.into());
}

#[test]
fn test_token_delisted_mid_operation() {
    let (
        _env,
        admin,
        customer,
        merchant,
        token,
        _whitelist_contract_id,
        payments_client,
        whitelist_client,
    ) = setup_test_env();

    // Add token to whitelist
    whitelist_client.add_token(&admin, &token);

    // Create payment should succeed
    let _payment_id = payments_client.create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &None,
        &None,
        &None,
    );

    // Remove token from whitelist
    whitelist_client.remove_token(&admin, &token);

    // New payment should fail
    let result = payments_client.try_create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &None,
        &None,
        &None,
    );

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), Error::TokenNotAllowed.into());
}

#[test]
fn test_no_whitelist_contract_allows_all_tokens() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let (token_address, token_client) = create_token_contract(&env, &token_admin);
    token_client.mint(&customer, &10_000);

    // Setup payments contract WITHOUT whitelist
    let payments_contract_id = env.register(AhjoorPaymentsContract, ());
    let payments_client = AhjoorPaymentsContractClient::new(&env, &payments_contract_id);
    payments_client.initialize(&admin, &fee_recipient, &0);
    payments_client.set_merchant_open_mode(&true);

    // Don't set whitelist contract

    // Payment should succeed (backward compatibility)
    let payment_id = payments_client.create_payment(
        &customer,
        &merchant,
        &1000,
        &token_address,
        &None,
        &None,
        &None,
    );

    let payment = payments_client.get_payment(&payment_id);
    assert_eq!(payment.amount, 1000);
}

#[test]
fn test_is_token_allowed_public_function() {
    let (
        _env,
        admin,
        _customer,
        _merchant,
        token,
        _whitelist_contract_id,
        payments_client,
        whitelist_client,
    ) = setup_test_env();

    // Initially not allowed
    assert!(!payments_client.is_token_allowed(&token));

    // Add to whitelist
    whitelist_client.add_token(&admin, &token);

    // Now allowed
    assert!(payments_client.is_token_allowed(&token));

    // Remove from whitelist
    whitelist_client.remove_token(&admin, &token);

    // Not allowed again
    assert!(!payments_client.is_token_allowed(&token));
}