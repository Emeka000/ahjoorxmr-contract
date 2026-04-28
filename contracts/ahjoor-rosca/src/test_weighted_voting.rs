#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, token
};

fn setup_with_members<'a>(n: usize, voting_mode: VotingMode) -> (Env, AhjoorContractClient<'a>, Address, Address, soroban_sdk::Vec<Address>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..n {
        members.push_back(Address::generate(&env));
    }

    client.init(
        &admin,
        &members,
        &1000, // contribution amount
        &token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode,
        },
        &None,
    );

    (env, client, admin, token_admin, members)
}

#[test]
fn test_weighted_voting_power() {
    let (env, client, admin, token_addr, members) = setup_with_members(3, VotingMode::WeightedByContributions);
    let token_client = token::Client::new(&env, &token_addr);

    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();
    let member3 = members.get(2).unwrap();

    // Give members some tokens
    token_client.mint(&member1, &2000);
    token_client.mint(&member2, &1000);
    // member3 stays with 0 contributions

    // Members contribute to gain weight
    client.contribute(&member1, &token_addr, &2000); // member1 contributes 2000 (maybe 2 rounds worth or partial)
    // Wait, contribute logic handles rounds. Let's see how weight is tracked.
    // MemberContributions is cumulative.

    // member1: 2000
    // member2: 1000 (let's say they contribute 1000)
    client.contribute(&member2, &token_addr, &1000);

    // member3: 0 contributions

    // Verify weights
    assert_eq!(client.get_member_voting_weight(&member1), 2000);
    assert_eq!(client.get_member_voting_weight(&member2), 1000);
    assert_eq!(client.get_member_voting_weight(&member3), 0);

    // Test voting
    client.create_proposal(
        &member1,
        &ProposalType::RuleChange,
        &admin,
        &soroban_sdk::String::from_str(&env, "test"),
        &86400,
        &None,
    );
    let prop_id = 0;

    // Member 3 cannot vote (0 weight)
    // client.vote_on_proposal(&member3, &prop_id, &true); // This should panic

    // Member 2 votes (weight 1000)
    client.vote_on_proposal(&member2, &prop_id, &false);
    
    let prop = client.get_proposals().get(prop_id).unwrap();
    assert_eq!(prop.votes_against, 1000);

    // Member 1 votes (weight 2000)
    client.vote_on_proposal(&member1, &prop_id, &true);
    let prop = client.get_proposals().get(prop_id).unwrap();
    assert_eq!(prop.votes_for, 2000);

    // Quorum check:
    // Total possible votes = 2000 + 1000 + 0 = 3000
    // Required quorum = 51% of 3000 = 1530
    // Total votes cast = 2000 (for) + 1000 (against) = 3000
    // 3000 >= 1530 -> Quorum met
    // 2000 > 1000 -> Approved

    env.ledger().set_timestamp(100000);
    client.execute_proposal(&prop_id);

    let prop = client.get_proposals().get(prop_id).unwrap();
    assert_eq!(prop.status, ProposalStatus::Executed);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #56)")] // InsufficientWeight
fn test_zero_contribution_cannot_vote_in_weighted_mode() {
    let (env, client, _admin, _token_addr, members) = setup_with_members(3, VotingMode::WeightedByContributions);
    let member1 = members.get(0).unwrap();
    let member3 = members.get(2).unwrap();

    client.create_proposal(
        &member1,
        &ProposalType::RuleChange,
        &member1,
        &soroban_sdk::String::from_str(&env, "test"),
        &86400,
        &None,
    );
    
    client.vote_on_proposal(&member3, &0, &true);
}

#[test]
fn test_equal_voting_mode_preserves_behavior() {
    let (env, client, _admin, token_addr, members) = setup_with_members(3, VotingMode::Equal);
    let token_client = token::Client::new(&env, &token_addr);
    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();

    token_client.mint(&member1, &2000);
    client.contribute(&member1, &token_addr, &1000);

    // member1 weight should be 1 despite contribution
    assert_eq!(client.get_member_voting_weight(&member1), 1);

    client.create_proposal(
        &member1,
        &ProposalType::RuleChange,
        &member1,
        &soroban_sdk::String::from_str(&env, "test"),
        &86400,
        &None,
    );

    client.vote_on_proposal(&member2, &0, &true);
    let prop = client.get_proposals().get(0).unwrap();
    assert_eq!(prop.votes_for, 1);
}
