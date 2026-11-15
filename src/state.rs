//! In-memory state with an atomic JSON snapshot (docs/SPEC.md §5.6).
//!
//! Three collections live in memory — counterparty profiles, receipts, and
//! learned rules — and are persisted to `STATE_FILE` (default
//! `solv_state.json`) by a background worker: write to a `.tmp` sibling,
//! `fsync`, then `rename`, so an interrupted write can never leave a corrupt
//! file behind. The snapshot is triggered asynchronously after the response
//! ships, with a 2-second debounce, and never touches the hot path.
//!
//! Accepted limit (deliberate): the three collections are serialised without
//! a cross-collection transaction, so a snapshot can pair a fresh receipt
//! with a not-yet-updated profile if both change mid-serialisation. It
//! self-heals on the next mutation's snapshot. A decision writes only its
//! receipt (a receipt embeds the profile data it was made from), so the
//! pre-spend path never needs the transaction; the Reppo worker (PR-09),
//! which updates profiles, is where to revisit it.
//!
//! The snapshot file contains untrusted counterparty text and is never
//! committed (AGENTS.md hard prohibitions).

use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc};

/// Debounce window for asynchronous snapshots (SPEC §5.6).
pub const DEFAULT_SNAPSHOT_DEBOUNCE: Duration = Duration::from_secs(2);

/// Bumped when the on-disk snapshot layout changes in a way older code
/// cannot read; a snapshot with any other version is rejected on load.
const SCHEMA_VERSION: u32 = 1;

pub type CounterpartyId = String;
pub type ReceiptId = String;

/// A counterparty's profile.
///
/// `score` is `None` until a Reppo evaluation settles (UNRATED). The
/// effective score is recomputed from `settled_at` at read time, so it
/// ages and expires honestly (see `score::effective_score`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub counterparty_name: String,
    pub target_url: String,
    #[serde(default)]
    pub score: Option<crate::score::SettledScore>,
}

/// A decision receipt (SPEC §3.3 G4, §6.3, §7.3): the decision summary, the
/// evidence document with its SHA-256, and the response as it was returned.
/// Never altered after creation — `insert_receipt` refuses to overwrite.
///
/// `evidence` and `response` default to `null` so a snapshot from before
/// the receipt store still parses; the service never wrote receipts then.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub decision: String,
    pub amount_usd: f64,
    pub created_at_unix: u64,
    #[serde(default)]
    pub evidence_sha256: String,
    #[serde(default)]
    pub evidence: serde_json::Value,
    #[serde(default)]
    pub response: serde_json::Value,
}

/// A learned guardrail rule (SPEC §5.5). Full shape lands with rule
/// compilation; what the state layer needs to persist is identity and status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnedRule {
    pub rule_id: String,
    pub status: RuleStatus,
}

/// A rule is born `Shadow` and reaches `Active` only through operator
/// approval (SPEC §5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleStatus {
    Shadow,
    Active,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error on the state file: {0}")]
    Io(#[from] std::io::Error),
    #[error("state file is corrupt: {0}")]
    Corrupt(String),
}

/// Everything a snapshot contains on disk.
/// The three collections default to empty so a snapshot from a different
/// schema version parses far enough for the version check to reject it with
/// a precise message; `schema_version` itself stays mandatory. `BTreeMap`
/// makes the serialised key order deterministic, so identical state
/// produces byte-identical snapshots.
#[derive(Serialize, Deserialize)]
struct StateData {
    schema_version: u32,
    #[serde(default)]
    counterparties: BTreeMap<CounterpartyId, Profile>,
    #[serde(default)]
    receipts: BTreeMap<ReceiptId, Receipt>,
    #[serde(default)]
    learned_rules: Vec<LearnedRule>,
}

impl Default for StateData {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            counterparties: BTreeMap::new(),
            receipts: BTreeMap::new(),
            learned_rules: Vec::new(),
        }
    }
}

