#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String,
};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TokenAdminClient};
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

fn setup_escalation<'a>() -> (
    Env,
    AhjoorRefundContractClient<'a>,
    AhjoorPaymentsContractClient<'a>,
    Address, // admin
    Address, // senior_arbiter
    Address, // token
    TokenClient<'a>,
    TokenAdminClient<'a>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let payment_id = env.register(AhjoorPaymentsContract, ());
    let payment_client = AhjoorPaymentsContractClient::new(&env, &payment_id);

    let refund_id = env.register(AhjoorRefundContract, ());
    let refund_client = AhjoorRefundContractClient::new(&env, &refund_id);

    let admin = Address::generate(&env);
    let senior_arbiter = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin = TokenAdminClient::new(&env, &token_addr);

    payment_client.initialize(&admin, &admin, &0u32);
    refund_client.initialize(&admin, &payment_id, &86_400u64, &None);

    // Configure escalation
    refund_client.set_primary_review_window(&admin, &500u32);
    refund_client.set_senior_review_window(&admin, &300u32);
    refund_client.set_senior_arbiter(&admin, &senior_arbiter);
    refund_client.set_auto_approve_on_senior_miss(&admin, &true);

    (env, refund_client, payment_client, admin, senior_arbiter, token_addr, token_client, token_admin)
}

fn make_completed_payment<'a>(
    env: &Env,
    payment_client: &AhjoorPaymentsContractClient<'a>,
    token_admin: &TokenAdminClient<'a>,
    customer: &Address,
    merchant: &Address,
    token: &Address,
    amount: i128,
) -> u32 {
    token_admin.mint(customer, &(amount * 2));
    let pid = payment_client.create_payment(customer, merchant, &amount, token, &None, &None, &None);
    payment_client.complete_payment(&pid);
    pid
}

#[test]
fn test_primary_resolution_before_deadline() {
    let (env, refund_client, payment_client, admin, _senior, token_addr, _tc, token_admin) =
        setup_escalation();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let pid = make_completed_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &500);
    let rid = refund_client.request_refund(
        &customer, &pid, &500, &String::from_str(&env, "defective"), &0,
    );

    // Admin resolves before primary deadline — normal approve
    refund_client.approve_refund(&admin, &rid);
    let refund = refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Approved);
}

#[test]
#[should_panic(expected = "PrimaryDeadlineNotPassed")]
fn test_escalation_blocked_before_primary_deadline() {
    let (env, refund_client, payment_client, _admin, _senior, token_addr, _tc, token_admin) =
        setup_escalation();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let caller = Address::generate(&env);

    let pid = make_completed_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &500);
    let rid = refund_client.request_refund(
        &customer, &pid, &500, &String::from_str(&env, "defective"), &0,
    );

    // Primary deadline not passed yet → should panic
    refund_client.escalate_to_senior(&caller, &rid);
}

#[test]
fn test_escalation_after_primary_deadline() {
    let (env, refund_client, payment_client, _admin, senior, token_addr, _tc, token_admin) =
        setup_escalation();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let caller = Address::generate(&env);

    let pid = make_completed_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &500);
    let rid = refund_client.request_refund(
        &customer, &pid, &500, &String::from_str(&env, "defective"), &0,
    );

    // Advance ledger past primary deadline (500 ledgers)
    env.ledger().set_sequence_number(env.ledger().sequence() + 501);

    refund_client.escalate_to_senior(&caller, &rid);

    let refund = refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::EscalatedToSenior);
    assert!(refund.senior_review_deadline_ledger > 0);
}

