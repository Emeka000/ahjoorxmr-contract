use soroban_sdk::{contractclient, Address, Env};

/// Client interface for the token whitelist contract
/// This allows other contracts to call the whitelist contract
#[contractclient(name = "TokenWhitelistClient")]
pub trait TokenWhitelistInterface {
    /// Check if a token is allowed
    fn is_token_allowed(env: Env, token: Address) -> bool;
    
    /// Add a token to the whitelist (admin only)
    fn add_token(env: Env, admin: Address, token: Address);
    
    /// Remove a token from the whitelist (admin only)
    fn remove_token(env: Env, admin: Address, token: Address);
    
    /// Get all whitelisted tokens
    fn get_whitelisted_tokens(env: Env) -> soroban_sdk::Vec<Address>;
    
    /// Get the current admin
    fn get_admin(env: Env) -> Address;
}