/// The in-memory store behind the whole service.
pub struct Store {
    counterparties: DashMap<CounterpartyId, Profile>,
    receipts: DashMap<ReceiptId, Receipt>,
    learned_rules: std::sync::RwLock<Vec<LearnedRule>>,
    path: PathBuf,
    debounce: Duration,
    snapshot_tx: mpsc::Sender<()>,
    snapshot_rx: Mutex<Option<mpsc::Receiver<()>>>,
    /// Serialises every snapshot write. The debounce worker and the
    /// shutdown flush are two independent callers; without this lock their
    /// writes interleave into the shared `.tmp` sibling and a mixed file
    /// gets renamed into place.
    write_mutex: std::sync::Mutex<()>,
}

impl Store {
    /// Opens the store, reloading state from `path` when a snapshot exists.
    ///
    /// A missing file is a fresh start; a corrupt or future-version file is
    /// an error — silently starting empty would lose profiles a payments
    /// product depends on.
    pub fn open(path: impl Into<PathBuf>) -> Result<Arc<Self>, StoreError> {
        Self::open_with_debounce(path, DEFAULT_SNAPSHOT_DEBOUNCE)
    }

    /// [`Store::open`] with a custom debounce window, for tests.
    pub fn open_with_debounce(
        path: impl Into<PathBuf>,
        debounce: Duration,
    ) -> Result<Arc<Self>, StoreError> {
        let path = path.into();
        let data = Self::load(&path)?;
        let (snapshot_tx, snapshot_rx) = mpsc::channel(1);
        Ok(Arc::new(Self {
            counterparties: data.counterparties.into_iter().collect(),
            receipts: data.receipts.into_iter().collect(),
            learned_rules: std::sync::RwLock::new(data.learned_rules),
            path,
            debounce,
            snapshot_tx,
            snapshot_rx: Mutex::new(Some(snapshot_rx)),
            write_mutex: std::sync::Mutex::new(()),
        }))
    }

