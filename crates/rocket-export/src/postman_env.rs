//! Postman environment files and the collection plus environments ZIP.

use std::io::{Cursor, Write};

use rocket_environment::Variable;
use rocket_shared::error::{DomainError, DomainResult};
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::report::{ExportAction, ExportNote};
use crate::scrub::is_variable_reference;
use crate::sensitive::is_sensitive_name;

/// The exported value of a variable: a secret keeps only a lone reference, and a
/// plain variable with a secret-like name is blanked.
fn exported_value(v: &Variable) -> &str {
    let keep_ref = is_variable_reference(&v.value);
    if v.secret {
        if keep_ref {
            v.value.as_str()
        } else {
            ""
        }
    } else if is_sensitive_name(&v.key) && !keep_ref {
        ""
    } else {
        v.value.as_str()
    }
}

/// One Postman environment document. A secret variable keeps its key and
/// `type: "secret"`, and its value only when that is a variable reference. The
/// service clears secret values before this call, so they are normally empty.
pub fn postman_environment(
    name: &str,
    vars: &[Variable],
    exported_at: &str,
    version: &str,
) -> Value {
    let values: Vec<Value> = vars
        .iter()
        .map(|v| {
            json!({
                "key": v.key,
                "value": exported_value(v),
                "type": if v.secret { "secret" } else { "default" },
                "enabled": v.enabled,
            })
        })
        .collect();
    json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "name": name,
        "values": values,
        "_postman_variable_scope": "environment",
        "_postman_exported_at": exported_at,
        "_postman_exported_using": format!("Rocket/{version}"),
    })
}

/// Report notes of one environment: every blanked value and every
/// `{{process.env.X}}` reference that stays as text.
pub fn environment_notes(name: &str, vars: &[Variable]) -> Vec<ExportNote> {
    let path = format!("environments/{name}");
    let mut notes = Vec::new();
    for v in vars {
        if v.secret {
            if !is_variable_reference(&v.value) {
                notes.push(ExportNote {
                    path: path.clone(),
                    feature: "Secret variable".into(),
                    action: ExportAction::Blanked,
                    detail: Some(v.key.clone()),
                });
            }
        } else if is_sensitive_name(&v.key)
            && !v.value.is_empty()
            && !is_variable_reference(&v.value)
        {
            notes.push(ExportNote {
                path: path.clone(),
                feature: "Variable".into(),
                action: ExportAction::Blanked,
                detail: Some(format!("{} (by name)", v.key)),
            });
        } else if v.value.contains("{{process.env.") {
            notes.push(ExportNote {
                path: path.clone(),
                feature: "process.env reference".into(),
                action: ExportAction::Preserved,
                detail: Some(v.key.clone()),
            });
        }
    }
    notes
}

fn zip_err(e: impl std::fmt::Display) -> DomainError {
    DomainError::Internal(format!("zip: {e}"))
}

