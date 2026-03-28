# OC-P16: Minor Fixes

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 3 minor issues: relax Description parsing, preserve multipart metadata, distinguish form body types.

**Architecture:** Small targeted changes to domain types and conversion layer.

**Tech Stack:** Rust, serde

**Prerequisite:** OC-P14 complete.

---

## Task 1: Relax Description deserialization when `type` is absent

**Problem:** In `description.rs`, the `visit_map` arm `(Some(c), None)` returns `Err(de::Error::missing_field("type"))`. Real-world OC files may contain `{"content": "hello"}` without a `type` field. The original spec allowed this to produce `Description::Text(content)`.

**Files:**
- `crates/rocket-shared/src/description.rs`

### Steps

- [ ] **1.1** In `crates/rocket-shared/src/description.rs`, line 77, change the `(Some(_), None)` arm from returning an error to returning `Description::text(c)`:

  **Before (line 75-78):**
  ```rust
  match (content, content_type) {
      (Some(c), Some(t)) => Ok(Description::typed(c, t)),
      (Some(_), None) => Err(de::Error::missing_field("type")),
      _ => Err(de::Error::missing_field("content")),
  }
  ```

  **After:**
  ```rust
  match (content, content_type) {
      (Some(c), Some(t)) => Ok(Description::typed(c, t)),
      (Some(c), None) => Ok(Description::text(c)),
      _ => Err(de::Error::missing_field("content")),
  }
  ```

- [ ] **1.2** Update the existing test `description_object_missing_type_is_rejected` (line 128) to assert success instead of error:

  **Before:**
  ```rust
  #[test]
  fn description_object_missing_type_is_rejected() {
      let json = r#"{"content": "hello"}"#;
      let result = serde_json::from_str::<Description>(json);
      assert!(result.is_err());
  }
  ```

  **After:**
  ```rust
  #[test]
  fn description_object_missing_type_falls_back_to_text() {
      let json = r#"{"content": "hello"}"#;
      let desc: Description = serde_json::from_str(json).unwrap();
      assert_eq!(desc, Description::Text("hello".into()));
      assert_eq!(desc.content(), Some("hello"));
      assert_eq!(desc.content_type(), None);
  }
  ```

- [ ] **1.3** Add a new test for the `(None, Some(_))` case (still rejected):

  ```rust
  #[test]
  fn description_object_missing_content_is_rejected() {
      let json = r#"{"type": "text/markdown"}"#;
      let result = serde_json::from_str::<Description>(json);
      assert!(result.is_err());
  }
  ```

- [ ] **1.4** Run tests: `cargo test -p rocket-shared -- description`

**Commit message:** `fix(shared): relax Description deserializer to accept object without type field`

---

## Task 2: Preserve multipart form metadata (content_type, description)

**Problem:** When converting `OcMultipartFormPart` to `FormDataEntry`, the `content_type` and `description` fields are silently dropped. The reverse conversion (`entry_to_multipart`) always emits `None` for both. This loses metadata during OC round-trips.

**Files:**
- `crates/rocket-shared/src/types.rs` (add fields to `FormDataEntry`)
- `crates/rocket-infra/src/oc_conversions.rs` (update `multipart_to_entry` and `entry_to_multipart`)

### Steps

- [ ] **2.1** In `crates/rocket-shared/src/types.rs`, add `content_type` and `description` to `FormDataEntry` (currently lines 151-158):

  **Before:**
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct FormDataEntry {
      pub key: String,
      pub value: String,
      pub entry_type: FormDataType,
      pub enabled: bool,
  }
  ```

  **After:**
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct FormDataEntry {
      pub key: String,
      pub value: String,
      pub entry_type: FormDataType,
      pub enabled: bool,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub content_type: Option<String>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub description: Option<Description>,
  }
  ```

- [ ] **2.2** Fix all existing `FormDataEntry` struct literals across the codebase. Every place that constructs a `FormDataEntry` must now include the two new fields. Known sites:

  - `crates/rocket-infra/src/oc_conversions.rs` `form_field_to_entry` (line 176): add `content_type: None, description: f.description,`
  - `crates/rocket-infra/src/oc_conversions.rs` `multipart_to_entry` (line 186): add `content_type: p.content_type, description: p.description,`

