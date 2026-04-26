#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Vec,
};

/// Storage TTL Constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 100_000;
const INSTANCE_BUMP_AMOUNT: u32 = 120_000;

const PERSISTENT_LIFETIME_THRESHOLD: u32 = 100_000;
const PERSISTENT_BUMP_AMOUNT: u32 = 120_000;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    TokenAlreadyWhitelisted = 4,
    TokenNotWhitelisted = 5,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Instance: Admin address
    Admin,
    /// Instance: Proposed new admin address (pending acceptance)
    ProposedAdmin,
    /// Persistent: Vec of whitelisted token addresses
    WhitelistedTokens,
}

mod events;
mod client;

pub use client::TokenWhitelistClient;

#[contract]
pub struct TokenWhitelistContract;

#[contractimpl]
impl TokenWhitelistContract {
    /// Initialize the contract with an admin address
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        
        // Initialize empty whitelist
        let empty_vec: Vec<Address> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&DataKey::WhitelistedTokens, &empty_vec);
        env.storage().persistent().extend_ttl(
            &DataKey::WhitelistedTokens,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        events::emit_contract_initialized(&env, admin);
    }

    /// Add a token to the whitelist (admin only)
    pub fn add_token(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut whitelist: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));

        // Check if token already exists
        for existing_token in whitelist.iter() {
            if existing_token == token {
                panic!("Token already whitelisted");
            }
        }

        whitelist.push_back(token.clone());
        env.storage()
            .persistent()
            .set(&DataKey::WhitelistedTokens, &whitelist);
        env.storage().persistent().extend_ttl(
            &DataKey::WhitelistedTokens,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        events::emit_token_whitelisted(&env, token, admin);
    }

    /// Remove a token from the whitelist (admin only)
    pub fn remove_token(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut whitelist: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));

        // Find and remove the token
        let mut found = false;
        let mut new_whitelist = Vec::new(&env);
        for existing_token in whitelist.iter() {
            if existing_token == token {
                found = true;
            } else {
                new_whitelist.push_back(existing_token);
            }
        }

        if !found {
            panic!("Token not whitelisted");
        }

        env.storage()
            .persistent()
            .set(&DataKey::WhitelistedTokens, &new_whitelist);
        env.storage().persistent().extend_ttl(
            &DataKey::WhitelistedTokens,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        events::emit_token_delisted(&env, token, admin);
    }

    /// Check if a token is allowed (public view function)
    pub fn is_token_allowed(env: Env, token: Address) -> bool {
        let whitelist: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));

        env.storage().persistent().extend_ttl(
            &DataKey::WhitelistedTokens,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        for existing_token in whitelist.iter() {
            if existing_token == token {
                return true;
            }
        }
        false
    }

    /// Get all whitelisted tokens (view function)
    pub fn get_whitelisted_tokens(env: Env) -> Vec<Address> {
        let whitelist: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));

        env.storage().persistent().extend_ttl(
            &DataKey::WhitelistedTokens,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        whitelist
    }

    /// Get the current admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    /// Propose a new admin (current admin only)
    pub fn propose_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);

        env.storage()
            .instance()
            .set(&DataKey::ProposedAdmin, &new_admin);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        events::emit_admin_transfer_proposed(&env, current_admin, new_admin);
    }

    /// Accept admin transfer (proposed admin only)
    pub fn accept_admin(env: Env, new_admin: Address) {
        new_admin.require_auth();

        let proposed_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::ProposedAdmin)
            .expect("No admin transfer proposed");

        if new_admin != proposed_admin {
            panic!("Only proposed admin can accept");
        }

        let old_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.storage().instance().remove(&DataKey::ProposedAdmin);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        events::emit_admin_transferred(&env, old_admin, new_admin);
    }

    /// Internal helper to check admin authorization
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");

        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }
}

#[cfg(test)]
mod test;