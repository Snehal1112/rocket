//! The four small YAML lists of the team network, kept under `~/.rocket-api/network/`.
//!
//! Machine-local files: never inside a workspace or collection and never committed. A missing
//! or blank file is an empty list. A damaged file is renamed to `<name>.corrupt-<unix-secs>`,
//! the list starts empty and the file name is remembered once for `take_quarantined()`.
//!
//! `YamlFile` is the shared file handling. `YamlListRepo` is the shared list logic (list, get,
//! remove, upsert with a limit). The three list repos are thin wrappers over it, and the
//! settings repo uses `YamlFile` alone.

use std::fs;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use rocket_network::device::DeviceId;
use rocket_network::limits::{MAX_MANUAL_ADDRESSES, MAX_PAIRED_DEVICES};
use rocket_network::ports::{
    BlockedDeviceRepository, ManualAddressRepository, NetworkSettingsRepository,
    PairedDeviceRepository, QuarantineReport,
};
use rocket_network::records::{BlockedDevice, ManualAddress, NetworkSettings, PairedDevice};
use rocket_shared::error::{DomainError, DomainResult};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::atomic_write;
use crate::network::lock::lock_ignore_poison;
use crate::yaml_io::quarantine_corrupt;

/// One YAML file with a lock, atomic writes and quarantine of damaged content.
///
/// Reads take the lock too, so two readers can never both try to set a damaged file aside.
struct YamlFile {
    path: PathBuf,
    file_name: String,
    write_lock: Mutex<()>,
    quarantined: Mutex<Option<String>>,
}

impl YamlFile {
    fn new(path: PathBuf) -> Self {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "network file".to_string());
        Self {
            path,
            file_name,
            write_lock: Mutex::new(()),
            quarantined: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        lock_ignore_poison(&self.write_lock)
    }

    fn read<T: DeserializeOwned + Default>(&self) -> DomainResult<T> {
        let _guard = self.lock();
        self.load()
    }

    /// Loads the value, lets `change` edit it and writes it back when `change` says it changed.
    fn modify<T, R>(
        &self,
        change: impl FnOnce(&mut T) -> DomainResult<(R, bool)>,
    ) -> DomainResult<R>
    where
        T: DeserializeOwned + Default + Serialize,
    {
        let _guard = self.lock();
        let mut value = self.load::<T>()?;
        let (out, changed) = change(&mut value)?;
        if changed {
            self.store(&value)?;
        }
        Ok(out)
    }

    fn take_quarantined(&self) -> Option<String> {
        lock_ignore_poison(&self.quarantined).take()
    }

    fn load<T: DeserializeOwned + Default>(&self) -> DomainResult<T> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
            // Bytes that are not UTF-8 count as a damaged file.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                self.quarantine()?;
                return Ok(T::default());
            }
            Err(e) => {
                return Err(DomainError::Io(format!(
                    "Failed to read {}: {e}",
                    self.file_name
                )))
            }
        };
        if text.trim().is_empty() {
            return Ok(T::default());
        }
        match serde_yaml::from_str::<T>(&text) {
            Ok(value) => Ok(value),
            Err(_) => {
                self.quarantine()?;
                Ok(T::default())
            }
        }
    }

    fn store<T: Serialize>(&self, value: &T) -> DomainResult<()> {
        let yaml = serde_yaml::to_string(value).map_err(|e| {
            DomainError::Serialization(format!("Failed to serialize {}: {e}", self.file_name))
        })?;
        atomic_write(&self.path, yaml.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write {}: {e}", self.file_name)))
    }

    /// Moves the damaged file aside (the shared `quarantine_corrupt` helper) and remembers its name.
    fn quarantine(&self) -> DomainResult<()> {
        quarantine_corrupt(&self.path)
            .map_err(|e| DomainError::Io(format!("Failed to set {} aside: {e}", self.file_name)))?;
        tracing::warn!(file = %self.file_name, "network file was damaged and has been set aside");
        *lock_ignore_poison(&self.quarantined) = Some(self.file_name.clone());
        Ok(())
    }
}