- [ ] **2.3** Update `entry_to_multipart` in `oc_conversions.rs` (line 257) to propagate the fields back:

  **Before:**
  ```rust
  fn entry_to_multipart(e: FormDataEntry) -> OcMultipartFormPart {
      OcMultipartFormPart {
          name: e.key,
          part_type: match e.entry_type {
              FormDataType::File => "file".into(),
              FormDataType::Text => "text".into(),
          },
          value: OcMultipartValue::Single(e.value),
          description: None,
          content_type: None,
          disabled: if e.enabled { None } else { Some(true) },
      }
  }
  ```

  **After:**
  ```rust
  fn entry_to_multipart(e: FormDataEntry) -> OcMultipartFormPart {
      OcMultipartFormPart {
          name: e.key,
          part_type: match e.entry_type {
              FormDataType::File => "file".into(),
              FormDataType::Text => "text".into(),
          },
          value: OcMultipartValue::Single(e.value),
          description: e.description,
          content_type: e.content_type,
          disabled: if e.enabled { None } else { Some(true) },
      }
  }
  ```

- [ ] **2.4** Update `entry_to_form_field` in `oc_conversions.rs` (line 247) to propagate `description`:

  **Before:**
  ```rust
  fn entry_to_form_field(e: FormDataEntry) -> OcFormField {
      OcFormField {
          name: e.key,
          value: e.value,
          description: None,
          disabled: if e.enabled { None } else { Some(true) },
      }
  }
  ```

  **After:**
  ```rust
  fn entry_to_form_field(e: FormDataEntry) -> OcFormField {
      OcFormField {
          name: e.key,
          value: e.value,
          description: e.description,
          disabled: if e.enabled { None } else { Some(true) },
      }
  }
  ```

- [ ] **2.5** Add a test for multipart metadata preservation in `oc_conversions.rs`:

  ```rust
  #[test]
  fn multipart_metadata_preserved_in_roundtrip() {
      let part = OcMultipartFormPart {
          name: "avatar".into(),
          part_type: "file".into(),
          value: OcMultipartValue::Single("/tmp/avatar.png".into()),
          description: Some(Description::text("User avatar")),
          content_type: Some("image/png".into()),
          disabled: None,
      };
      let entry = multipart_to_entry(part);
      assert_eq!(entry.content_type, Some("image/png".into()));
      assert_eq!(
          entry.description,
          Some(Description::text("User avatar"))
      );
      let back = entry_to_multipart(entry);
      assert_eq!(back.content_type, Some("image/png".into()));
      assert_eq!(
          back.description,
          Some(Description::text("User avatar"))
      );
  }
  ```

- [ ] **2.6** Run tests: `cargo test -p rocket-infra -- multipart_metadata` and `cargo test -p rocket-shared`

**Commit message:** `fix(shared): preserve content_type and description on FormDataEntry round-trips`

---

## Task 3: Distinguish FormUrlEncoded from MultipartForm in the domain

**Problem:** Both `OcHttpRequestBody::FormUrlEncoded` and `OcHttpRequestBody::MultipartForm` map to `BodyMode::FormData` in the domain. This makes the reverse conversion lossy (it guesses based on whether any entry is a file) and prevents `apply_body()` from sending the correct Content-Type. A multipart form with only text fields would incorrectly be serialized as `application/x-www-form-urlencoded`.

**Files:**
- `crates/rocket-shared/src/types.rs` (add `BodyMode::FormUrlEncoded`)
- `crates/rocket-infra/src/oc_conversions.rs` (use correct variant per body type)
- `crates/rocket-infra/src/reqwest_executor.rs` (handle both variants in `apply_body`)

### Steps

