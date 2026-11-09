//! The file merge of a sync (spec decision 11 of the sync design).

use std::cell::Cell;
use std::fmt;

use rocket_shared::error::{DomainError, DomainResult};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_yaml::{Mapping, Number, Value};

use crate::bruno::export_transform;

/// The smallest budget for a received file, in bytes.
const MIN_BUDGET: u64 = 1024 * 1024;
/// The budget cost of one node, so many tiny aliased nodes cannot grow without a bound.
const NODE_COST: u64 = 16;

/// The collection root file. Its `info.name` is local-only for the sync.
pub const ROOT_FILE: &str = "opencollection.yml";

/// True when a collection-relative path names the root collection file.
pub fn is_root_file(path: &str) -> bool {
    path.eq_ignore_ascii_case(ROOT_FILE)
}

/// The bytes of one exported file as the sync hashes and sends them. The root file loses
/// `info.name`, so each node keeps its own collection name. Any other file is unchanged.
pub fn sync_bytes(path: &str, bytes: Vec<u8>) -> DomainResult<Vec<u8>> {
    if !is_root_file(path) {
        return Ok(bytes);
    }
    let mut value = parse_received(&bytes)
        .map_err(|_| DomainError::InvalidInput("The collection file is not valid YAML.".into()))?;
    let removed = match &mut value {
        Value::Mapping(map) => match map.get_mut("info") {
            Some(Value::Mapping(info)) => info.remove("name").is_some(),
            _ => false,
        },
        _ => false,
    };
    if removed {
        write_yaml(&value)
    } else {
        Ok(bytes)
    }
}

/// Merges a received (scrubbed) synced file into the local file at `path`. The root file
/// also keeps its local `info.name`.
pub fn merge_synced_file(
    path: &str,
    local: Option<&[u8]>,
    received: &[u8],
) -> DomainResult<Vec<u8>> {
    merge_with(local, received, is_root_file(path))
}

/// Merges a received (scrubbed) file into the local file. It is not the root file.
pub fn merge_file(local: Option<&[u8]>, received: &[u8]) -> DomainResult<Vec<u8>> {
    merge_with(local, received, false)
}

fn merge_with(local: Option<&[u8]>, received: &[u8], root_file: bool) -> DomainResult<Vec<u8>> {
    let received_value = parse_received(received)?;
    let Some(local) = local else {
        let stripped = take_received(&received_value, "", &mut KeyPath::new(root_file));
        if stripped == received_value {
            return Ok(received.to_vec());
        }
        return write_yaml(&stripped);
    };
    let local_value: Value = serde_yaml::from_slice(local)
        .map_err(|_| DomainError::InvalidInput("The local file is not valid YAML.".into()))?;
    if local_value.is_mapping() && !received_value.is_mapping() {
        return Err(DomainError::InvalidInput(
            "The received file is not shaped like the local one.".into(),
        ));
    }
    // The export blanks secrets. A local value is kept over a blank only where the export
    // would have blanked it, so a deliberate clear by the peer still lands.
    let mut scrubbed = local_value.clone();
    export_transform(&mut scrubbed);
    let merged = merge_value(
        &local_value,
        &received_value,
        Some(&scrubbed),
        &mut KeyPath::new(root_file),
    );
    write_yaml(&merged)
}

fn write_yaml(value: &Value) -> DomainResult<Vec<u8>> {
    serde_yaml::to_string(value)
        .map(String::into_bytes)
        .map_err(|_| DomainError::Internal("The merged file could not be written.".into()))
}

/// Parses a received file. It refuses tags and non-string keys, and it stops when aliases
/// expand to more than four times the input size (at least 1 MiB).
fn parse_received(received: &[u8]) -> DomainResult<Value> {
    let budget = Cell::new((received.len() as u64).saturating_mul(4).max(MIN_BUDGET));
    let seed = Budgeted { budget: &budget };
    seed.deserialize(serde_yaml::Deserializer::from_slice(received))
        .map_err(|_| DomainError::InvalidInput("The received file is not valid YAML.".into()))
}

#[derive(Clone, Copy)]
struct Budgeted<'a> {
    budget: &'a Cell<u64>,
}

impl Budgeted<'_> {
    fn charge<E: de::Error>(&self, bytes: u64) -> Result<(), E> {
        let left = self.budget.get();
        if bytes > left {
            return Err(E::custom("budget"));
        }
        self.budget.set(left - bytes);
        Ok(())
    }
}

