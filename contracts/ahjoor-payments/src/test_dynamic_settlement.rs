#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

// ---------------------------------------------------------------------------
// Mock oracle contract for multi-token settlement tests
// ---------------------------------------------------------------------------
mod mock_oracle {
    use crate::PriceData;
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct MockOracle;

    #[contractimpl]
    impl MockOracle {
        pub fn lastprice(_env: Env, _base: Address, _quote: Address) -> Option<PriceData> {
            Some(PriceData {
                // 1.0 scaled by 10^7
                price: 10_000_000,
                // Intentionally stale baseline for stale-price tests.
                timestamp: 0,
            })
        }
    }
}

fn setup_multi_token<'a>() -> (
    Env,
    AhjoorPaymentsContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
    TokenClient<'a>,
    TokenAdminClient<'a>,
    TokenClient<'a>,
    TokenAdminClient<'a>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);

    // USDC settlement token
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let usdc_client = TokenClient::new(&env, &usdc_addr);
    let usdc_admin_client = TokenAdminClient::new(&env, &usdc_addr);

    // Customer payment token
    let pay_token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let pay_token_client = TokenClient::new(&env, &pay_token_addr);
    let pay_token_admin_client = TokenAdminClient::new(&env, &pay_token_addr);

    let oracle_addr = env.register(mock_oracle::MockOracle, ());

    client.initialize(&admin, &admin, &0u32);
    client.set_min_collateral(&0i128);
    client.approve_merchant(&merchant);
    client.set_oracle(&oracle_addr, &usdc_addr, &3600u64);

    (
        env,
        client,
        admin,
        merchant,
        usdc_addr,
        pay_token_addr,
        usdc_client,
        usdc_admin_client,
        pay_token_client,
        pay_token_admin_client,
    )
}

#[test]
fn test_create_payment_multi_token_success() {
    let (
        env,
        client,
        _admin,
        merchant,
        usdc_addr,
        pay_token_addr,
        _usdc_client,
        usdc_admin_client,
        pay_token_client,
        pay_token_admin_client,
    ) = setup_multi_token();

    let customer = Address::generate(&env);
    pay_token_admin_client.mint(&customer, &10_000_000);
    usdc_admin_client.mint(&client.address, &5_000_000);

    let pid = client.create_payment_multi_token(
        &customer,
        &merchant,
        &1_000_000,
        &pay_token_addr,
        &Some(50u32),
    );

    // Complete should release USDC (token recorded on payment)
    client.complete_payment(&pid);
    let payment = client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::Completed);
    assert_eq!(payment.token, usdc_addr);

    // Customer paid in pay_token and merchant received USDC.
    assert!(pay_token_client.balance(&customer) < 10_000_000);
}

#[test]
#[should_panic(expected = "Oracle not configured")]
fn test_create_payment_multi_token_requires_oracle_config() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let payment_token_admin = TokenAdminClient::new(&env, &payment_token);

    client.initialize(&admin, &admin, &0u32);
    client.set_min_collateral(&0i128);
    client.approve_merchant(&merchant);

    let customer = Address::generate(&env);
    payment_token_admin.mint(&customer, &5_000_000);

    // No set_oracle call -> should panic
    client.create_payment_multi_token(&customer, &merchant, &1_000_000, &payment_token, &None);
}

#[test]
fn test_create_payment_multi_token_rejects_stale_price() {
    let (
        env,
        client,
        _admin,
        merchant,
        _usdc_addr,
        pay_token_addr,
        _usdc_client,
        _usdc_admin_client,
        _pay_token_client,
        pay_token_admin_client,
    ) = setup_multi_token();

    let customer = Address::generate(&env);
    pay_token_admin_client.mint(&customer, &10_000_000);

    // Advance time beyond max_oracle_age to force stale oracle rejection.
    let oracle_addr = client.get_oracle_address();
    let usdc_addr = client.get_usdc_token();
    client.set_oracle(&oracle_addr, &usdc_addr, &1u64);
    env.ledger().set_timestamp(10);
    env.ledger().with_mut(|l| l.timestamp += 10);

    let result = client.try_create_payment_multi_token(
        &customer,
        &merchant,
        &1_000_000,
        &pay_token_addr,
        &None,
    );
    assert!(result.is_err());
}