/// The collection file at the ZIP root and each environment file (name and bytes)
/// under `environments/`.
pub fn postman_bundle(
    collection_file: (&str, &[u8]),
    envs: Vec<(String, Vec<u8>)>,
) -> DomainResult<Vec<u8>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    writer
        .start_file(collection_file.0, options)
        .map_err(zip_err)?;
    writer.write_all(collection_file.1).map_err(zip_err)?;
    for (file_name, bytes) in envs {
        writer
            .start_file(format!("environments/{file_name}"), options)
            .map_err(zip_err)?;
        writer.write_all(&bytes).map_err(zip_err)?;
    }
    let cursor = writer.finish().map_err(zip_err)?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::ExportAction;
    use rocket_environment::Variable;
    use std::io::Cursor;

    const STAMP: &str = "2026-10-23T00:00:00Z";

    fn secret(key: &str, value: &str) -> Variable {
        let mut v = Variable::new(key, value);
        v.secret = true;
        v
    }

    #[test]
    fn secret_variable_value_is_empty_even_when_hydrated() {
        let doc = postman_environment(
            "prod",
            &[
                secret("API_KEY", "hydrated-value"),
                Variable::new("HOST", "x.test"),
            ],
            STAMP,
            "2.1.1",
        );
        assert_eq!(doc["values"][0]["type"], "secret");
        assert_eq!(doc["values"][0]["value"], "");
        assert_eq!(doc["values"][1]["value"], "x.test");
        assert_eq!(doc["_postman_variable_scope"], "environment");
        assert_eq!(doc["_postman_exported_using"], "Rocket/2.1.1");
        assert_eq!(doc["_postman_exported_at"], STAMP);
        assert_eq!(doc["name"], "prod");
        assert!(!doc.to_string().contains("hydrated-value"));
    }

    #[test]
    fn environment_secret_ref_is_kept_and_names_are_scrubbed() {
        let vars = [
            secret("A", "{{ref}}"),
            secret("B", "hydrated-value"),
            Variable::new("api_token", "abc"),
            Variable::new("HOST", "x.test"),
        ];
        let doc = postman_environment("e", &vars, STAMP, "1");
        assert_eq!(doc["values"][0]["value"], "{{ref}}");
        assert_eq!(doc["values"][0]["type"], "secret");
        assert_eq!(doc["values"][1]["value"], "");
        assert_eq!(doc["values"][2]["value"], "");
        assert_eq!(doc["values"][3]["value"], "x.test");
        let notes = environment_notes("e", &vars);
        let details: Vec<&str> = notes
            .iter()
            .filter(|n| n.action == ExportAction::Blanked)
            .filter_map(|n| n.detail.as_deref())
            .collect();
        assert_eq!(details, ["B", "api_token (by name)"]);
    }

    #[test]
    fn plain_variable_has_default_type_and_is_enabled() {
        let doc = postman_environment("e", &[Variable::new("HOST", "x.test")], STAMP, "1");
        assert_eq!(doc["values"][0]["type"], "default");
        assert_eq!(doc["values"][0]["enabled"], true);
        assert_eq!(doc["values"][0]["key"], "HOST");
        let id = doc["id"].as_str().expect("id string");
        assert!(uuid::Uuid::parse_str(id).is_ok());
    }

    #[test]
    fn disabled_variable_has_enabled_false() {
        let doc = postman_environment("e", &[Variable::disabled("A", "1")], STAMP, "1");
        assert_eq!(doc["values"][0]["enabled"], false);
    }

    #[test]
    fn bundle_contains_collection_and_environment_files() {
        let bytes = postman_bundle(
            ("my-api.postman_collection.json", b"{}"),
            vec![("prod.postman_environment.json".into(), b"{}".to_vec())],
        )
        .expect("bundle");
        let archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("zip");
        let mut names: Vec<String> = archive.file_names().map(str::to_string).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "environments/prod.postman_environment.json",
                "my-api.postman_collection.json"
            ]
        );
    }

    #[test]
    fn secret_variables_give_blanked_notes_and_process_env_is_preserved() {
        let notes = environment_notes(
            "prod",
            &[
                secret("A", "x"),
                secret("B", ""),
                Variable::new("HOME_DIR", "{{process.env.HOME}}/x"),
                Variable::new("PLAIN", "1"),
            ],
        );
        assert_eq!(notes.len(), 3);
        assert!(notes.iter().all(|n| n.path == "environments/prod"));
        let blanked: Vec<&str> = notes
            .iter()
            .filter(|n| n.action == ExportAction::Blanked)
            .filter_map(|n| n.detail.as_deref())
            .collect();
        assert_eq!(blanked, ["A", "B"]);
        let kept = notes
            .iter()
            .find(|n| n.action == ExportAction::Preserved)
            .expect("process.env note");
        assert_eq!(kept.detail.as_deref(), Some("HOME_DIR"));
    }
}