impl<'de> DeserializeSeed<'de> for Budgeted<'_> {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Budgeted<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a plain YAML value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        self.charge(NODE_COST)?;
        Ok(Value::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        self.charge(NODE_COST)?;
        Ok(Value::Number(Number::from(v)))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        self.charge(NODE_COST)?;
        Ok(Value::Number(Number::from(v)))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        self.charge(NODE_COST)?;
        Ok(Value::Number(Number::from(v)))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        self.charge(NODE_COST + v.len() as u64)?;
        Ok(Value::String(v.to_string()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        self.charge(NODE_COST + v.len() as u64)?;
        Ok(Value::String(v))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        self.charge(NODE_COST)?;
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        self.visit_unit()
    }

    fn visit_some<D: de::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        self.deserialize(d)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        self.charge(NODE_COST)?;
        let mut out = Vec::new();
        while let Some(item) = seq.next_element_seed(self)? {
            out.push(item);
        }
        Ok(Value::Sequence(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        self.charge(NODE_COST)?;
        let mut out = Mapping::new();
        while let Some(key) = map.next_key_seed(self)? {
            if !matches!(key, Value::String(_)) {
                return Err(de::Error::custom("key"));
            }
            let value = map.next_value_seed(self)?;
            if out.insert(key, value).is_some() {
                return Err(de::Error::custom("duplicate"));
            }
        }
        Ok(Value::Mapping(out))
    }

    // A tagged value reaches this default and is refused: the export never writes tags.
    fn visit_enum<A: de::EnumAccess<'de>>(self, _data: A) -> Result<Value, A::Error> {
        Err(de::Error::custom("tag"))
    }
}

/// Merges one slot. `scrub` is the same slot of the local file after the export transform.
fn merge_value(
    local: &Value,
    received: &Value,
    scrub: Option<&Value>,
    path: &mut KeyPath,
) -> Value {
    match (local, received) {
        (Value::Mapping(l), Value::Mapping(r)) => {
            Value::Mapping(merge_map(l, r, scrub.and_then(Value::as_mapping), path))
        }
        (Value::Sequence(l), Value::Sequence(r)) => Value::Sequence(merge_rows(
            l,
            r,
            scrub.and_then(Value::as_sequence).map(Vec::as_slice),
            path,
        )),
        (Value::Mapping(l), r) => {
            // The local owned keys must not be lost to a non-mapping value.
            let kept = merge_map(l, &Mapping::new(), None, path);
            if kept.is_empty() {
                fallback(local, r, scrub, path)
            } else {
                Value::Mapping(kept)
            }
        }
        (l, r) => fallback(l, r, scrub, path),
    }
}

fn fallback(local: &Value, received: &Value, scrub: Option<&Value>, path: &mut KeyPath) -> Value {
    if is_blank(received) && is_filled_scalar(local) && scrub.is_some_and(is_blank) {
        local.clone()
    } else {
        take_received(received, "", path)
    }
}

fn is_blank(value: &Value) -> bool {
    matches!(value, Value::Null) || matches!(value, Value::String(s) if s.is_empty())
}

fn is_filled_scalar(value: &Value) -> bool {
    match value {
        Value::String(s) => !s.is_empty(),
        Value::Number(_) | Value::Bool(_) => true,
        _ => false,
    }
}

/// The keys above the merged slot, and whether the file is the root collection file.
struct KeyPath {
    keys: Vec<String>,
    root_file: bool,
}

impl KeyPath {
    fn new(root_file: bool) -> KeyPath {
        KeyPath {
            keys: Vec::new(),
            root_file,
        }
    }

    fn push(&mut self, key: String) {
        self.keys.push(key);
    }

    fn pop(&mut self) {
        self.keys.pop();
    }
}

/// The keys the export or the sync removes. They always come from the local file. The root
/// file's `info.name` is one of them, so a node's collection name never syncs.
fn is_local_owned(path: &KeyPath, key: &str) -> bool {
    let keys: Vec<&str> = path.keys.iter().map(String::as_str).collect();
    let root_name = path.root_file && keys.as_slice() == ["info"] && key == "name";
    root_name
        || matches!(
            (keys.as_slice(), key),
            ([], "uid") | (["info"], "uid") | (["extensions"], "rocketapi") | (_, "examples")
        )
}

fn merge_map(
    local: &Mapping,
    received: &Mapping,
    scrub: Option<&Mapping>,
    path: &mut KeyPath,
) -> Mapping {
    let mut out = Mapping::new();
    for (key, value) in received {
        let name = key.as_str().unwrap_or("");
        if is_local_owned(path, name) {
            continue;
        }
        let merged = match local.get(key) {
            Some(l) => {
                path.push(name.to_string());
                let v = merge_value(l, value, scrub.and_then(|m| m.get(key)), path);
                path.pop();
                v
            }
            None => take_received(value, name, path),
        };
        out.insert(key.clone(), merged);
    }
    for (key, value) in local {
        let name = key.as_str().unwrap_or("");
        let theirs = received.get(key);
        if is_local_owned(path, name) {
            out.insert(key.clone(), value.clone());
        } else if theirs.is_none() {
            // The peer dropped this key. Only owned keys deeper down are kept.
            if let Value::Mapping(inner) = value {
                path.push(name.to_string());
                let kept = merge_map(inner, &Mapping::new(), None, path);
                path.pop();
                if !kept.is_empty() {
                    out.insert(key.clone(), Value::Mapping(kept));
                }
            }
        }
    }
    out
}

/// Takes a received value that has no local twin. Received local-owned keys are dropped,
/// because the local file owns them even when it lacks the parent key.
fn take_received(value: &Value, name: &str, path: &mut KeyPath) -> Value {
    match value {
        Value::Mapping(map) => {
            let pushed = !name.is_empty();
            if pushed {
                path.push(name.to_string());
            }
            let kept = merge_map(&Mapping::new(), map, None, path);
            if pushed {
                path.pop();
            }
            Value::Mapping(kept)
        }
        Value::Sequence(rows) => Value::Sequence(merge_rows(&[], rows, None, path)),
        other => other.clone(),
    }
}

/// A row is identified by its name (or key) and its type.
fn row_id(row: &Mapping) -> Option<(String, Option<String>)> {
    let name = row
        .get("name")
        .or_else(|| row.get("key"))
        .and_then(Value::as_str)?;
    let kind = row.get("type").and_then(Value::as_str).map(str::to_string);
    Some((name.to_string(), kind))
}

fn merge_rows(
    local: &[Value],
    received: &[Value],
    scrub: Option<&[Value]>,
    path: &mut KeyPath,
) -> Vec<Value> {
    // The export keeps every row, so the scrubbed rows line up with the local ones.
    let scrub = scrub.filter(|rows| rows.len() == local.len());
    let mut used = vec![false; local.len()];
    let mut out = Vec::with_capacity(received.len());
    for row in received {
        let (Value::Mapping(r), Some(id)) = (row, row_of(row)) else {
            out.push(take_received(row, "", path));
            continue;
        };
        let found = local.iter().enumerate().find(|(i, l)| {
            !used[*i] && matches!(l, Value::Mapping(m) if row_id(m).as_ref() == Some(&id))
        });
        match found {
            Some((i, Value::Mapping(l))) => {
                used[i] = true;
                let scrub_row = scrub
                    .and_then(|rows| rows.get(i))
                    .and_then(Value::as_mapping);
                out.push(Value::Mapping(merge_map(l, r, scrub_row, path)));
            }
            _ => out.push(take_received(row, "", path)),
        }
    }
    out
}

fn row_of(value: &Value) -> Option<(String, Option<String>)> {
    match value {
        Value::Mapping(m) => row_id(m),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bruno::bruno_zip;
    use crate::ports::SourceFile;
    use crate::sync_zip::zip_entries;
    use serde_yaml::Value;
    use std::io::Cursor;

    fn parse(bytes: &[u8]) -> Value {
        serde_yaml::from_slice(bytes).expect("valid yaml")
    }

    fn merged(local: &str, received: &str) -> Value {
        let bytes = merge_file(Some(local.as_bytes()), received.as_bytes()).expect("merges");
        parse(&bytes)
    }

    /// The bytes the export puts in the ZIP for one file.
    fn exported(rel: &str, raw: &str) -> Vec<u8> {
        let file = SourceFile {
            rel_path: rel.to_string(),
            bytes: raw.as_bytes().to_vec(),
        };
        let (zip, _) = bruno_zip(vec![file], "c", false).expect("exports");
        zip_entries(Cursor::new(zip)).expect("entries").remove(0).1
    }

    #[test]
    fn a_new_file_is_written_as_received() {
        let received = b"info:\n  name: R\nhttp:\n  url: https://x\n";
        let out = merge_file(None, received).expect("merges");
        assert_eq!(out, received.to_vec());
    }

    #[test]
    fn a_new_file_that_is_not_yaml_is_refused() {
        let err = merge_file(None, b"a: [1, 2").expect_err("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn invalid_yaml_on_either_side_is_refused() {
        let ok = "a: 1\n";
        let bad = "a: [1, 2";
        let received_bad = merge_file(Some(ok.as_bytes()), bad.as_bytes()).expect_err("refused");
        let local_bad = merge_file(Some(bad.as_bytes()), ok.as_bytes()).expect_err("refused");
        assert!(matches!(received_bad, DomainError::InvalidInput(_)));
        assert!(matches!(local_bad, DomainError::InvalidInput(_)));
        // The message never quotes the file.
        assert!(!received_bad.to_string().contains("[1, 2"));
    }

    #[test]
    fn a_local_secret_survives_a_received_blank() {
        let local = "uid: r1\ninfo:\n  name: Login\nhttp:\n  url: https://old\n  headers:\n    - name: Authorization\n      value: Bearer abc\n    - name: Accept\n      value: text/plain\nauth:\n  type: bearer\n  token: s3cret\n";
        let received = "info:\n  name: Login\nhttp:\n  url: https://new\n  headers:\n    - name: Authorization\n      value: ''\n    - name: Accept\n      value: application/json\nauth:\n  type: bearer\n  token: ''\n";
        let v = merged(local, received);
        assert_eq!(v["http"]["url"].as_str(), Some("https://new"));
        assert_eq!(
            v["http"]["headers"][0]["value"].as_str(),
            Some("Bearer abc")
        );
        assert_eq!(
            v["http"]["headers"][1]["value"].as_str(),
            Some("application/json")
        );
        assert_eq!(v["auth"]["token"].as_str(), Some("s3cret"));
    }

    #[test]
    fn a_number_secret_survives_a_received_blank() {
        let v = merged("auth:\n  token: 123456\n", "auth:\n  token: ''\n");
        assert_eq!(v["auth"]["token"].as_u64(), Some(123456));
    }

    #[test]
    fn a_secret_variable_data_survives_a_blank() {
        let local = "variables:\n  - name: api\n    secret: true\n    value:\n      type: secret\n      data: abc\n";
        let received =
            "variables:\n  - name: api\n    secret: true\n    value:\n      type: secret\n      data: ''\n";
        let v = merged(local, received);
        assert_eq!(v["variables"][0]["value"]["data"].as_str(), Some("abc"));
    }

    #[test]
    fn local_only_keys_survive() {
        let local = "uid: top-uid\ninfo:\n  name: R\n  uid: info-uid\nextensions:\n  rocketapi:\n    sandboxMode: developer\n    agentAutonomyEnabled: true\n    scripts:\n      additionalContextRoots:\n        - /opt/lib\nexamples:\n  - name: ok\n    response:\n      status: 200\nhttp:\n  url: https://old\n";
        let received = "info:\n  name: R\nhttp:\n  url: https://new\n";
        let v = merged(local, received);
        assert_eq!(v["uid"].as_str(), Some("top-uid"));
        assert_eq!(v["info"]["uid"].as_str(), Some("info-uid"));
        let api = &v["extensions"]["rocketapi"];
        assert_eq!(api["sandboxMode"].as_str(), Some("developer"));
        assert_eq!(api["agentAutonomyEnabled"].as_bool(), Some(true));
        assert_eq!(
            api["scripts"]["additionalContextRoots"][0].as_str(),
            Some("/opt/lib")
        );
        assert_eq!(v["examples"][0]["name"].as_str(), Some("ok"));
        assert_eq!(v["http"]["url"].as_str(), Some("https://new"));
    }

    #[test]
    fn local_only_keys_survive_when_the_received_file_has_no_extensions_or_info() {
        let local = "info:\n  uid: i\nextensions:\n  rocketapi:\n    sandboxMode: developer\n";
        let v = merged(local, "http:\n  url: https://x\n");
        assert_eq!(v["info"]["uid"].as_str(), Some("i"));
        assert_eq!(
            v["extensions"]["rocketapi"]["sandboxMode"].as_str(),
            Some("developer")
        );
    }

    #[test]
    fn received_local_only_keys_are_ignored() {
        let local = "uid: good\ninfo:\n  name: R\nhttp:\n  url: https://a\n";
        let received = "uid: evil\ninfo:\n  name: R\n  uid: evil2\nextensions:\n  rocketapi:\n    sandboxMode: developer\n  bruno:\n    keep: 1\nexamples:\n  - name: x\nhttp:\n  url: https://b\n";
        let v = merged(local, received);
        assert_eq!(v["uid"].as_str(), Some("good"));
        assert!(v["info"]["uid"].is_null());
        assert!(v["extensions"]["rocketapi"].is_null());
        assert_eq!(v["extensions"]["bruno"]["keep"].as_u64(), Some(1));
        assert!(v["examples"].is_null());
        assert_eq!(v["http"]["url"].as_str(), Some("https://b"));
    }

    #[test]
    fn the_peers_value_wins_on_other_keys() {
        let local = "info:\n  name: R\n  seq: 1\nhttp:\n  method: GET\n  url: https://a\ndocs:\n  content: old docs\n";
        let received = "info:\n  name: R2\n  seq: 3\nhttp:\n  method: POST\n  url: https://b\n";
        let v = merged(local, received);
        assert_eq!(v["info"]["name"].as_str(), Some("R2"));
        assert_eq!(v["info"]["seq"].as_u64(), Some(3));
        assert_eq!(v["http"]["method"].as_str(), Some("POST"));
        assert_eq!(v["http"]["url"].as_str(), Some("https://b"));
        // The peer removed the key, so it goes.
        assert!(v["docs"].is_null());
    }

    #[test]
    fn lists_are_replaced_by_the_peers_list() {
        let local = "http:\n  headers:\n    - name: A\n      value: '1'\n    - name: B\n      value: '2'\n    - name: C\n      value: '3'\n";
        let received =
            "http:\n  headers:\n    - name: B\n      value: two\n    - name: D\n      value: '4'\n";
        let v = merged(local, received);
        let rows = v["http"]["headers"].as_sequence().expect("list");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"].as_str(), Some("B"));
        assert_eq!(rows[0]["value"].as_str(), Some("two"));
        assert_eq!(rows[1]["name"].as_str(), Some("D"));
        assert_eq!(rows[1]["value"].as_str(), Some("4"));
    }

    #[test]
    fn a_blank_row_value_keeps_the_local_value_of_the_row_with_the_same_name() {
        let local = "http:\n  headers:\n    - name: Authorization\n      value: tok\n    - name: X-Token\n      value: one\n    - name: X-Token\n      value: two\n    - name: Old\n      value: gone\n";
        let received = "http:\n  headers:\n    - name: X-Token\n      value: ''\n    - name: Authorization\n      value: ''\n    - name: X-Token\n      value: ''\n    - name: New\n      value: ''\n";
        let v = merged(local, received);
        let rows = v["http"]["headers"].as_sequence().expect("list");
        let got: Vec<(&str, &str)> = rows
            .iter()
            .map(|r| {
                (
                    r["name"].as_str().expect("name"),
                    r["value"].as_str().expect("value"),
                )
            })
            .collect();
        // The received order wins. Duplicates pair up in order. A new row stays blank.
        assert_eq!(
            got,
            vec![
                ("X-Token", "one"),
                ("Authorization", "tok"),
                ("X-Token", "two"),
                ("New", "")
            ]
        );
    }

    #[test]
    fn rows_with_the_same_name_and_another_type_do_not_mix() {
        let local = "http:\n  params:\n    - name: token\n      type: query\n      value: q\n    - name: token\n      type: path\n      value: '7'\n";
        let received = "http:\n  params:\n    - name: token\n      type: path\n      value: ''\n    - name: token\n      type: query\n      value: ''\n";
        let v = merged(local, received);
        assert_eq!(v["http"]["params"][0]["value"].as_str(), Some("7"));
        assert_eq!(v["http"]["params"][1]["value"].as_str(), Some("q"));
    }

    #[test]
    fn a_peer_that_clears_a_plain_value_clears_it_here() {
        // The export does not blank these slots, so an empty value is a deliberate clear.
        let v = merged("docs:\n  content: old\n", "docs:\n  content: ''\n");
        assert_eq!(v["docs"]["content"].as_str(), Some(""));
        let v = merged(
            "http:\n  url: https://a\n  headers:\n    - name: Accept\n      value: json\n",
            "http:\n  url: ''\n  headers:\n    - name: Accept\n      value: ''\n",
        );
        assert_eq!(v["http"]["url"].as_str(), Some(""));
        assert_eq!(v["http"]["headers"][0]["value"].as_str(), Some(""));
    }

    #[test]
    fn merging_an_export_gives_back_the_export() {
        let fixtures = [
            (
                "uid: u\ninfo:\n  name: A\n  type: http\nhttp:\n  url: https://a/?token=SECRET&x=1\n  headers:\n    - name: Authorization\n      value: Bearer S\n    - name: Accept\n      value: json\ndocs:\n  content: old\nauth:\n  type: bearer\n  token: T\n",
                "info:\n  name: A\n  type: http\nhttp:\n  url: https://b/?token=&x=2\n  headers:\n    - name: Authorization\n      value: ''\n    - name: Accept\n      value: ''\ndocs:\n  content: ''\nauth:\n  type: bearer\n  token: ''\n",
            ),
            (
                "info:\n  name: F\nvariables:\n  - name: api\n    secret: true\n    value: abc\n  - name: plain\n    value: keep\nexamples:\n  - name: e\n",
                "info:\n  name: F\nvariables:\n  - name: api\n    secret: true\n    value: ''\n  - name: plain\n    value: ''\n",
            ),
            (
                "extensions:\n  rocketapi:\n    sandboxMode: developer\nrequest:\n  auth:\n    type: basic\n    password: pw\n",
                "request:\n  auth:\n    type: basic\n    password: ''\n",
            ),
        ];
        for (local, peer) in fixtures {
            let sent = exported("f.yml", peer);
            let out = merge_file(Some(local.as_bytes()), &sent).expect("merges");
            let text = String::from_utf8(out).expect("utf8");
            assert_eq!(exported("f.yml", &text), sent, "{local}");
        }
    }

    #[test]
    fn a_legacy_folder_file_is_rewritten_to_the_info_shape() {
        let local = "name: users\ntype: folder\nuid: f1\nseq: 2\n";
        let received = "info:\n  name: users\n  type: folder\n  seq: 2\n";
        let v = merged(local, received);
        assert_eq!(v["info"]["name"].as_str(), Some("users"));
        assert!(v["name"].is_null());
        assert!(v["type"].is_null());
        assert!(v["seq"].is_null());
        assert_eq!(v["uid"].as_str(), Some("f1"));
    }

    fn refused(local: Option<&str>, received: &str) -> bool {
        matches!(
            merge_file(local.map(str::as_bytes), received.as_bytes()),
            Err(DomainError::InvalidInput(_))
        )
    }

    #[test]
    fn a_type_mismatch_still_drops_received_local_only_keys() {
        let v = merged(
            "info: ~
",
            "info:
  name: R
  uid: evil
",
        );
        assert_eq!(v["info"]["name"].as_str(), Some("R"));
        assert!(v["info"]["uid"].is_null());
        for local in [
            "extensions: ''
",
            "extensions: []
",
            "extensions: ~
",
        ] {
            let v = merged(
                local,
                "extensions:
  rocketapi:
    sandboxMode: developer
  bruno: 1
",
            );
            assert!(v["extensions"]["rocketapi"].is_null(), "{local}");
            assert_eq!(v["extensions"]["bruno"].as_u64(), Some(1), "{local}");
        }
        let v = merged(
            "",
            "uid: evil
info:
  name: R
  uid: e2
examples:
  - name: x
",
        );
        assert!(v["uid"].is_null());
        assert!(v["info"]["uid"].is_null());
        assert!(v["examples"].is_null());
        assert_eq!(v["info"]["name"].as_str(), Some("R"));
        let v = merged(
            "items: ~
",
            "items:
  - name: a
    examples:
      - name: e
    http:
      url: u
",
        );
        assert!(v["items"][0]["examples"].is_null());
        assert_eq!(v["items"][0]["http"]["url"].as_str(), Some("u"));
    }

    #[test]
    fn tagged_values_in_the_received_file_are_refused() {
        for received in [
            "extensions:
  !x rocketapi:
    sandboxMode: developer
",
            "examples: !x
  - name: e
",
            "!x
name: r
",
            "a: !x 1
",
            "a:
  - !x b
",
        ] {
            assert!(
                refused(
                    Some(
                        "a: 1
"
                    ),
                    received
                ),
                "{received}"
            );
            assert!(refused(None, received), "{received}");
        }
    }

    #[test]
    fn non_string_keys_in_the_received_file_are_refused() {
        for received in [
            "1: a
",
            "? [a, b]
: c
",
            "true: a
",
            "~: a
",
        ] {
            assert!(
                refused(
                    Some(
                        "a: 1
"
                    ),
                    received
                ),
                "{received}"
            );
        }
    }

    #[test]
    fn a_received_null_or_scalar_keeps_the_local_owned_keys() {
        let local = "uid: u
info:
  name: R
  uid: iu
extensions:
  rocketapi:
    sandboxMode: developer
examples:
  - name: e
http:
  url: a
";
        for received in [
            "extensions: ~
",
            "info: ''
",
            "info: 5
",
            "x: 1
",
        ] {
            let v = merged(local, received);
            assert_eq!(v["uid"].as_str(), Some("u"), "{received}");
            assert_eq!(v["info"]["uid"].as_str(), Some("iu"), "{received}");
            assert_eq!(
                v["extensions"]["rocketapi"]["sandboxMode"].as_str(),
                Some("developer"),
                "{received}"
            );
            assert_eq!(v["examples"][0]["name"].as_str(), Some("e"), "{received}");
        }
        // Without owned keys the received scalar still wins.
        let v = merged(
            "docs:
  content: x
",
            "docs: ~
",
        );
        assert!(v["docs"].is_null());
    }

    #[test]
    fn a_received_top_level_that_is_not_a_mapping_is_refused_for_a_mapping_local() {
        for received in ["", "~\n", "[1, 2]\n", "text\n"] {
            let err = merge_file(Some(b"a: 1\n"), received.as_bytes()).expect_err("refused");
            assert!(matches!(err, DomainError::InvalidInput(_)), "{received}");
            assert!(!err.to_string().contains("text"));
        }
    }

    #[test]
    fn examples_are_local_only_whatever_their_type() {
        let v = merged(
            "a: 1
",
            "a: 1
examples:
  k: v
",
        );
        assert!(v["examples"].is_null());
        let v = merged(
            "examples:
  - name: e
",
            "examples:
  k: v
",
        );
        assert_eq!(v["examples"][0]["name"].as_str(), Some("e"));
    }

    #[test]
    fn a_huge_alias_expansion_is_refused_fast() {
        let big = "x".repeat(1024 * 1024);
        let mut doc = format!("a: &a \"{big}\"\nl:\n");
        for _ in 0..4000 {
            doc.push_str("  - *a\n");
        }
        let start = std::time::Instant::now();
        assert!(refused(Some("a: 1\n"), &doc));
        assert!(refused(None, &doc));
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn nested_alias_growth_is_refused() {
        let mut doc = String::from("a: &a [x, x, x, x, x, x, x, x, x, x]\n");
        let mut prev = 'a';
        for next in ['b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k'] {
            let refs = format!("*{prev}, ").repeat(10);
            doc.push_str(&format!("{next}: &{next} [{refs}]\n"));
            prev = next;
        }
        assert!(refused(Some("a: 1\n"), &doc));
    }

    #[test]
    fn ampersands_and_stars_in_strings_and_urls_still_merge() {
        let received = "http:\n  url: https://x.test/a?b=1&c=2\n  note: \"a & b * c &x *y\"\n  glob: '*.txt'\n";
        let v = merged("http:\n  url: old\n", received);
        assert_eq!(v["http"]["url"].as_str(), Some("https://x.test/a?b=1&c=2"));
        assert_eq!(v["http"]["note"].as_str(), Some("a & b * c &x *y"));
        assert_eq!(v["http"]["glob"].as_str(), Some("*.txt"));
    }

    #[test]
    fn a_small_alias_is_allowed() {
        let v = merged("a: 1\n", "a: &x hello\nb: *x\n");
        assert_eq!(v["b"].as_str(), Some("hello"));
    }

    #[test]
    fn a_new_file_loses_the_peers_local_only_keys_and_keeps_the_rest() {
        let received = "uid: evil\ninfo:\n  name: R\n  uid: e2\nextensions:\n  rocketapi:\n    sandboxMode: developer\n  bruno:\n    keep: 1\nexamples:\n  - name: x\nhttp:\n  url: https://x\n";
        let out = merge_file(None, received.as_bytes()).expect("merges");
        let v = parse(&out);
        assert!(v["uid"].is_null());
        assert!(v["info"]["uid"].is_null());
        assert!(v["extensions"]["rocketapi"].is_null());
        assert!(v["examples"].is_null());
        assert_eq!(v["extensions"]["bruno"]["keep"].as_u64(), Some(1));
        assert_eq!(v["info"]["name"].as_str(), Some("R"));
        assert_eq!(v["http"]["url"].as_str(), Some("https://x"));
    }

    #[test]
    fn the_merged_file_exports_to_the_received_bytes() {
        let local = "uid: local-uid\ninfo:\n  name: Login\n  type: http\n  uid: local-info\nextensions:\n  rocketapi:\n    sandboxMode: developer\nexamples:\n  - name: e1\nhttp:\n  method: GET\n  url: https://old\n  headers:\n    - name: Authorization\n      value: Bearer LOCALSECRET\n";
        let peer = "uid: peer-uid\ninfo:\n  name: Login\n  type: http\nhttp:\n  method: POST\n  url: https://new\n  headers:\n    - name: Authorization\n      value: Bearer PEERSECRET\n    - name: Accept\n      value: '*/*'\n";
        let received = exported("Login.yml", peer);
        let merged_bytes = merge_file(Some(local.as_bytes()), &received).expect("merges");
        let text = String::from_utf8(merged_bytes).expect("utf8");
        // The local secret is still there ...
        assert!(text.contains("LOCALSECRET"));
        // ... and the export of the merged file is the received file, so both sides hash alike.
        assert_eq!(exported("Login.yml", &text), received);
    }

    /// The sync view of one exported file.
    fn synced(rel: &str, raw: &str) -> Vec<u8> {
        sync_bytes(rel, exported(rel, raw)).expect("sync view")
    }

    #[test]
    fn the_sync_view_of_the_root_file_has_no_collection_name() {
        let root =
            "opencollection: 1.0.0\nuid: c1\ninfo:\n  name: Orders\n  version: '2'\ndocs: hi\n";
        let v = parse(&synced("opencollection.yml", root));
        assert!(v["info"]["name"].is_null());
        assert_eq!(v["info"]["version"].as_str(), Some("2"));
        assert_eq!(v["docs"].as_str(), Some("hi"));
        // Two roots that differ only by name look the same to the sync.
        let other = root.replace("name: Orders", "name: Orders (from Ana)");
        assert_eq!(
            synced("opencollection.yml", root),
            synced("opencollection.yml", &other)
        );
    }

    #[test]
    fn the_sync_view_of_other_files_is_the_export() {
        let request = "info:\n  name: get\n  type: http\nhttp:\n  url: https://x\n";
        let folder = "info:\n  name: users\n  type: folder\n";
        assert_eq!(synced("get.yml", request), exported("get.yml", request));
        assert_eq!(
            synced("users/folder.yml", folder),
            exported("users/folder.yml", folder)
        );
        // A root without a name is left as it is.
        let bare = exported("opencollection.yml", "docs: hi\n");
        assert_eq!(
            sync_bytes("opencollection.yml", bare.clone()).expect("view"),
            bare
        );
    }

    #[test]
    fn the_root_file_keeps_its_local_name_and_a_request_takes_the_peers_name() {
        let local_root = "uid: c1\ninfo:\n  name: Orders (from Ana)\n  version: '1'\n";
        let received = synced(
            "opencollection.yml",
            "info:\n  name: Orders\n  version: '2'\n",
        );
        let out = merge_synced_file("opencollection.yml", Some(local_root.as_bytes()), &received)
            .expect("merges");
        let v = parse(&out);
        assert_eq!(v["info"]["name"].as_str(), Some("Orders (from Ana)"));
        assert_eq!(v["info"]["version"].as_str(), Some("2"));
        assert_eq!(v["uid"].as_str(), Some("c1"));
        // A received root name is ignored too.
        let named = b"info:\n  name: Evil\n";
        let out = merge_synced_file("opencollection.yml", Some(local_root.as_bytes()), named)
            .expect("merges");
        assert_eq!(
            parse(&out)["info"]["name"].as_str(),
            Some("Orders (from Ana)")
        );
        // A request and a folder sync their names.
        for path in ["get.yml", "users/folder.yml"] {
            let out =
                merge_synced_file(path, Some(b"info:\n  name: old\n"), b"info:\n  name: new\n")
                    .expect("merges");
            assert_eq!(parse(&out)["info"]["name"].as_str(), Some("new"), "{path}");
        }
    }

    #[test]
    fn a_new_root_file_is_written_without_the_peers_name() {
        let out = merge_synced_file(
            "opencollection.yml",
            None,
            b"info:\n  name: Evil\n  version: '1'\n",
        )
        .expect("merges");
        let v = parse(&out);
        assert!(v["info"]["name"].is_null());
        assert_eq!(v["info"]["version"].as_str(), Some("1"));
    }

    #[test]
    fn a_merged_root_file_has_the_received_sync_view() {
        // The local root has its keys in another order and its own name, uid and settings.
        let local = "opencollection: 1.0.0\nuid: c2\ninfo:\n  name: Orders (from Ana)\nextensions:\n  rocketapi:\n    sandboxMode: developer\nrequest:\n  auth:\n    type: bearer\n    token: MINE\ndocs: old\nconfig:\n  proxy:\n    disabled: true\n";
        let peer = "opencollection: 1.0.0\nuid: c1\ninfo:\n  name: Orders\nconfig:\n  proxy:\n    disabled: true\ndocs: new\nrequest:\n  auth:\n    type: bearer\n    token: THEIRS\n";
        let received = synced("opencollection.yml", peer);
        let out = merge_synced_file("opencollection.yml", Some(local.as_bytes()), &received)
            .expect("merges");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("MINE"));
        assert!(text.contains("Orders (from Ana)"));
        assert_eq!(synced("opencollection.yml", &text), received);
    }
}