#[test]
fn test_senior_resolution_approved() {
    let (env, refund_client, payment_client, _admin, senior, token_addr, token_client, token_admin) =
        setup_escalation();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let caller = Address::generate(&env);

    let pid = make_completed_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &500);
    let rid = refund_client.request_refund(
        &customer, &pid, &500, &String::from_str(&env, "not_delivered"), &1,
    );

    env.ledger().set_sequence_number(env.ledger().sequence() + 501);
    refund_client.escalate_to_senior(&caller, &rid);

    let resolution_hash = BytesN::from_array(&env, &[1u8; 32]);
    let balance_before = token_client.balance(&customer);

    refund_client.resolve_escalated_refund(&senior, &rid, &true, &resolution_hash);

    let refund = refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Processed);
    // Customer received funds
    assert!(token_client.balance(&customer) > balance_before);
}

#[test]
fn test_senior_resolution_rejected() {
    let (env, refund_client, payment_client, _admin, senior, token_addr, token_client, token_admin) =
        setup_escalation();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let caller = Address::generate(&env);

    let pid = make_completed_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &500);
    let rid = refund_client.request_refund(
        &customer, &pid, &500, &String::from_str(&env, "not_delivered"), &1,
    );

    env.ledger().set_sequence_number(env.ledger().sequence() + 501);
    refund_client.escalate_to_senior(&caller, &rid);

    let balance_before = token_client.balance(&customer);
    let resolution_hash = BytesN::from_array(&env, &[2u8; 32]);

    refund_client.resolve_escalated_refund(&senior, &rid, &false, &resolution_hash);

    let refund = refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Rejected);
    // Escrowed funds returned to customer
    assert!(token_client.balance(&customer) >= balance_before);
}

#[test]
fn test_auto_approve_on_senior_miss() {
    let (env, refund_client, payment_client, _admin, _senior, token_addr, token_client, token_admin) =
        setup_escalation();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let caller = Address::generate(&env);

    let pid = make_completed_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &500);
    let rid = refund_client.request_refund(
        &customer, &pid, &500, &String::from_str(&env, "defective"), &0,
    );

    // Advance past primary deadline and escalate
    env.ledger().set_sequence_number(env.ledger().sequence() + 501);
    refund_client.escalate_to_senior(&caller, &rid);

    // Advance past senior deadline (300 ledgers from escalation)
    env.ledger().set_sequence_number(env.ledger().sequence() + 302);

    let balance_before = token_client.balance(&customer);
    refund_client.trigger_senior_auto_approve(&rid);

    let refund = refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert_eq!(refund.auto_approved_source, Some(String::from_str(&env, "senior_miss")));
    assert!(token_client.balance(&customer) > balance_before);
}

#[test]
#[should_panic(expected = "SeniorDeadlineNotPassed")]
fn test_auto_approve_before_senior_deadline() {
    let (env, refund_client, payment_client, _admin, _senior, token_addr, _tc, token_admin) =
        setup_escalation();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let caller = Address::generate(&env);

    let pid = make_completed_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &500);
    let rid = refund_client.request_refund(
        &customer, &pid, &500, &String::from_str(&env, "defective"), &0,
    );

    env.ledger().set_sequence_number(env.ledger().sequence() + 501);
    refund_client.escalate_to_senior(&caller, &rid);

    // Senior deadline NOT yet passed
    refund_client.trigger_senior_auto_approve(&rid);
}

#[test]
#[should_panic(expected = "UnauthorizedSeniorArbiter")]
fn test_non_senior_cannot_resolve_escalated() {
    let (env, refund_client, payment_client, _admin, _senior, token_addr, _tc, token_admin) =
        setup_escalation();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let caller = Address::generate(&env);
    let impostor = Address::generate(&env);

    let pid = make_completed_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &500);
    let rid = refund_client.request_refund(
        &customer, &pid, &500, &String::from_str(&env, "defective"), &0,
    );

    env.ledger().set_sequence_number(env.ledger().sequence() + 501);
    refund_client.escalate_to_senior(&caller, &rid);

    let hash = BytesN::from_array(&env, &[0u8; 32]);
    refund_client.resolve_escalated_refund(&impostor, &rid, &true, &hash);
}