    fn load(path: &Path) -> Result<StateData, StoreError> {
        match fs::read(path) {
            Ok(bytes) => {
                let data: StateData = serde_json::from_slice(&bytes)
                    .map_err(|error| StoreError::Corrupt(error.to_string()))?;
                if data.schema_version != SCHEMA_VERSION {
                    return Err(StoreError::Corrupt(format!(
                        "unsupported schema_version {} (expected {SCHEMA_VERSION})",
                        data.schema_version
                    )));
                }
                Ok(data)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(StateData::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// Inserts or replaces a counterparty profile.
    pub fn upsert_profile(&self, id: CounterpartyId, profile: Profile) {
        self.counterparties.insert(id, profile);
    }

    pub fn profile(&self, id: &str) -> Option<Profile> {
        self.counterparties
            .get(id)
            .map(|entry| entry.value().clone())
    }

    /// All profiles, sorted by id so the directory output is deterministic.
    pub fn list_profiles(&self) -> Vec<(CounterpartyId, Profile)> {
        let mut entries: Vec<_> = self
            .counterparties
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    /// Inserts a receipt, refusing to overwrite an existing id: receipts
    /// are never mutated after creation (SPEC §3.3 G4). Returns `false`
    /// and leaves the stored receipt untouched when `id` already exists.
    ///
    /// The result is `#[must_use]`: an ignored `false` would silently
    /// reintroduce the mutation this guard exists to prevent.
    #[must_use = "a false return means the receipt id already exists; the caller must handle it"]
    pub fn insert_receipt(&self, id: ReceiptId, receipt: Receipt) -> bool {
        use dashmap::mapref::entry::Entry;
        match self.receipts.entry(id) {
            Entry::Occupied(_) => false,
            Entry::Vacant(vacant) => {
                vacant.insert(receipt);
                true
            }
        }
    }

    pub fn receipt(&self, id: &str) -> Option<Receipt> {
        self.receipts.get(id).map(|entry| entry.value().clone())
    }

    // Lock poisoning is treated as unrecoverable: the critical sections are
    // trivial clones and assignments that cannot panic, so a poisoned lock
    // means memory corruption, not a recoverable state.
    pub fn learned_rules(&self) -> Vec<LearnedRule> {
        self.learned_rules
            .read()
            .expect("learned_rules lock")
            .clone()
    }

    pub fn set_learned_rules(&self, rules: Vec<LearnedRule>) {
        *self.learned_rules.write().expect("learned_rules lock") = rules;
    }

    /// Asks the snapshot worker to persist state after the debounce window.
    ///
    /// Never blocks and never allocates: the channel has capacity one, so a
    /// request while one is pending coalesces into it.
    pub fn request_snapshot(&self) {
        let _ = self.snapshot_tx.try_send(());
    }

    /// Serialises state and writes it atomically, now.
    ///
    /// Used by the background worker and by tests; the hot path uses
    /// [`Store::request_snapshot`] instead.
    pub fn write_snapshot_now(&self) -> Result<(), StoreError> {
        // Serialised across callers (see `write_mutex`): two concurrent
        // writers would interleave into the shared `.tmp` sibling and rename
        // a mixed file into place. Off the hot path, so holding this across
        // fsync is fine.
        let _guard = self.write_mutex.lock().expect("write_mutex lock");
        // Serialising these plain types cannot fail; a panic here (caught by
        // the worker's spawn_blocking) is more honest than a "corrupt file"
        // error that would send an operator digging through a healthy file.
        let bytes =
            serde_json::to_vec(&self.snapshot_data()).expect("state serialisation cannot fail");
        atomic_write(&self.path, &bytes)?;
        Ok(())
    }

    fn snapshot_data(&self) -> StateData {
        StateData {
            schema_version: SCHEMA_VERSION,
            counterparties: self.list_profiles().into_iter().collect(),
            receipts: self
                .receipts
                .iter()
                .map(|entry| (entry.key().clone(), entry.value().clone()))
                .collect(),
            learned_rules: self.learned_rules(),
        }
    }

    /// Spawns the background snapshot worker: wait for a request, let the
    /// debounce window absorb further mutations, then persist off the async
    /// runtime's threads. The worker holds the store weakly, so it exits
    /// once every strong reference is gone. Panics if called twice on one
    /// store.
    pub async fn spawn_snapshot_worker(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let weak = Arc::downgrade(self);
        let debounce = self.debounce;
        let mut receiver = self
            .snapshot_rx
            .lock()
            .await
            .take()
            .expect("snapshot worker already spawned");
        tokio::spawn(async move {
            loop {
                if receiver.recv().await.is_none() {
                    break; // sender dropped with the store; nothing to persist
                }
                tokio::time::sleep(debounce).await;
                while receiver.try_recv().is_ok() {
                    // Coalesce requests that arrived during the window.
                }
                let Some(store) = weak.upgrade() else {
                    break; // store dropped while we slept
                };
                let result = tokio::task::spawn_blocking(move || store.write_snapshot_now()).await;
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => eprintln!("solv: snapshot write failed: {error}"),
                    Err(join_error) => {
                        eprintln!("solv: snapshot writer panicked: {join_error}")
                    }
                }
            }
        })
    }
}

/// Writes `bytes` to `path` atomically: create the `.tmp` sibling, write,
/// `fsync`, `rename` over the destination, then best-effort fsync of the
/// parent directory so the rename itself survives a power loss.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_path(path);
    {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    sync_parent_dir(path);
    Ok(())
}

/// `solv_state.json` -> `solv_state.json.tmp` (the name `.gitignore` knows).
fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    {
        if let Some(dir) = path.parent() {
            if let Ok(dir_file) = File::open(dir) {
                let _ = dir_file.sync_all();
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_path_appends_the_tmp_suffix() {
        assert_eq!(
            tmp_path(Path::new("solv_state.json")),
            PathBuf::from("solv_state.json.tmp")
        );
    }

    #[test]
    fn default_state_data_carries_the_schema_version() {
        let data = StateData::default();
        assert_eq!(data.schema_version, SCHEMA_VERSION);
        let bytes = serde_json::to_vec(&data).unwrap();
        assert!(
            std::str::from_utf8(&bytes)
                .unwrap()
                .contains("\"schema_version\":1")
        );
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert!(!dir.path().join("state.json.tmp").exists());
    }
}
