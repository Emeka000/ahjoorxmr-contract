use crate::{events, ContributionEntry, CycleRecord, DataKey2};
use soroban_sdk::{Address, Env, Map, Vec};

const PERSISTENT_LIFETIME_THRESHOLD: u32 = 100_000;
const PERSISTENT_BUMP_AMOUNT: u32 = 120_000;
const TEMP_LIFETIME_THRESHOLD: u32 = 10_000;
const TEMP_BUMP_AMOUNT: u32 = 15_000;

/// Default retention window: keep 100 cycles in persistent storage
const DEFAULT_RETENTION_WINDOW: u32 = 100;

/// Records a complete cycle audit trail atomically at round closure.
/// This captures all significant events: contributions, payouts, defaults, skips, and penalties.
pub(crate) fn record_cycle_audit(
    env: &Env,
    cycle_number: u32,
    total_pool_amount: i128,
    payout_recipient: Address,
    payout_amount: i128,
    contributions: Vec<ContributionEntry>,
    defaulters: Vec<Address>,
    skippers: Vec<Address>,
    penalties_collected: i128,
    fee_collected: i128,
    insurance_drawn: i128,
    cycle_start_timestamp: u64,
    cycle_end_timestamp: u64,
) {
    let record = CycleRecord {
        cycle_number,
        total_pool_amount,
        payout_recipient: payout_recipient.clone(),
        payout_amount,
        contributions,
        defaulters,
        skippers,
        penalties_collected,
        fee_collected,
        insurance_drawn,
        cycle_start_timestamp,
        cycle_end_timestamp,
    };

    // Store in persistent storage
    let mut cycle_records: Map<u32, CycleRecord> = env
        .storage()
        .persistent()
        .get(&DataKey2::CycleRecords)
        .unwrap_or(Map::new(env));

    cycle_records.set(cycle_number, record);
    env.storage()
        .persistent()
        .set(&DataKey2::CycleRecords, &cycle_records);
    env.storage().persistent().extend_ttl(
        &DataKey2::CycleRecords,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );

    events::emit_cycle_record_created(env, cycle_number, total_pool_amount, payout_recipient);

    // Check if archival is needed
    archive_old_records(env, cycle_number);
}

/// Archives old cycle records to temporary storage based on retention window.
/// Records older than the retention window are moved from persistent to temporary storage.
fn archive_old_records(env: &Env, current_cycle: u32) {
    let retention_window: u32 = env
        .storage()
        .persistent()
        .get(&DataKey2::CycleRecordRetentionWindow)
        .unwrap_or(DEFAULT_RETENTION_WINDOW);

    if current_cycle <= retention_window {
        return; // Not enough cycles to archive yet
    }

    let archive_threshold = current_cycle - retention_window;

    let mut cycle_records: Map<u32, CycleRecord> = env
        .storage()
        .persistent()
        .get(&DataKey2::CycleRecords)
        .unwrap_or(Map::new(env));

    let mut archived_records: Map<u32, CycleRecord> = env
        .storage()
        .temporary()
        .get(&DataKey2::ArchivedCycleRecords)
        .unwrap_or(Map::new(env));

    // Find records to archive
    let mut cycles_to_archive = Vec::new(env);
    for (cycle_num, _) in cycle_records.iter() {
        if cycle_num < archive_threshold {
            cycles_to_archive.push_back(cycle_num);
        }
    }

    // Move records to temporary storage
    for cycle_num in cycles_to_archive.iter() {
        if let Some(record) = cycle_records.get(cycle_num) {
            archived_records.set(cycle_num, record);
            cycle_records.remove(cycle_num);
            events::emit_cycle_record_archived(env, cycle_num);
        }
    }

    // Update storage
    if !cycles_to_archive.is_empty() {
        env.storage()
            .persistent()
            .set(&DataKey::DataKey2::DataKey2::DataKey2::CycleRecords, &cycle_records);
        env.storage()
            .temporary()
            .set(&DataKey::DataKey2::DataKey2::DataKey2::ArchivedCycleRecords, &archived_records);
        env.storage().temporary().extend_ttl(
            &DataKey::DataKey2::DataKey2::DataKey2::ArchivedCycleRecords,
            TEMP_LIFETIME_THRESHOLD,
            TEMP_BUMP_AMOUNT,
        );
    }
}

