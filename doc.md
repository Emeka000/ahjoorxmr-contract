# Code Notes / Audit – Current State vs Requested Features

## Repository overview

This workspace contains multiple Soroban contracts under `contracts/`.

## 1) “Payments Payment Link Generation with On-Chain Shareable Metadata” (ahjoor-payments)

**Requested feature (summary):**

- Merchant creates an on-chain payment link via `create_payment_link(token, amount, description, max_uses, expiry_ledger)`.
- Link code is an 8-byte deterministic short code derived from merchant address + nonce + ledger.
- Customer pays via `pay_via_link(link_code)` which:
  - Resolves the stored link record,
  - Creates a standard on-chain payment record,
  - Transfers funds atomically from customer to merchant.
- Enforce:
  - `max_uses` with exhausted status,
  - expiry ledger with `LinkExpired`,
  - merchant cancellation via `cancel_payment_link(link_code)`.
- Events expected:
  - `PaymentLinkCreated`, `PaymentLinkRedeemed`, `PaymentLinkCancelled`.
- Tests expected:
  - full redemption,
  - max-uses exhaustion,
  - expiry,
  - cancellation,
  - duplicate redemption guard.

**Actual implementation status (code audit performed):**

- In `contracts/ahjoor-payments/src/lib.rs`:
  - No payment-link storage/record struct exists.
  - No methods exist matching:
    - `create_payment_link`
    - `pay_via_link`
    - `cancel_payment_link`
  - No link-code redemption counters / max-uses enforcement logic exists.
- In `contracts/ahjoor-payments/src/events.rs`:
  - No events matching `PaymentLinkCreated`, `PaymentLinkRedeemed`, `PaymentLinkCancelled` exist.

**Conclusion:**

- `ahjoor-payments` currently **does not support** the requested payment-link feature.
- Implementing it would require adding:
  - new `#[contracttype]` structs (PaymentLink record, link status enum, etc.),
  - new storage keys (mapping link_code → PaymentLink + redemption counters/state),
  - new public/external methods,
  - new error variants,
  - new events,
  - and a test suite.

## 2) “On-Chain Immutable Refund History” (ahjoor-refund)

**Requested note (from previous truncated discussion):**

- There should be a customer-facing, on-chain, immutable refund state history (append-only), including history entry cap/limit (20 entries) and a public read function like `get_refund_history(refund_id)`.

**Actual implementation status (code audit performed):**

- Refund lifecycle / status transitions exist (refund request/approval/rejection/processing/appeal, etc.).
- However, in the reviewed code:
  - No append-only refund history storage exists.
  - No `RefundHistoryEntry` or equivalent type exists.
  - No public API for retrieving refund history exists.

**Conclusion:**

- `ahjoor-refund` contains refund status logic but **does not** contain the requested per-refund immutable history log.
- Implementing it would require code additions in `contracts/ahjoor-refund/src/lib.rs`, likely `events.rs`, and tests.

## Important constraint respected

- No code changes were made.
- No tests were executed.

## Next steps (recommended)

1. Create an implementation plan at contract/file granularity for `ahjoor-payments` and `ahjoor-refund`.
2. Add minimal deterministic link-code derivation spec (what nonce means; whether it’s user-supplied or contract-generated; link-code collision rules).
3. Decide atomicity boundary:
   - `pay_via_link` should both create the payment record and transfer funds in the same call (or match existing escrow patterns).
4. Add full unit tests for link creation/redemption/cancellation/expiry.
5. Add refund history append-only storage with strict cap and verify gas/storage behavior.