/// An item of a list file. The key decides which entry an upsert replaces.
trait Keyed {
    type Key: PartialEq;

    fn key(&self) -> Self::Key;
}

/// The private file shape of one list, for example `devices: [...]`.
trait ListFile: Default + Serialize + DeserializeOwned {
    type Item: Keyed + Clone;

    fn into_items(self) -> Vec<Self::Item>;
    fn items_mut(&mut self) -> &mut Vec<Self::Item>;
}

/// The key type of the items of list file `F`.
type KeyOf<F> = <<F as ListFile>::Item as Keyed>::Key;

/// A list kept in one YAML file: the logic the three list repos share.
struct YamlListRepo<F: ListFile> {
    file: YamlFile,
    shape: PhantomData<fn() -> F>,
}

impl<F: ListFile> YamlListRepo<F> {
    fn new(path: PathBuf) -> Self {
        Self {
            file: YamlFile::new(path),
            shape: PhantomData,
        }
    }

    fn list(&self) -> DomainResult<Vec<F::Item>> {
        Ok(self.file.read::<F>()?.into_items())
    }

    fn get(&self, key: &KeyOf<F>) -> DomainResult<Option<F::Item>> {
        Ok(self.list()?.into_iter().find(|item| item.key() == *key))
    }

    fn contains(&self, key: &KeyOf<F>) -> DomainResult<bool> {
        Ok(self.list()?.iter().any(|item| item.key() == *key))
    }

    /// Replaces the entry with the same key in place, or appends a new one.
    fn upsert(&self, item: F::Item) -> DomainResult<()> {
        self.upsert_with_limit(item, usize::MAX, "")
    }

    /// Like `upsert`, but a new entry beyond `max` entries is refused with `full_message`.
    /// Replacing an existing entry is always allowed.
    fn upsert_with_limit(&self, item: F::Item, max: usize, full_message: &str) -> DomainResult<()> {
        let key = item.key();
        self.file.modify(|file: &mut F| {
            let items = file.items_mut();
            match items.iter().position(|existing| existing.key() == key) {
                Some(index) => items[index] = item,
                None => {
                    if items.len() >= max {
                        return Err(DomainError::InvalidInput(full_message.to_string()));
                    }
                    items.push(item);
                }
            }
            Ok(((), true))
        })
    }

    /// Appends `item` unless an entry with its key exists. Returns whether it was added.
    /// A new entry beyond `max` entries is refused with `full_message`.
    fn insert_if_absent(
        &self,
        item: F::Item,
        max: usize,
        full_message: &str,
    ) -> DomainResult<bool> {
        let key = item.key();
        self.file.modify(|file: &mut F| {
            let items = file.items_mut();
            if items.iter().any(|existing| existing.key() == key) {
                return Ok((false, false));
            }
            if items.len() >= max {
                return Err(DomainError::InvalidInput(full_message.to_string()));
            }
            items.push(item);
            Ok((true, true))
        })
    }

    fn take_quarantined(&self) -> Option<String> {
        self.file.take_quarantined()
    }

    /// Returns whether an entry was removed.
    fn remove_by_key(&self, key: &KeyOf<F>) -> DomainResult<bool> {
        self.file.modify(|file: &mut F| {
            let items = file.items_mut();
            let before = items.len();
            items.retain(|item| item.key() != *key);
            let removed = items.len() != before;
            Ok((removed, removed))
        })
    }
}