- [ ] **3.1** In `crates/rocket-shared/src/types.rs`, add a `FormUrlEncoded` variant to `BodyMode` (currently lines 123-138):

  **Before:**
  ```rust
  pub enum BodyMode {
      #[serde(rename = "none")]
      None,
      #[serde(rename = "json")]
      Json,
      #[serde(rename = "xml")]
      Xml,
      #[serde(rename = "text")]
      Text,
      #[serde(rename = "formdata")]
      FormData,
      #[serde(rename = "binary")]
      Binary,
  }
  ```

  **After:**
  ```rust
  pub enum BodyMode {
      #[serde(rename = "none")]
      None,
      #[serde(rename = "json")]
      Json,
      #[serde(rename = "xml")]
      Xml,
      #[serde(rename = "text")]
      Text,
      #[serde(rename = "formurlencoded")]
      FormUrlEncoded,
      #[serde(rename = "formdata")]
      FormData,
      #[serde(rename = "binary")]
      Binary,
  }
  ```

- [ ] **3.2** In `crates/rocket-infra/src/oc_conversions.rs`, update the `From<OcHttpRequestBody> for Body` impl. Change the `FormUrlEncoded` arm (line 153) to use `BodyMode::FormUrlEncoded`:

  **Before:**
  ```rust
  OcHttpRequestBody::FormUrlEncoded { data } => Body {
      mode: BodyMode::FormData,
      content: None,
      form_data: Some(data.into_iter().map(form_field_to_entry).collect()),
      file_path: None,
  },
  ```

  **After:**
  ```rust
  OcHttpRequestBody::FormUrlEncoded { data } => Body {
      mode: BodyMode::FormUrlEncoded,
      content: None,
      form_data: Some(data.into_iter().map(form_field_to_entry).collect()),
      file_path: None,
  },
  ```

- [ ] **3.3** In `crates/rocket-infra/src/oc_conversions.rs`, update the `From<Body> for OcHttpRequestBody` impl. The current `BodyMode::FormData` arm (line 216) uses heuristics. Split into two explicit arms:

  **Before:**
  ```rust
  BodyMode::FormData => {
      let entries = b.form_data.unwrap_or_default();
      // If any entry is a file type, emit multipart-form.
      if entries.iter().any(|e| e.entry_type == FormDataType::File) {
          OcHttpRequestBody::MultipartForm {
              data: entries.into_iter().map(entry_to_multipart).collect(),
          }
      } else {
          OcHttpRequestBody::FormUrlEncoded {
              data: entries.into_iter().map(entry_to_form_field).collect(),
          }
      }
  }
  ```

  **After:**
  ```rust
  BodyMode::FormUrlEncoded => {
      let entries = b.form_data.unwrap_or_default();
      OcHttpRequestBody::FormUrlEncoded {
          data: entries.into_iter().map(entry_to_form_field).collect(),
      }
  }
  BodyMode::FormData => {
      let entries = b.form_data.unwrap_or_default();
      OcHttpRequestBody::MultipartForm {
          data: entries.into_iter().map(entry_to_multipart).collect(),
      }
  }
  ```

- [ ] **3.4** In `crates/rocket-infra/src/reqwest_executor.rs`, update `apply_body()` to handle `FormUrlEncoded` separately from `FormData`. The current `FormData` arm (line 265) sends `form()` (URL-encoded). Add a `FormUrlEncoded` arm for URL-encoded and change the `FormData` arm to send multipart:

  **Before (line 265-274):**
  ```rust
  BodyMode::FormData => {
      if let Some(entries) = &body.form_data {
          let params: Vec<(&str, &str)> = entries
              .iter()
              .filter(|e| e.enabled)
              .map(|e| (e.key.as_str(), e.value.as_str()))
              .collect();
          builder = builder.form(&params);
      }
  }
  ```

  **After:**
  ```rust
  BodyMode::FormUrlEncoded => {
      if let Some(entries) = &body.form_data {
          let params: Vec<(&str, &str)> = entries
              .iter()
              .filter(|e| e.enabled)
              .map(|e| (e.key.as_str(), e.value.as_str()))
              .collect();
          builder = builder.form(&params);
      }
  }
  BodyMode::FormData => {
      if let Some(entries) = &body.form_data {
          let mut form = reqwest::multipart::Form::new();
          for entry in entries.iter().filter(|e| e.enabled) {
              match entry.entry_type {
                  rocket_shared::types::FormDataType::File => {
                      let path = std::path::Path::new(&entry.value);
                      let file_bytes = std::fs::read(path).map_err(|e| {
                          DomainError::Internal(format!(
                              "Failed to read multipart file '{}': {e}",
                              entry.value
                          ))
                      })?;
                      let file_name = path
                          .file_name()
                          .map(|n| n.to_string_lossy().into_owned())
                          .unwrap_or_default();
                      let mut part = reqwest::multipart::Part::bytes(file_bytes)
                          .file_name(file_name);
                      if let Some(ct) = &entry.content_type {
                          let mime: mime::Mime = ct.parse().unwrap_or(mime::APPLICATION_OCTET_STREAM);
                          part = part.mime_str(mime.as_ref()).map_err(|e| {
                              DomainError::Internal(format!("Invalid MIME: {e}"))
                          })?;
                      }
                      form = form.part(entry.key.clone(), part);
                  }
                  rocket_shared::types::FormDataType::Text => {
                      form = form.text(entry.key.clone(), entry.value.clone());
                  }
              }
          }
          builder = builder.multipart(form);
      }
  }
  ```

