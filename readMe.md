# Ahjoor - Decentralized ROSCA Platform

**Ahjoor** is a decentralized Rotating Savings and Credit Association (ROSCA) platform built on the Stellar blockchain. It empowers communities and savings groups to pool funds and take turns receiving the collective pot with complete transparency, security, and no middlemen.

## Overview

ROSCAs are one of the oldest and most widely used savings systems in the world, yet they still rely entirely on trust and manual processes. Ahjoor brings this tradition on-chain using Stellar's fast, low-cost blockchain infrastructure to provide:

- **Trustless Savings Circles**: Automated round management with no central authority
- **Transparent Operations**: All participants can verify contributions and payouts on-chain
- **Secure Funds**: Cryptographically secured group wallets and contribution records
- **Cost-Effective**: Built on Stellar's efficient, low-fee blockchain infrastructure
- **Scalable**: Designed to support many groups running simultaneously

## Use Cases

- Community savings circles (Ajo, Esusu, Susu, Tanda, Chit Funds)
- Corporate employee savings programs
- Diaspora remittance and group savings
- Micro-lending and credit-building for underbanked communities
- Multi-party escrow and collective fund management

## Quick Start

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [Stellar CLI](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup)
- Make (optional, for convenience commands)

### Installation

```bash
# Fork the repository
# Then clone your fork into your local environment
git clone https://github.com/Ahjoor/ahjoor-contract.git
cd ahjoor-contracts

# Add wasm32 target
rustup target add wasm32-unknown-unknown
```

### Build

```bash
# Using Make
make build
```

OR

```bash
# Using cargo
cargo build --target wasm32-unknown-unknown --release
```

```bash
# Or directly with Stellar CLI
stellar contract build
```

### Test

```bash
# Run all tests
make test
```

### Format & Lint

```bash
# Format code
make fmt
```

```bash
# OR
cargo fmt
```

```bash
# Check formatting
make fmt-check
```

```bash
# OR
cargo check --all
```

```bash
# Run clippy lints
make lint
```

## Architecture

Ahjoor's smart contracts handle:

- **Group Management**: Create and manage multiple independent ROSCA groups
- **Access Control**: Only pre-registered participants can contribute to a group
- **Contribution Tracking**: Immutable record of who has paid each round
- **Automated Payouts**: Scheduled recipients claim the full pot when their round is due
- **Round Progression**: Time-based round advancement using Stellar ledger timestamps

## Technology Stack

- **Blockchain**: Stellar (Soroban smart contracts)
- **Language**: Rust
- **SDK**: Soroban SDK v21.0.0
- **Token Standard**: SEP-41 / Stellar Asset Contract (XLM or any compatible token)
- **Testing**: Soroban test utilities

## Contributing

We welcome contributions! Please see our [CONTRIBUTING.md](CONTRIBUTING.md) for details on:

- Setting up your development environment
- Code style and standards
- Testing requirements
- Submitting pull requests

## Resources

- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Smart Contracts](https://soroban.stellar.org/)
- [Stellar CLI Reference](https://developers.stellar.org/docs/tools/developer-tools)

  ## Community

- [Telegram Group Chat](https://t.me/ahjoor)