/// One `QuarantineReport` impl for every repo. Each repo keeps its storage in a field named
/// `inner` (a `YamlListRepo` or a `YamlFile`) that has a `take_quarantined()` method.
macro_rules! quarantine_report {
    ($($repo:ty),+ $(,)?) => {
        $(
            impl QuarantineReport for $repo {
                fn take_quarantined(&self) -> Option<String> {
                    self.inner.take_quarantined()
                }
            }
        )+
    };
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PairedFile {
    #[serde(default)]
    devices: Vec<PairedDevice>,
}

impl Keyed for PairedDevice {
    type Key = DeviceId;

    fn key(&self) -> DeviceId {
        self.device_id
    }
}

impl ListFile for PairedFile {
    type Item = PairedDevice;

    fn into_items(self) -> Vec<PairedDevice> {
        self.devices
    }

    fn items_mut(&mut self) -> &mut Vec<PairedDevice> {
        &mut self.devices
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BlockedFile {
    #[serde(default)]
    devices: Vec<BlockedDevice>,
}

impl Keyed for BlockedDevice {
    type Key = DeviceId;

    fn key(&self) -> DeviceId {
        self.device_id
    }
}

impl ListFile for BlockedFile {
    type Item = BlockedDevice;

    fn into_items(self) -> Vec<BlockedDevice> {
        self.devices
    }

    fn items_mut(&mut self) -> &mut Vec<BlockedDevice> {
        &mut self.devices
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AddressFile {
    #[serde(default)]
    addresses: Vec<ManualAddress>,
}

/// Hosts compare without case and without surrounding spaces.
fn normalize_host(host: &str) -> String {
    host.trim().to_lowercase()
}

impl Keyed for ManualAddress {
    type Key = (String, u16);

    fn key(&self) -> (String, u16) {
        (normalize_host(&self.host), self.port)
    }
}

impl ListFile for AddressFile {
    type Item = ManualAddress;

    fn into_items(self) -> Vec<ManualAddress> {
        self.addresses
    }

    fn items_mut(&mut self) -> &mut Vec<ManualAddress> {
        &mut self.addresses
    }
}

/// `paired.yml`: the devices this node has paired with.
pub struct FsPairedDeviceRepo {
    inner: YamlListRepo<PairedFile>,
}

impl FsPairedDeviceRepo {
    /// `path` is the YAML file itself, for example `<data dir>/network/paired.yml`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            inner: YamlListRepo::new(path),
        }
    }
}

impl PairedDeviceRepository for FsPairedDeviceRepo {
    fn list(&self) -> DomainResult<Vec<PairedDevice>> {
        self.inner.list()
    }

    fn get(&self, id: &DeviceId) -> DomainResult<Option<PairedDevice>> {
        self.inner.get(id)
    }

    fn upsert(&self, device: PairedDevice) -> DomainResult<()> {
        let full = format!(
            "You've reached the limit of {MAX_PAIRED_DEVICES} paired devices. Remove one to pair."
        );
        self.inner
            .upsert_with_limit(device, MAX_PAIRED_DEVICES, &full)
    }

    fn remove(&self, id: &DeviceId) -> DomainResult<bool> {
        self.inner.remove_by_key(id)
    }
}

/// `blocked.yml`: devices that are refused until the user unblocks them.
pub struct FsBlockedDeviceRepo {
    inner: YamlListRepo<BlockedFile>,
}

impl FsBlockedDeviceRepo {
    /// `path` is the YAML file itself, for example `<data dir>/network/blocked.yml`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            inner: YamlListRepo::new(path),
        }
    }
}

impl BlockedDeviceRepository for FsBlockedDeviceRepo {
    fn list(&self) -> DomainResult<Vec<BlockedDevice>> {
        self.inner.list()
    }

    fn contains(&self, id: &DeviceId) -> DomainResult<bool> {
        self.inner.contains(id)
    }

    fn add(&self, device: BlockedDevice) -> DomainResult<()> {
        self.inner.upsert(device)
    }

    fn remove(&self, id: &DeviceId) -> DomainResult<bool> {
        self.inner.remove_by_key(id)
    }
}

/// `addresses.yml`: addresses the user added by hand ("Add by address").
pub struct FsManualAddressRepo {
    inner: YamlListRepo<AddressFile>,
}

impl FsManualAddressRepo {
    /// `path` is the YAML file itself, for example `<data dir>/network/addresses.yml`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            inner: YamlListRepo::new(path),
        }
    }
}

impl ManualAddressRepository for FsManualAddressRepo {
    fn list(&self) -> DomainResult<Vec<ManualAddress>> {
        self.inner.list()
    }

