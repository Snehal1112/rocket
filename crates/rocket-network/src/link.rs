//! Link types: ids, the manifest and the link record. Pure: no I/O.

use std::collections::BTreeMap;

use rand::RngCore;
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::device::DeviceId;
use crate::limits::{MAX_MANIFEST_ENTRIES, MAX_TRANSFER_BYTES};

const PATH_MAX_BYTES: usize = 1024;

/// Whether the text is exactly 32 lowercase hex characters.
fn is_hex32(text: &str) -> bool {
    text.len() == 32
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// A new random 128-bit value as 32 lowercase hex characters.
fn random_hex32() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Defines an id type: 32 lowercase hex characters (a random 128-bit value), not secret.
macro_rules! hex_id {
    ($(#[$meta:meta])* $name:ident, $what:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// A new random id.
            pub fn random() -> $name {
                $name(random_hex32())
            }

            /// Accepts exactly 32 lowercase hex characters.
            pub fn parse(text: &str) -> DomainResult<$name> {
                if is_hex32(text) {
                    Ok($name(text.to_string()))
                } else {
                    Err(DomainError::InvalidInput(concat!("invalid ", $what).to_string()))
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = DomainError;

            fn try_from(text: String) -> DomainResult<$name> {
                $name::parse(&text)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }
    };
}

hex_id!(
    /// Joins two linked copies of a collection. Made by the sender at the first send.
    LinkId,
    "link id"
);
hex_id!(
    /// One sync run. Made by the node that starts it.
    SyncId,
    "sync id"
);

/// Whether the text is a lowercase hex SHA-256 (64 characters).
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_device_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or("").to_ascii_lowercase();
    let numbered = (stem.starts_with("com") || stem.starts_with("lpt"))
        && matches!(stem.as_bytes(), [_, _, _, b'1'..=b'9']);
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul") || numbered
}

/// Whether a collection-relative path is safe: `/` separated, inside the root, no `..`, no
/// `.rocket` folder, no device names, no drive letters, no control characters.
pub fn path_is_valid(path: &str) -> bool {
    if path.is_empty() || path.len() > PATH_MAX_BYTES || path.starts_with('/') {
        return false;
    }
    if path
        .chars()
        .any(|c| c.is_control() || c == '\\' || c == ':')
    {
        return false;
    }
    path.split('/').all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && !segment.eq_ignore_ascii_case(".rocket")
            && !segment.ends_with('.')
            && !segment.ends_with(' ')
            && !is_device_name(segment)
    })
}

/// Relative path (`/` separated) to the lowercase hex SHA-256 of the exported file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest(BTreeMap<String, String>);

impl Manifest {
    pub fn new() -> Manifest {
        Manifest(BTreeMap::new())
    }
    pub fn insert(&mut self, path: &str, sha256: &str) -> DomainResult<()> {
        if !path_is_valid(path) {
            return Err(DomainError::InvalidInput("invalid manifest path".into()));
        }
        if !is_sha256_hex(sha256) {
            return Err(DomainError::InvalidInput("invalid manifest hash".into()));
        }
        if !self.0.contains_key(path) && self.0.len() >= MAX_MANIFEST_ENTRIES {
            return Err(DomainError::InvalidInput(
                "manifest has too many entries".into(),
            ));
        }
        self.0.insert(path.to_string(), sha256.to_string());
        Ok(())
    }
    pub fn get(&self, path: &str) -> Option<&str> {
        self.0.get(path).map(String::as_str)
    }
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn base_hash(&self) -> String {
        let mut hasher = Sha256::new();
        for (path, hash) in &self.0 {
            hasher.update(path.as_bytes());
            hasher.update([0u8]);
            hasher.update(hash.as_bytes());
            hasher.update(b"\n");
        }
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
    fn from_checked(entries: BTreeMap<String, String>) -> DomainResult<Manifest> {
        let mut manifest = Manifest::new();
        for (path, hash) in &entries {
            manifest.insert(path, hash)?;
        }
        Ok(manifest)
    }
    pub fn to_json(&self) -> DomainResult<Vec<u8>> {
        let file = ManifestFile {
            entries: self
                .0
                .iter()
                .map(|(p, h)| ManifestEntry {
                    path: p.clone(),
                    sha256: h.clone(),
                })
                .collect(),
        };
        serde_json::to_vec(&file)
            .map_err(|_| DomainError::InvalidInput("manifest cannot be encoded".into()))
    }
    pub fn from_json(bytes: &[u8]) -> DomainResult<Manifest> {
        if bytes.len() as u64 > MAX_TRANSFER_BYTES {
            return Err(DomainError::InvalidInput("manifest too large".into()));
        }
        let file: ManifestFile = serde_json::from_slice(bytes)
            .map_err(|_| DomainError::InvalidInput("invalid manifest".into()))?;
        let mut manifest = Manifest::new();
        for entry in &file.entries {
            if manifest.0.contains_key(&entry.path) {
                return Err(DomainError::InvalidInput("duplicate manifest path".into()));
            }
            manifest.insert(&entry.path, &entry.sha256)?;
        }
        Ok(manifest)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestEntry {
    path: String,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    entries: Vec<ManifestEntry>,
}

impl Serialize for Manifest {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}
impl<'de> Deserialize<'de> for Manifest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Manifest, D::Error> {
        let entries = BTreeMap::<String, String>::deserialize(d)?;
        Manifest::from_checked(entries).map_err(serde::de::Error::custom)
    }
}

/// What the user sees for a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    InSync,
    /// A local change since the base.
    ChangesWaiting,
    /// A sync failed after the peer applied.
    OutOfDate,
    Broken,
}

/// One link from a local collection to a collection on one paired node.
///
/// A persistence struct: snake_case keys, never sent on the wire. The base files live next to
/// it in the repository (`LinkRepository::save`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRecord {
    pub link_id: LinkId,
    pub peer_device_id: DeviceId,
    pub collection_name: String,
    /// Canonical path of the local collection folder.
    pub collection_root: String,
    /// The `uid` in the local `opencollection.yml`. A root that no longer holds it is broken.
    #[serde(default)]
    pub collection_uid: Option<String>,
    /// Path to hash as last agreed.
    pub base: Manifest,
    pub state: LinkState,
    /// RFC 3339.
    pub updated_at: String,
}

impl LinkRecord {
    /// The hash both sides compare at the start of a sync.
    pub fn base_hash(&self) -> String {
        self.base.base_hash()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn two() -> Manifest {
        let mut manifest = Manifest::new();
        manifest.insert("b/c.yml", B).expect("valid entry");
        manifest.insert("a.yml", A).expect("valid entry");
        manifest
    }

    #[test]
    fn ids_are_32_lowercase_hex_characters() {
        let link = LinkId::random();
        assert_eq!(link.as_str().len(), 32);
        assert_eq!(LinkId::parse(link.as_str()).expect("round trip"), link);
        assert_ne!(LinkId::random(), LinkId::random());
        let sync = SyncId::random();
        assert_eq!(SyncId::parse(sync.as_str()).expect("round trip"), sync);
        for bad in [
            "",
            "abc",
            &"A".repeat(32),
            &"g".repeat(32),
            &"a".repeat(31),
            &"a".repeat(33),
            &"é".repeat(16),
        ] {
            assert!(LinkId::parse(bad).is_err(), "link id {bad:?}");
            assert!(SyncId::parse(bad).is_err(), "sync id {bad:?}");
        }
    }

    #[test]
    fn ids_serialize_as_plain_strings_and_validate_on_read() {
        let id = LinkId::parse("0123456789abcdef0123456789abcdef").expect("valid");
        assert_eq!(
            serde_json::to_string(&id).expect("json"),
            "\"0123456789abcdef0123456789abcdef\""
        );
        assert!(serde_json::from_str::<LinkId>("\"nope\"").is_err());
        assert_eq!(
            serde_json::from_str::<LinkId>("\"0123456789abcdef0123456789abcdef\"").expect("valid"),
            id
        );
    }

    #[test]
    fn base_hash_is_pinned_and_ignores_insertion_order() {
        // sha256 of "a.yml\0<A>\nb/c.yml\0<B>\n".
        assert_eq!(
            two().base_hash(),
            "278204d7086284145bc980a5fa47f7bc6abfe3e0d4670ded3ec18e4b7aaa096d"
        );
        let mut one = Manifest::new();
        one.insert("a.yml", A).expect("valid entry");
        assert_eq!(
            one.base_hash(),
            "79aa69741931a903111f2314d0fe1be6fb11b748eb6be1564ec1205c740a2ae1"
        );
        // The empty manifest hashes the empty string.
        assert_eq!(
            Manifest::new().base_hash(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let mut other = Manifest::new();
        other.insert("a.yml", A).expect("valid entry");
        other.insert("b/c.yml", B).expect("valid entry");
        assert_eq!(other.base_hash(), two().base_hash());
    }

    #[test]
    fn base_hash_changes_with_a_path_or_a_hash() {
        let mut changed_hash = two();
        changed_hash.insert("a.yml", B).expect("valid entry");
        assert_ne!(changed_hash.base_hash(), two().base_hash());
        let mut renamed = Manifest::new();
        renamed.insert("a2.yml", A).expect("valid entry");
        renamed.insert("b/c.yml", B).expect("valid entry");
        assert_ne!(renamed.base_hash(), two().base_hash());
    }

    #[test]
    fn the_manifest_reads_back_what_was_inserted() {
        let manifest = two();
        assert_eq!(manifest.len(), 2);
        assert!(!manifest.is_empty());
        assert_eq!(manifest.get("a.yml"), Some(A));
        assert_eq!(manifest.get("missing.yml"), None);
        assert_eq!(manifest.paths().collect::<Vec<_>>(), ["a.yml", "b/c.yml"]);
        let mut replaced = manifest.clone();
        replaced
            .insert("a.yml", B)
            .expect("an existing path can be replaced");
        assert_eq!(replaced.len(), 2);
        assert_eq!(replaced.get("a.yml"), Some(B));
    }

    #[test]
    fn insert_accepts_ordinary_paths() {
        let mut manifest = Manifest::new();
        for path in [
            "opencollection.yml",
            "folder/request.yml",
            "a/b/c/d.yml",
            "My Folder/get users (v2).yml",
            "über/ünï.yml",
            "scripts/pre.js",
            ".gitignore",
            "console.yml",
            "com10.yml",
            "x/.rocketapi.yml",
        ] {
            manifest
                .insert(path, A)
                .unwrap_or_else(|e| panic!("{path} should be accepted: {e:?}"));
        }
    }

    #[test]
    fn insert_rejects_unsafe_paths() {
        let long = "a/".repeat(600);
        for path in [
            "",
            "/etc/passwd",
            "/a.yml",
            "../a.yml",
            "a/../b.yml",
            "a/..",
            "..",
            ".",
            "./a.yml",
            "a/./b.yml",
            "a//b.yml",
            "a/",
            "a\\b.yml",
            "..\\a.yml",
            "C:\\a.yml",
            "C:/a.yml",
            ".rocket/state.yml",
            "a/.rocket/state.yml",
            ".ROCKET/x.yml",
            "a/.rocket",
            "CON",
            "con.yml",
            "a/NUL.txt",
            "aux/x.yml",
            "COM1",
            "x/lpt9.yml",
            "name.",
            "name /x.yml",
            "a\u{0}b.yml",
            "a\nb.yml",
            "a\u{7f}b.yml",
            long.as_str(),
        ] {
            let err = Manifest::new().insert(path, A).expect_err(path);
            assert!(matches!(err, DomainError::InvalidInput(_)), "path {path:?}");
        }
    }

    #[test]
    fn insert_rejects_bad_hashes() {
        let upper = "A".repeat(64);
        let short = "a".repeat(63);
        let long = "a".repeat(65);
        let not_hex = "g".repeat(64);
        for hash in [
            "",
            upper.as_str(),
            short.as_str(),
            long.as_str(),
            not_hex.as_str(),
            "é",
        ] {
            assert!(
                Manifest::new().insert("a.yml", hash).is_err(),
                "hash {hash:?}"
            );
        }
    }

    #[test]
    fn a_manifest_holds_at_most_5000_entries() {
        let mut manifest = Manifest::new();
        for n in 0..MAX_MANIFEST_ENTRIES {
            manifest
                .insert(&format!("r{n}.yml"), A)
                .expect("within the cap");
        }
        assert_eq!(manifest.len(), 5000);
        assert!(manifest.insert("one-too-many.yml", A).is_err());
        assert_eq!(manifest.len(), 5000);
        // Replacing an existing path is not a new entry.
        manifest.insert("r0.yml", B).expect("replaces");
    }

    #[test]
    fn to_json_lists_entries_sorted_by_path() {
        let text = String::from_utf8(two().to_json().expect("encodes")).expect("utf8");
        assert_eq!(
            text,
            format!(
                r#"{{"entries":[{{"path":"a.yml","sha256":"{A}"}},{{"path":"b/c.yml","sha256":"{B}"}}]}}"#
            )
        );
        assert_eq!(
            Manifest::new().to_json().expect("encodes"),
            br#"{"entries":[]}"#.to_vec()
        );
    }

    #[test]
    fn json_round_trips() {
        let manifest = two();
        let back = Manifest::from_json(&manifest.to_json().expect("encodes")).expect("decodes");
        assert_eq!(back, manifest);
        assert_eq!(back.base_hash(), manifest.base_hash());
    }

    #[test]
    fn from_json_rejects_bad_input() {
        let entry = |path: &str, hash: &str| format!(r#"{{"path":"{path}","sha256":"{hash}"}}"#);
        let wrap = |entries: &[String]| format!(r#"{{"entries":[{}]}}"#, entries.join(","));
        let too_many: Vec<String> = (0..=MAX_MANIFEST_ENTRIES)
            .map(|n| entry(&format!("r{n}.yml"), A))
            .collect();
        for (name, text) in [
            ("not json", "nope".to_string()),
            ("wrong shape", "[]".to_string()),
            ("missing entries", "{}".to_string()),
            ("unknown field", r#"{"entries":[],"extra":1}"#.to_string()),
            (
                "unknown entry field",
                format!(r#"{{"entries":[{{"path":"a.yml","sha256":"{A}","x":1}}]}}"#),
            ),
            ("dot dot", wrap(&[entry("../a.yml", A)])),
            ("absolute", wrap(&[entry("/a.yml", A)])),
            ("backslash", wrap(&[entry("a\\\\b.yml", A)])),
            ("rocket folder", wrap(&[entry(".rocket/x.yml", A)])),
            ("device name", wrap(&[entry("nul.yml", A)])),
            ("bad hex", wrap(&[entry("a.yml", &"z".repeat(64))])),
            ("upper case hex", wrap(&[entry("a.yml", &"A".repeat(64))])),
            ("short hash", wrap(&[entry("a.yml", "abc")])),
            (
                "duplicate path",
                wrap(&[entry("a.yml", A), entry("a.yml", B)]),
            ),
            ("over the cap", wrap(&too_many)),
        ] {
            let err = Manifest::from_json(text.as_bytes()).expect_err(name);
            assert!(matches!(err, DomainError::InvalidInput(_)), "case {name}");
        }
    }

    #[test]
    fn from_json_accepts_exactly_the_cap() {
        let mut manifest = Manifest::new();
        for n in 0..MAX_MANIFEST_ENTRIES {
            manifest
                .insert(&format!("r{n}.yml"), A)
                .expect("within the cap");
        }
        let back = Manifest::from_json(&manifest.to_json().expect("encodes")).expect("decodes");
        assert_eq!(back.len(), 5000);
    }

    #[test]
    fn from_json_refuses_a_payload_over_the_transfer_cap_without_parsing() {
        let huge = vec![b' '; MAX_TRANSFER_BYTES as usize + 1];
        assert!(Manifest::from_json(&huge).is_err());
    }

    #[test]
    fn the_error_never_repeats_the_path() {
        let err = Manifest::new()
            .insert("../secret-token-123", A)
            .expect_err("rejected");
        match err {
            DomainError::InvalidInput(text) => assert!(!text.contains("secret-token")),
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn a_manifest_stored_as_a_map_is_validated_when_read() {
        let manifest = two();
        let text = serde_json::to_string(&manifest).expect("json");
        assert_eq!(text, format!(r#"{{"a.yml":"{A}","b/c.yml":"{B}"}}"#));
        assert_eq!(
            serde_json::from_str::<Manifest>(&text).expect("valid"),
            manifest
        );
        let bad = format!(r#"{{"../a.yml":"{A}"}}"#);
        assert!(serde_json::from_str::<Manifest>(&bad).is_err());
    }

    fn record() -> LinkRecord {
        LinkRecord {
            link_id: LinkId::parse("0123456789abcdef0123456789abcdef").expect("valid"),
            peer_device_id: DeviceId::from_bytes([7; 32]),
            collection_name: "Orders".into(),
            collection_root: "/home/ada/collections/orders".into(),
            collection_uid: Some("col-1".into()),
            base: two(),
            state: LinkState::ChangesWaiting,
            updated_at: "2026-11-08T10:00:00Z".into(),
        }
    }

    #[test]
    fn link_states_use_snake_case_names() {
        for (state, name) in [
            (LinkState::InSync, "in_sync"),
            (LinkState::ChangesWaiting, "changes_waiting"),
            (LinkState::OutOfDate, "out_of_date"),
            (LinkState::Broken, "broken"),
        ] {
            assert_eq!(
                serde_json::to_value(state).expect("json"),
                serde_json::json!(name)
            );
            let back: LinkState = serde_json::from_value(serde_json::json!(name)).expect("reads");
            assert_eq!(back, state);
        }
    }

    #[test]
    fn a_link_record_round_trips_with_snake_case_keys() {
        let record = record();
        let value = serde_json::to_value(&record).expect("json");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "base",
                "collection_name",
                "collection_root",
                "collection_uid",
                "link_id",
                "peer_device_id",
                "state",
                "updated_at"
            ]
        );
        assert_eq!(value["state"], "changes_waiting");
        assert_eq!(value["base"]["a.yml"], A);
        let back: LinkRecord = serde_json::from_value(value).expect("reads");
        assert_eq!(back, record);
    }

    #[test]
    fn a_record_without_a_uid_reads_back_with_none() {
        let mut value = serde_json::to_value(record()).expect("json");
        value
            .as_object_mut()
            .expect("an object")
            .remove("collection_uid");
        let back: LinkRecord = serde_json::from_value(value).expect("reads");
        assert_eq!(back.collection_uid, None);
    }

    #[test]
    fn a_record_with_an_unsafe_base_path_or_a_bad_id_is_refused_on_read() {
        let mut value = serde_json::to_value(record()).expect("json");
        value["base"] = serde_json::json!({ "../x.yml": A });
        assert!(serde_json::from_value::<LinkRecord>(value).is_err());
        let mut value = serde_json::to_value(record()).expect("json");
        value["link_id"] = serde_json::json!("not-an-id");
        assert!(serde_json::from_value::<LinkRecord>(value).is_err());
    }

    #[test]
    fn the_record_base_hash_is_the_hash_of_its_base() {
        assert_eq!(record().base_hash(), two().base_hash());
        assert_eq!(
            record().base_hash(),
            "278204d7086284145bc980a5fa47f7bc6abfe3e0d4670ded3ec18e4b7aaa096d"
        );
    }
}
