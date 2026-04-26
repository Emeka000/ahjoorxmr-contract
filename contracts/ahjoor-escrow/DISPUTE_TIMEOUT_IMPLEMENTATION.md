# Dispute Timeout Mechanism Implementation

## Overview
Successfully implemented automatic dispute timeout functionality in ahjoor-escrow contract that releases funds to a pre-configured default party when arbiters fail to resolve disputes within a configurable deadline.

## Features Implemented

### 1. Configurable Default Winner
- **Global Configuration**: Admin can set default winner (Buyer or Seller) via `set_default_dispute_winner()`
- **Per-Escrow Override**: Each escrow can specify its own default winner at creation time
- **Default Behavior**: Defaults to Buyer-favored if not configured

### 2. Configurable Timeout Deadline
- **Global Configuration**: Admin can set default timeout via `update_default_dispute_timeout()`
- **Per-Escrow Override**: Each escrow can specify custom timeout via `create_escrow_w_timeout()`
- **Default Timeout**: 7 days (604,800 seconds)

### 3. Automatic Timeout Enforcement
- **Public Function**: `enforce_dispute_timeout(escrow_id)` callable by anyone
- **Deadline Tracking**: Starts when dispute is raised and arbiter assigned
- **Automatic Release**: Funds released to configured default winner after deadline
- **Arbiter Penalty**: Timeout counter incremented for reputation tracking

### 4. Arbiter Reputation System
- **Timeout Counter**: Tracks missed deadlines per arbiter via `get_arbiter_timeout_count()`
- **Automatic Increment**: Counter increases each time arbiter times out
- **Integration Ready**: Designed to integrate with arbiter pool system (#151)

## New Storage Keys
- `DefaultDisputeWinner`: Global default winner configuration
- `DisputeDeadlineStart(u32)`: Timestamp when dispute deadline starts
- `ArbiterTimeoutCount(Address)`: Per-arbiter timeout counter

## New Events
- `DisputeTimedOut`: Emitted when timeout is enforced
- `ArbitersTimeoutPenaltyApplied`: Emitted when arbiter timeout counter increments

## API Functions

### Admin Functions
```rust
pub fn set_default_dispute_winner(env: Env, admin: Address, winner: DisputeDefaultWinner)
pub fn get_default_dispute_winner(env: Env) -> DisputeDefaultWinner
pub fn update_default_dispute_timeout(env: Env, admin: Address, timeout_seconds: u64)
pub fn get_default_dispute_timeout(env: Env) -> u64
```

### Public Functions
```rust
pub fn enforce_dispute_timeout(env: Env, escrow_id: u32)
pub fn get_arbiter_timeout_count(env: Env, arbiter: Address) -> u32
```

### Enhanced Escrow Creation
```rust
// Per-escrow timeout override
pub fn create_escrow_w_timeout(
    env: Env,
    buyer: Address,
    seller: Address,
    arbiter: Address,
    amount: i128,
    token: Address,
    deadline: u64,
    metadata_hash: Option<BytesN<32>>,
    sellers: Vec<(Address, u32)>,
    renewal_count: u32,
    dispute_timeout_seconds: u64,
) -> u32

// Per-escrow default winner override via EscrowCreateRequest.dispute_default_winner
pub fn create_escrow_v2(env: Env, buyer: Address, request: EscrowCreateRequest) -> u32
```

## Implementation Details

### Data Types
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum DisputeDefaultWinner {
    Buyer = 0,
    Seller = 1,
}
```

### Enhanced Structures
- `EscrowCreateRequest`: Added `dispute_default_winner: Option<u32>` field
- `EscrowExtensions`: Added `dispute_default_winner: Option<u32>` field

### Timeout Logic
1. **Deadline Start**: Recorded when `dispute_escrow()` is called
2. **Effective Timeout**: Uses per-escrow override or global default
3. **Deadline Check**: `current_time - deadline_start >= effective_timeout`
4. **Winner Determination**: Uses per-escrow override or global default
5. **Fund Release**: Transfers to winner (Buyer → Refunded, Seller → Released)
6. **Reputation Update**: Increments arbiter timeout counter

## Test Coverage
Comprehensive test suite with 11 test cases covering:

### Core Functionality
- ✅ `test_arbiter_resolves_before_timeout`: Normal resolution prevents timeout
- ✅ `test_enforce_dispute_timeout_buyer_default`: Buyer-favored timeout
- ✅ `test_enforce_dispute_timeout_seller_default`: Seller-favored timeout
- ✅ `test_partial_dispute_timeout`: Partial dispute timeout handling

### Edge Cases
- ✅ `test_enforce_timeout_before_deadline`: Prevents premature timeout
- ✅ `test_enforce_timeout_on_non_disputed_escrow`: Requires active dispute
- ✅ `test_enforce_timeout_on_resolved_dispute`: Prevents double resolution

### Configuration
- ✅ `test_per_escrow_timeout_override`: Custom per-escrow timeouts
- ✅ `test_get_set_default_dispute_winner`: Global winner configuration
- ✅ `test_update_default_dispute_timeout`: Global timeout configuration

### Reputation System
- ✅ `test_arbiter_timeout_counter_increments`: Arbiter penalty tracking

## Acceptance Criteria Status
- ✅ **Deadline and default winner configurable globally and per-escrow at creation**
- ✅ **`enforce_dispute_timeout` callable by anyone after deadline elapses**
- ✅ **Arbiter timeout counter incremented on missed deadline**
- ✅ **Funds released correctly to configured default winner**
- ✅ **Tests: arbiter resolves before deadline (no timeout), timeout enforced, buyer-default and seller-default configurations**

## Integration Notes
- **Backward Compatible**: All existing functionality preserved
- **Event Integration**: New events follow existing pattern
- **Storage Efficient**: Minimal storage overhead
- **Gas Optimized**: Efficient timeout checking and enforcement
- **Future Ready**: Designed for arbiter pool integration

## Security Considerations
- **Access Control**: Admin-only configuration functions
- **Validation**: Proper input validation and error handling
- **Reentrancy Safe**: No external calls during critical sections
- **Overflow Protection**: Safe arithmetic operations
- **State Consistency**: Atomic state updates

The implementation successfully addresses the motivation of preventing funds from being locked indefinitely due to inactive arbiters while providing predictable outcomes and strong incentives for prompt arbiter action.