    fn add(&self, address: ManualAddress) -> DomainResult<()> {
        let host = normalize_host(&address.host);
        if host.is_empty() {
            return Err(DomainError::InvalidInput(
                "The address needs a host".to_string(),
            ));
        }
        let full = format!(
            "You've reached the limit of {MAX_MANUAL_ADDRESSES} addresses. Remove one to add another."
        );
        // A known address keeps its first entry, so the first `added_at` stays.
        self.inner.insert_if_absent(
            ManualAddress { host, ..address },
            MAX_MANUAL_ADDRESSES,
            &full,
        )?;
        Ok(())
    }

    fn remove(&self, host: &str, port: u16) -> DomainResult<bool> {
        self.inner.remove_by_key(&(normalize_host(host), port))
    }
}

/// `settings.yml`: visibility, display name and the intro flag.
pub struct FsNetworkSettingsRepo {
    // The settings file is one map, not a list, so it uses `YamlFile` alone.
    inner: YamlFile,
}

impl FsNetworkSettingsRepo {
    /// `path` is the YAML file itself, for example `<data dir>/network/settings.yml`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            inner: YamlFile::new(path),
        }
    }
}

impl NetworkSettingsRepository for FsNetworkSettingsRepo {
    fn load(&self) -> DomainResult<NetworkSettings> {
        self.inner.read::<NetworkSettings>()
    }

    fn save(&self, settings: &NetworkSettings) -> DomainResult<()> {
        self.inner.modify(|current: &mut NetworkSettings| {
            *current = settings.clone();
            Ok(((), true))
        })
    }
}