/// Retrieves a cycle record from either persistent or archived storage.
pub(crate) fn get_cycle_record(env: &Env, cycle_number: u32) -> Option<CycleRecord> {
    // First check persistent storage
    let cycle_records: Map<u32, CycleRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::DataKey2::DataKey2::DataKey2::CycleRecords)
        .unwrap_or(Map::new(env));

    if let Some(record) = cycle_records.get(cycle_number) {
        return Some(record);
    }

    // Then check archived storage
    let archived_records: Map<u32, CycleRecord> = env
        .storage()
        .temporary()
        .get(&DataKey::DataKey2::DataKey2::DataKey2::ArchivedCycleRecords)
        .unwrap_or(Map::new(env));

    archived_records.get(cycle_number)
}

/// Returns all contribution entries for a specific member across all cycles.
pub(crate) fn get_member_contribution_history(
    env: &Env,
    member: Address,
) -> Vec<ContributionEntry> {
    let mut history = Vec::new(env);

    // Check persistent storage
    let cycle_records: Map<u32, CycleRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::DataKey2::DataKey2::DataKey2::CycleRecords)
        .unwrap_or(Map::new(env));

    for (_, record) in cycle_records.iter() {
        for contribution in record.contributions.iter() {
            if contribution.member == member {
                history.push_back(contribution);
            }
        }
    }

    // Check archived storage
    let archived_records: Map<u32, CycleRecord> = env
        .storage()
        .temporary()
        .get(&DataKey::DataKey2::DataKey2::DataKey2::ArchivedCycleRecords)
        .unwrap_or(Map::new(env));

    for (_, record) in archived_records.iter() {
        for contribution in record.contributions.iter() {
            if contribution.member == member {
                history.push_back(contribution);
            }
        }
    }

    history
}

/// Updates the retention window for cycle records. Admin only.
pub(crate) fn set_retention_window(env: &Env, new_window: u32) {
    let old_window: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::DataKey2::DataKey2::DataKey2::CycleRecordRetentionWindow)
        .unwrap_or(DEFAULT_RETENTION_WINDOW);

    env.storage()
        .persistent()
        .set(&DataKey::DataKey2::DataKey2::DataKey2::CycleRecordRetentionWindow, &new_window);
    env.storage().persistent().extend_ttl(
        &DataKey::DataKey2::DataKey2::DataKey2::CycleRecordRetentionWindow,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );

    events::emit_retention_window_updated(env, old_window, new_window);
}

/// Gets the current retention window setting.
pub(crate) fn get_retention_window(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::DataKey2::DataKey2::DataKey2::CycleRecordRetentionWindow)
        .unwrap_or(DEFAULT_RETENTION_WINDOW)
}

/// Records the start timestamp for a cycle.
pub(crate) fn record_cycle_start(env: &Env, cycle_number: u32, timestamp: u64) {
    let mut timestamps: Map<u32, u64> = env
        .storage()
        .instance()
        .get(&DataKey::DataKey2::DataKey2::DataKey2::CycleStartTimestamps)
        .unwrap_or(Map::new(env));

    timestamps.set(cycle_number, timestamp);
    env.storage()
        .instance()
        .set(&DataKey::DataKey2::DataKey2::DataKey2::CycleStartTimestamps, &timestamps);
}

/// Gets the start timestamp for a cycle.
pub(crate) fn get_cycle_start_timestamp(env: &Env, cycle_number: u32) -> u64 {
    let timestamps: Map<u32, u64> = env
        .storage()
        .instance()
        .get(&DataKey::DataKey2::DataKey2::DataKey2::CycleStartTimestamps)
        .unwrap_or(Map::new(env));

    timestamps.get(cycle_number).unwrap_or(0)
}
