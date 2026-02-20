#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, token, Address, Env, Vec, Map};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum PayoutStrategy {
    RoundRobin = 0,
    AdminAssigned = 1,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,           // Address
    Members,         // Vec<Address>
    PayoutOrder,     // Vec<Address>
    Strategy,        // PayoutStrategy
    ContributionAmt, // i128
    Token,           // Address
    CurrentRound,    // u32
    PaidMembers,     // Vec<Address>
    RoundDuration,   // u64
    RoundDeadline,   // u64
    Defaulters,      // Vec<Address>
    PenaltyAmount,   // i128
    DefaultCount,    // Map<Address, u32>
    SuspendedMembers, // Vec<Address>
}

#[contract]
pub struct AhjoorContract;

#[contractimpl]
impl AhjoorContract {
    pub fn init(
        env: Env,
        admin: Address,
        members: Vec<Address>,
        contribution_amount: i128,
        token: Address,
        round_duration: u64,
        strategy: PayoutStrategy,
        custom_order: Option<Vec<Address>>,
        penalty_amount: i128,
    ) {
        if env.storage().instance().has(&DataKey::Members) {
            panic!("Already initialized");
        }

        let resolved_order = match strategy {
            PayoutStrategy::RoundRobin => members.clone(),
            PayoutStrategy::AdminAssigned => {
                let order = custom_order.expect("AdminAssigned strategy requires a custom order");
                if order.len() != members.len() {
                    panic!("Custom order length mismatch");
                }
                for member in order.iter() {
                    if !members.contains(&member) {
                        panic!("Custom order contains non-member address");
                    }
                }
                order
            }
        };

        let start_time = env.ledger().timestamp();
        let deadline = start_time + round_duration;
        let member_count = members.len();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Members, &members);
        env.storage()
            .instance()
            .set(&DataKey::PayoutOrder, &resolved_order);
        env.storage().instance().set(&DataKey::Strategy, &strategy);
        env.storage()
            .instance()
            .set(&DataKey::ContributionAmt, &contribution_amount);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::CurrentRound, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::PaidMembers, &Vec::<Address>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::RoundDuration, &round_duration);
        env.storage()
            .instance()
            .set(&DataKey::RoundDeadline, &deadline);
        env.storage()
            .instance()
            .set(&DataKey::Defaulters, &Vec::<Address>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::PenaltyAmount, &penalty_amount);
        env.storage()
            .instance()
            .set(&DataKey::DefaultCount, &Map::<Address, u32>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::SuspendedMembers, &Vec::<Address>::new(&env));

        // Event: ContractInitialized
        env.events().publish(
            (symbol_short!("init"),),
            (member_count, contribution_amount),
        );
    }

    pub fn contribute(env: Env, contributor: Address) {
        contributor.require_auth();

        let deadline: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RoundDeadline)
            .expect("Deadline not set");
        if env.ledger().timestamp() > deadline {
            panic!("Contribution failed: Round deadline has passed");
        }

        let members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Members)
            .expect("Not initialized");
        if !members.contains(&contributor) {
            panic!("Not a member");
        }

        let mut paid_members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::PaidMembers)
            .expect("Not initialized");
        if paid_members.contains(&contributor) {
            panic!("Already contributed for this round");
        }

        let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let client = token::Client::new(&env, &token_addr);
        let amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ContributionAmt)
            .unwrap();
        let current_round: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CurrentRound)
            .unwrap_or(0);

        client.transfer(&contributor, &env.current_contract_address(), &amount);

        // Event: ContributionReceived
        env.events().publish(
            (symbol_short!("contrib"), contributor.clone(), current_round),
            amount,
        );

        paid_members.push_back(contributor.clone());
        env.storage()
            .instance()
            .set(&DataKey::PaidMembers, &paid_members);

        if paid_members.len() == members.len() {
            Self::complete_round_payout(&env, &paid_members, amount, client);
        }
    }

    pub fn close_round(env: Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Admin not set");
        admin.require_auth();

        let deadline: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RoundDeadline)
            .unwrap();
        if env.ledger().timestamp() <= deadline {
            panic!("Cannot close: Deadline has not passed yet");
        }

        let members: Vec<Address> = env.storage().instance().get(&DataKey::Members).unwrap();
        let paid_members: Vec<Address> =
            env.storage().instance().get(&DataKey::PaidMembers).unwrap();

        let mut defaulters = Vec::new(&env);
        for member in members.iter() {
            if !paid_members.contains(&member) {
                defaulters.push_back(member);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::Defaulters, &defaulters);

        let current_round: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CurrentRound)
            .unwrap();

        // Use an internal reset function or emit event here if required
        env.events()
            .publish((symbol_short!("closed"), current_round), defaulters);

        Self::reset_round_state(&env, current_round);
    }

    pub fn penalise_defaulter(env: Env, member: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Admin not set");
        admin.require_auth();

        let penalty_amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::PenaltyAmount)
            .unwrap_or(0);
        if penalty_amount == 0 {
            panic!("Penalty system is disabled");
        }

        let defaulters: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Defaulters)
            .unwrap_or(Vec::new(&env));
        if !defaulters.contains(&member) {
            panic!("Member is not a defaulter for this round");
        }

        // Transfer penalty from defaulter to contract
        let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        let client = token::Client::new(&env, &token_addr);
        
        // Require auth from the member being penalized
        member.require_auth();
        client.transfer(&member, &env.current_contract_address(), &penalty_amount);

        // Update default count
        let mut default_count: Map<Address, u32> = env
            .storage()
            .instance()
            .get(&DataKey::DefaultCount)
            .unwrap_or(Map::new(&env));
        let current_defaults = default_count.get(member.clone()).unwrap_or(0);
        let new_default_count = current_defaults + 1;
        default_count.set(member.clone(), new_default_count);
        env.storage()
            .instance()
            .set(&DataKey::DefaultCount, &default_count);

        // Event: MemberDefaulted
        let current_round: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CurrentRound)
            .unwrap();
        env.events().publish(
            (symbol_short!("defaulted"), member.clone(), current_round),
            (penalty_amount, new_default_count),
        );

        // Check if member should be suspended (2+ defaults)
        if new_default_count >= 2 {
            let mut suspended_members: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::SuspendedMembers)
                .unwrap_or(Vec::new(&env));
            if !suspended_members.contains(&member) {
                suspended_members.push_back(member.clone());
                env.storage()
                    .instance()
                    .set(&DataKey::SuspendedMembers, &suspended_members);

                // Event: MemberSuspended
                env.events().publish(
                    (symbol_short!("suspended"), member.clone()),
                    new_default_count,
                );
            }
        }
    }

    fn complete_round_payout(
        env: &Env,
        _paid_members: &Vec<Address>,
        _amount: i128,
        client: token::Client,
    ) {
        let current_round: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CurrentRound)
            .unwrap();
        let payout_order: Vec<Address> =
            env.storage().instance().get(&DataKey::PayoutOrder).unwrap();
        let suspended_members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::SuspendedMembers)
            .unwrap_or(Vec::new(env));

        // Find next non-suspended recipient
        let mut recipient_idx = current_round % payout_order.len();
        let mut attempts = 0;
        while attempts < payout_order.len() {
            let potential_recipient = payout_order.get(recipient_idx).unwrap();
            if !suspended_members.contains(&potential_recipient) {
                break;
            }
            recipient_idx = (recipient_idx + 1) % payout_order.len();
            attempts += 1;
        }

        if attempts >= payout_order.len() {
            panic!("All members are suspended");
        }

        let payout_recipient = payout_order.get(recipient_idx).unwrap();
        
        // Total pot is all the tokens in the contract (contributions + penalties)
        let total_pot = client.balance(&env.current_contract_address());

        client.transfer(
            &env.current_contract_address(),
            &payout_recipient,
            &total_pot,
        );

        // Event: RoundCompleted
        env.events().publish(
            (symbol_short!("rd_done"), current_round),
            (payout_recipient, total_pot),
        );

        Self::reset_round_state(env, current_round);
    }

    fn reset_round_state(env: &Env, current_round: u32) {
        let duration: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RoundDuration)
            .unwrap();

        env.storage()
            .instance()
            .set(&DataKey::CurrentRound, &(current_round + 1));
        env.storage()
            .instance()
            .set(&DataKey::PaidMembers, &Vec::<Address>::new(env));
        env.storage().instance().set(
            &DataKey::RoundDeadline,
            &(env.ledger().timestamp() + duration),
        );

        // Event: RoundReset
        env.events()
            .publish((symbol_short!("reset"),), current_round);
    }

    pub fn get_state(env: Env) -> (u32, Vec<Address>, u64, PayoutStrategy) {
        let current_round: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CurrentRound)
            .unwrap_or(0);
        let paid_members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::PaidMembers)
            .unwrap_or(Vec::new(&env));
        let deadline: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RoundDeadline)
            .unwrap_or(0);
        let strategy: PayoutStrategy = env
            .storage()
            .instance()
            .get(&DataKey::Strategy)
            .unwrap_or(PayoutStrategy::RoundRobin);

        (current_round, paid_members, deadline, strategy)
    }
}

mod test;