quarantine_report!(
    FsPairedDeviceRepo,
    FsBlockedDeviceRepo,
    FsManualAddressRepo,
    FsNetworkSettingsRepo,
);

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::net::SocketAddr;
    use tempfile::TempDir;

    fn id(n: u16) -> DeviceId {
        let mut bytes = [0u8; 32];
        bytes[..2].copy_from_slice(&n.to_be_bytes());
        DeviceId::from_bytes(bytes)
    }

    fn paired(n: u16) -> PairedDevice {
        PairedDevice {
            device_id: id(n),
            name_at_pairing: format!("Device {n}"),
            paired_at: Utc
                .with_ymd_and_hms(2026, 11, 1, 10, 0, 0)
                .single()
                .expect("date"),
            last_addresses: vec!["192.168.1.20:47821".parse::<SocketAddr>().expect("addr")],
        }
    }

    fn blocked(n: u16) -> BlockedDevice {
        BlockedDevice {
            device_id: id(n),
            name: format!("Blocked {n}"),
            blocked_at: Utc
                .with_ymd_and_hms(2026, 11, 2, 9, 0, 0)
                .single()
                .expect("date"),
        }
    }

    fn address(host: &str, port: u16, hour: u32) -> ManualAddress {
        ManualAddress {
            host: host.to_string(),
            port,
            added_at: Utc
                .with_ymd_and_hms(2026, 11, 3, hour, 0, 0)
                .single()
                .expect("date"),
        }
    }

    fn corrupt_files(dir: &TempDir, name: &str) -> Vec<std::path::PathBuf> {
        let prefix = format!("{name}.corrupt-");
        let mut found: Vec<_> = fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().starts_with(&prefix))
                    .unwrap_or(false)
            })
            .collect();
        found.sort();
        found
    }

    fn paired_repo() -> (TempDir, FsPairedDeviceRepo) {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsPairedDeviceRepo::new(dir.path().join("paired.yml"));
        (dir, repo)
    }

    // ---- the generic list helper ----

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Note {
        id: u32,
        text: String,
    }

    impl Keyed for Note {
        type Key = u32;

        fn key(&self) -> u32 {
            self.id
        }
    }

    #[derive(Debug, Default, Serialize, Deserialize)]
    struct NoteFile {
        #[serde(default)]
        notes: Vec<Note>,
    }

    impl ListFile for NoteFile {
        type Item = Note;

        fn into_items(self) -> Vec<Note> {
            self.notes
        }

        fn items_mut(&mut self) -> &mut Vec<Note> {
            &mut self.notes
        }
    }

    fn note(id: u32, text: &str) -> Note {
        Note {
            id,
            text: text.to_string(),
        }
    }

    fn note_repo() -> (TempDir, YamlListRepo<NoteFile>) {
        let dir = TempDir::new().expect("tempdir");
        let repo = YamlListRepo::new(dir.path().join("notes.yml"));
        (dir, repo)
    }

    #[test]
    fn the_list_helper_upsert_replaces_in_place_and_appends() {
        let (_dir, repo) = note_repo();
        repo.upsert(note(1, "one")).expect("upsert 1");
        repo.upsert(note(2, "two")).expect("upsert 2");
        repo.upsert(note(1, "uno")).expect("replace 1");
        assert_eq!(
            repo.list().expect("list"),
            vec![note(1, "uno"), note(2, "two")]
        );
        assert_eq!(repo.get(&2).expect("get"), Some(note(2, "two")));
        assert_eq!(repo.get(&9).expect("get a miss"), None);
        assert!(repo.contains(&1).expect("contains"));
        assert!(!repo.contains(&9).expect("contains a miss"));
    }

    #[test]
    fn the_list_helper_enforces_the_limit_but_allows_replacing() {
        let (_dir, repo) = note_repo();
        repo.upsert_with_limit(note(1, "one"), 2, "full")
            .expect("first");
        repo.upsert_with_limit(note(2, "two"), 2, "full")
            .expect("second");
        let err = repo
            .upsert_with_limit(note(3, "three"), 2, "the list is full")
            .expect_err("the third is refused");
        assert!(matches!(&err, DomainError::InvalidInput(m) if m == "the list is full"));
        repo.upsert_with_limit(note(2, "deux"), 2, "full")
            .expect("replacing at the limit is fine");
        assert_eq!(
            repo.list().expect("list"),
            vec![note(1, "one"), note(2, "deux")]
        );
    }

    #[test]
    fn the_list_helper_insert_if_absent_keeps_the_first_entry_and_obeys_the_limit() {
        let (_dir, repo) = note_repo();
        assert!(repo
            .insert_if_absent(note(1, "one"), 2, "full")
            .expect("add"));
        assert!(!repo
            .insert_if_absent(note(1, "other"), 2, "full")
            .expect("known"));
        assert!(repo
            .insert_if_absent(note(2, "two"), 2, "full")
            .expect("add 2"));
        let err = repo
            .insert_if_absent(note(3, "three"), 2, "full")
            .expect_err("the third is refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(!repo
            .insert_if_absent(note(2, "again"), 2, "full")
            .expect("known at the limit"));
        assert_eq!(
            repo.list().expect("list"),
            vec![note(1, "one"), note(2, "two")]
        );
    }

    #[test]
    fn the_list_helper_remove_by_key_reports_a_miss() {
        let (_dir, repo) = note_repo();
        repo.upsert(note(1, "one")).expect("upsert");
        repo.upsert(note(2, "two")).expect("upsert");
        assert!(repo.remove_by_key(&1).expect("remove"));
        assert!(!repo.remove_by_key(&1).expect("remove twice"));
        assert_eq!(repo.list().expect("list"), vec![note(2, "two")]);
    }

    #[test]
    fn the_list_helper_quarantines_a_damaged_file_once() {
        let (dir, repo) = note_repo();
        fs::write(dir.path().join("notes.yml"), "notes: nope").expect("write");
        assert!(repo.list().expect("list starts empty").is_empty());
        assert_eq!(repo.take_quarantined().as_deref(), Some("notes.yml"));
        assert_eq!(repo.take_quarantined(), None);
        assert!(repo.list().expect("list again").is_empty());
        assert_eq!(repo.take_quarantined(), None);
        assert_eq!(corrupt_files(&dir, "notes.yml").len(), 1);
        repo.upsert(note(1, "one"))
            .expect("upsert after quarantine");
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    // ---- paired ----

    #[test]
    fn a_missing_file_is_empty_and_creates_nothing() {
        let (dir, repo) = paired_repo();
        assert!(repo.list().expect("list").is_empty());
        assert_eq!(repo.get(&id(1)).expect("get"), None);
        assert!(!repo.remove(&id(1)).expect("remove"));
        assert!(!dir.path().join("paired.yml").exists());
        assert_eq!(repo.take_quarantined(), None);
    }

    #[test]
    fn a_blank_file_is_empty_and_not_quarantined() {
        let (dir, repo) = paired_repo();
        fs::write(dir.path().join("paired.yml"), "  \n\n").expect("write");
        assert!(repo.list().expect("list").is_empty());
        assert_eq!(repo.take_quarantined(), None);
        assert!(corrupt_files(&dir, "paired.yml").is_empty());
        assert!(dir.path().join("paired.yml").exists());
    }

    #[test]
    fn paired_devices_round_trip_and_survive_a_new_instance() {
        let (dir, repo) = paired_repo();
        repo.upsert(paired(1)).expect("upsert 1");
        repo.upsert(paired(2)).expect("upsert 2");
        let again = FsPairedDeviceRepo::new(dir.path().join("paired.yml"));
        let listed = again.list().expect("list");
        assert_eq!(listed, vec![paired(1), paired(2)]);
        assert_eq!(again.get(&id(2)).expect("get"), Some(paired(2)));
        assert!(again.remove(&id(1)).expect("remove"));
        assert!(!again.remove(&id(1)).expect("remove twice"));
        assert_eq!(again.list().expect("list"), vec![paired(2)]);
    }

    #[test]
    fn upsert_replaces_the_same_device_in_place() {
        let (_dir, repo) = paired_repo();
        repo.upsert(paired(1)).expect("upsert 1");
        repo.upsert(paired(2)).expect("upsert 2");
        let mut renamed = paired(1);
        renamed.name_at_pairing = "New name".to_string();
        repo.upsert(renamed.clone()).expect("replace 1");
        assert_eq!(repo.list().expect("list"), vec![renamed, paired(2)]);
    }

    #[test]
    fn a_paired_entry_without_last_addresses_still_loads() {
        let (dir, repo) = paired_repo();
        let text = format!(
            "devices:\n- device_id: {}\n  name_at_pairing: Ada\n  paired_at: 2026-11-01T10:00:00Z\n",
            id(7).to_base32()
        );
        fs::write(dir.path().join("paired.yml"), text).expect("write");
        let listed = repo.list().expect("list");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].last_addresses.is_empty());
        assert_eq!(listed[0].name_at_pairing, "Ada");
    }

    #[test]
    fn the_paired_list_stops_at_the_limit_but_allows_updates() {
        let (dir, repo) = paired_repo();
        let file = PairedFile {
            devices: (0..MAX_PAIRED_DEVICES as u16).map(paired).collect(),
        };
        let yaml = serde_yaml::to_string(&file).expect("yaml");
        atomic_write(&dir.path().join("paired.yml"), yaml.as_bytes()).expect("seed");

        let err = repo
            .upsert(paired(MAX_PAIRED_DEVICES as u16))
            .expect_err("the 201st device is refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(err.to_string().contains("limit of 200"));
        assert_eq!(repo.list().expect("list").len(), MAX_PAIRED_DEVICES);

        let mut renamed = paired(5);
        renamed.name_at_pairing = "Renamed".to_string();
        repo.upsert(renamed.clone())
            .expect("an existing device can change");
        assert_eq!(repo.get(&id(5)).expect("get"), Some(renamed));
        assert_eq!(repo.list().expect("list").len(), MAX_PAIRED_DEVICES);
    }

    #[test]
    fn a_damaged_file_is_set_aside_once_and_reported_once() {
        let cases: [(&str, &[u8]); 3] = [
            ("not yaml", b": : [ unbalanced"),
            ("wrong shape", b"devices: 5\n"),
            ("not utf8", &[0xff, 0xfe, 0x00, 0x80]),
        ];
        for (label, bytes) in cases {
            let (dir, repo) = paired_repo();
            let path = dir.path().join("paired.yml");
            fs::write(&path, bytes).expect("write damaged file");

            assert!(
                repo.list().expect("list starts empty").is_empty(),
                "{label}"
            );
            assert!(!path.exists(), "{label}: the damaged file moved away");
            let aside = corrupt_files(&dir, "paired.yml");
            assert_eq!(aside.len(), 1, "{label}");
            assert_eq!(fs::read(&aside[0]).expect("read aside"), bytes, "{label}");
            assert_eq!(
                repo.take_quarantined().as_deref(),
                Some("paired.yml"),
                "{label}"
            );
            assert_eq!(repo.take_quarantined(), None, "{label}: reported once");

            // Reading again does not quarantine again, and a write makes a clean file.
            assert!(repo.list().expect("list again").is_empty());
            assert_eq!(repo.take_quarantined(), None);
            repo.upsert(paired(1)).expect("upsert after quarantine");
            assert_eq!(repo.list().expect("list").len(), 1);
            assert_eq!(corrupt_files(&dir, "paired.yml").len(), 1);
        }
    }

    #[test]
    fn concurrent_upserts_all_land() {
        let (_dir, repo) = paired_repo();
        let repo = std::sync::Arc::new(repo);
        let handles: Vec<_> = (0..16u16)
            .map(|n| {
                let repo = repo.clone();
                std::thread::spawn(move || repo.upsert(paired(n)).expect("upsert"))
            })
            .collect();
        for handle in handles {
            handle.join().expect("thread");
        }
        assert_eq!(repo.list().expect("list").len(), 16);
    }

    // ---- blocked ----

    #[test]
    fn blocked_devices_add_contain_remove_and_persist() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("blocked.yml");
        let repo = FsBlockedDeviceRepo::new(path.clone());
        assert!(!repo.contains(&id(1)).expect("contains"));
        repo.add(blocked(1)).expect("add");
        repo.add(blocked(1)).expect("add the same device again");
        repo.add(blocked(2)).expect("add 2");
        let again = FsBlockedDeviceRepo::new(path);
        assert_eq!(again.list().expect("list"), vec![blocked(1), blocked(2)]);
        assert!(again.contains(&id(2)).expect("contains"));
        assert!(again.remove(&id(2)).expect("remove"));
        assert!(!again.remove(&id(2)).expect("remove twice"));
        assert!(!again.contains(&id(2)).expect("contains after remove"));
    }

    #[test]
    fn a_damaged_blocked_file_is_quarantined_and_named() {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsBlockedDeviceRepo::new(dir.path().join("blocked.yml"));
        fs::write(dir.path().join("blocked.yml"), "devices: [1, 2").expect("write");
        assert!(repo.list().expect("list").is_empty());
        assert_eq!(repo.take_quarantined().as_deref(), Some("blocked.yml"));
        assert_eq!(corrupt_files(&dir, "blocked.yml").len(), 1);
    }

    // ---- manual addresses ----

    fn address_repo() -> (TempDir, FsManualAddressRepo) {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsManualAddressRepo::new(dir.path().join("addresses.yml"));
        (dir, repo)
    }

    #[test]
    fn manual_addresses_dedupe_on_lowercased_host_and_port() {
        let (_dir, repo) = address_repo();
        repo.add(address("Host.Example", 4000, 8)).expect("add");
        repo.add(address("host.example", 4000, 9))
            .expect("add the same address");
        repo.add(address("  HOST.EXAMPLE ", 4000, 10))
            .expect("add with spaces");
        let listed = repo.list().expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].host, "host.example");
        assert_eq!(
            listed[0].added_at,
            address("x", 1, 8).added_at,
            "the first entry stays"
        );
        repo.add(address("host.example", 4001, 11))
            .expect("another port");
        repo.add(address("other.example", 4000, 12))
            .expect("another host");
        assert_eq!(repo.list().expect("list").len(), 3);
    }

    #[test]
    fn manual_addresses_remove_ignores_case_and_reports_a_miss() {
        let (_dir, repo) = address_repo();
        repo.add(address("10.0.0.5", 47821, 8)).expect("add");
        repo.add(address("Laptop.Local", 47821, 9)).expect("add");
        assert!(repo.remove("LAPTOP.local", 47821).expect("remove"));
        assert!(!repo.remove("laptop.local", 47821).expect("remove twice"));
        assert!(!repo.remove("10.0.0.5", 1).expect("wrong port"));
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    #[test]
    fn an_empty_host_is_refused() {
        let (_dir, repo) = address_repo();
        let err = repo.add(address("   ", 47821, 8)).expect_err("blank host");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(repo.list().expect("list").is_empty());
    }

    #[test]
    fn manual_addresses_stop_at_the_limit() {
        let (dir, repo) = address_repo();
        let file = AddressFile {
            addresses: (0..MAX_MANUAL_ADDRESSES as u16)
                .map(|n| address(&format!("host{n}.example"), 47821, 8))
                .collect(),
        };
        let yaml = serde_yaml::to_string(&file).expect("yaml");
        atomic_write(&dir.path().join("addresses.yml"), yaml.as_bytes()).expect("seed");

        let err = repo
            .add(address("one-more.example", 47821, 9))
            .expect_err("the 51st address is refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        repo.add(address("HOST3.example", 47821, 9))
            .expect("a duplicate is not a new entry");
        assert_eq!(repo.list().expect("list").len(), MAX_MANUAL_ADDRESSES);
    }

    #[test]
    fn a_damaged_address_file_is_quarantined() {
        let (dir, repo) = address_repo();
        fs::write(dir.path().join("addresses.yml"), "addresses: nope").expect("write");
        assert!(repo.list().expect("list").is_empty());
        assert_eq!(repo.take_quarantined().as_deref(), Some("addresses.yml"));
        repo.add(address("a.example", 1, 8))
            .expect("add after quarantine");
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    // ---- settings ----

    #[test]
    fn settings_default_when_missing_and_round_trip() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("settings.yml");
        let repo = FsNetworkSettingsRepo::new(path.clone());
        assert_eq!(repo.load().expect("load"), NetworkSettings::default());
        let settings = NetworkSettings {
            hidden: true,
            display_name: Some("Ada".to_string()),
            intro_seen: true,
        };
        repo.save(&settings).expect("save");
        let again = FsNetworkSettingsRepo::new(path);
        assert_eq!(again.load().expect("load"), settings);
    }

    #[test]
    fn settings_fill_missing_fields_with_defaults() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("settings.yml"), "hidden: true\n").expect("write");
        let repo = FsNetworkSettingsRepo::new(dir.path().join("settings.yml"));
        let loaded = repo.load().expect("load");
        assert!(loaded.hidden);
        assert_eq!(loaded.display_name, None);
        assert!(!loaded.intro_seen);
    }

    #[test]
    fn a_damaged_settings_file_gives_defaults_and_is_quarantined() {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsNetworkSettingsRepo::new(dir.path().join("settings.yml"));
        fs::write(dir.path().join("settings.yml"), "hidden: [maybe").expect("write");
        assert_eq!(repo.load().expect("load"), NetworkSettings::default());
        assert_eq!(repo.take_quarantined().as_deref(), Some("settings.yml"));
        let settings = NetworkSettings {
            hidden: true,
            ..NetworkSettings::default()
        };
        repo.save(&settings).expect("save after quarantine");
        assert_eq!(repo.load().expect("load"), settings);
        assert_eq!(corrupt_files(&dir, "settings.yml").len(), 1);
    }

    #[test]
    fn every_repo_is_a_quarantine_report() {
        let dir = TempDir::new().expect("tempdir");
        let a = FsPairedDeviceRepo::new(dir.path().join("paired.yml"));
        let b = FsBlockedDeviceRepo::new(dir.path().join("blocked.yml"));
        let c = FsManualAddressRepo::new(dir.path().join("addresses.yml"));
        let d = FsNetworkSettingsRepo::new(dir.path().join("settings.yml"));
        let reports: Vec<&dyn QuarantineReport> = vec![&a, &b, &c, &d];
        assert!(reports.iter().all(|r| r.take_quarantined().is_none()));
    }
}