- [ ] **3.5** Update the existing body conversion test `body_form_urlencoded_oc_to_domain` in `oc_conversions.rs` (line 1281) to assert `BodyMode::FormUrlEncoded`:

  **Before:**
  ```rust
  assert_eq!(body.mode, BodyMode::FormData);
  ```

  **After:**
  ```rust
  assert_eq!(body.mode, BodyMode::FormUrlEncoded);
  ```

- [ ] **3.6** Add a test for multipart form body mode in `oc_conversions.rs`:

  ```rust
  #[test]
  fn body_multipart_form_uses_formdata_mode() {
      let oc = OcHttpRequestBody::MultipartForm { data: vec![
          OcMultipartFormPart {
              name: "file".into(),
              part_type: "file".into(),
              value: OcMultipartValue::Single("/tmp/test.txt".into()),
              description: None,
              content_type: Some("text/plain".into()),
              disabled: None,
          },
      ]};
      let body: Body = oc.into();
      assert_eq!(body.mode, BodyMode::FormData);
  }
  ```

- [ ] **3.7** Add a test for the reverse conversion in `oc_conversions.rs`:

  ```rust
  #[test]
  fn body_formurlencoded_roundtrip() {
      let body = Body {
          mode: BodyMode::FormUrlEncoded,
          content: None,
          form_data: Some(vec![FormDataEntry {
              key: "user".into(),
              value: "admin".into(),
              entry_type: FormDataType::Text,
              enabled: true,
              content_type: None,
              description: None,
          }]),
          file_path: None,
      };
      let oc: OcHttpRequestBody = body.into();
      assert!(matches!(oc, OcHttpRequestBody::FormUrlEncoded { .. }));
  }

  #[test]
  fn body_formdata_roundtrip_emits_multipart() {
      let body = Body {
          mode: BodyMode::FormData,
          content: None,
          form_data: Some(vec![FormDataEntry {
              key: "name".into(),
              value: "test".into(),
              entry_type: FormDataType::Text,
              enabled: true,
              content_type: None,
              description: None,
          }]),
          file_path: None,
      };
      let oc: OcHttpRequestBody = body.into();
      assert!(matches!(oc, OcHttpRequestBody::MultipartForm { .. }));
  }
  ```

- [ ] **3.8** Check if `reqwest` multipart feature is enabled. In `crates/rocket-infra/Cargo.toml`, ensure `reqwest` has the `multipart` feature. Also confirm `mime` is available as a dependency (add if missing).

- [ ] **3.9** Run full test suite: `cargo test -p rocket-shared && cargo test -p rocket-infra`

**Commit message:** `feat(shared): add BodyMode::FormUrlEncoded to distinguish URL-encoded from multipart form bodies`

---

## Verification

- [ ] Run the full workspace build: `cargo build`
- [ ] Run the full workspace test suite: `cargo test`
- [ ] Confirm no clippy warnings: `cargo clippy -- -D warnings`
