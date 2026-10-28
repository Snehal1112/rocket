//! Tests for the raw spec text. They reuse the world of the apply tests.

use super::*;
use crate::openapi_sync_view::SpecSourceFormat;

/// Stores `bytes` as the applied copy and points the source at them.
fn set_applied(w: &World, bytes: &[u8]) {
    let mut source = w.source();
    source.spec_sha256 = sha256_hex(bytes);
    rocket_infra::FsOpenApiSourceRepo
        .save(&w.root, &source)
        .expect("save source");
    std::fs::write(w.root.join(".rocket/openapi-applied.spec"), bytes).expect("write");
}

#[tokio::test]
async fn no_copy_gives_no_text() {
    let w = world().await;
    std::fs::remove_file(w.root.join(".rocket/openapi-applied.spec")).expect("remove");
    let src = w.svc.spec_source(&w.name).expect("source");
    assert_eq!(src.text, None);
    assert_eq!(src.size_bytes, 0);
    assert!(!src.too_large);
    assert_eq!(src.format, SpecSourceFormat::Yaml);
}

#[tokio::test]
async fn a_yaml_spec_gives_its_text_and_yaml() {
    let w = world().await;
    let yaml = "openapi: 3.0.3\ninfo:\n  title: Pets\n  version: '1'\npaths: {}\n";
    set_applied(&w, yaml.as_bytes());
    let src = w.svc.spec_source(&w.name).expect("source");
    assert_eq!(src.text.as_deref(), Some(yaml));
    assert_eq!(src.format, SpecSourceFormat::Yaml);
    assert_eq!(src.size_bytes, yaml.len() as u64);
    assert!(!src.too_large);
}

#[tokio::test]
async fn a_json_spec_gives_json() {
    let w = world().await;
    let src = w.svc.spec_source(&w.name).expect("source");
    assert_eq!(src.format, SpecSourceFormat::Json);
    assert_eq!(src.text.as_deref(), Some(v1().as_str()));
    set_applied(&w, b"  \n{\"openapi\": \"3.0.3\"}");
    let src = w.svc.spec_source(&w.name).expect("source");
    assert_eq!(src.format, SpecSourceFormat::Json);
}

#[tokio::test]
async fn the_pending_copy_is_preferred() {
    let w = world().await;
    w.check_with(v2()).await;
    let src = w.svc.spec_source(&w.name).expect("source");
    assert_eq!(src.text.as_deref(), Some(v2().as_str()));
}

#[tokio::test]
async fn an_unsound_pending_copy_gives_the_fixed_sentence() {
    let w = world().await;
    w.check_with(v2()).await;
    std::fs::write(w.root.join(".rocket/openapi-pending.spec"), "{}").expect("tamper");
    let err = w.svc.spec_source(&w.name).expect_err("must fail");
    assert!(
        err.to_string()
            .contains("A newer spec is waiting but its copy is missing"),
        "{err}"
    );
}

#[tokio::test]
async fn a_copy_over_the_cap_gives_too_large_and_no_text() {
    let w = world().await;
    let size = v1().len();
    let src = w.svc.spec_source_capped(&w.name, size - 1).expect("source");
    assert_eq!(src.text, None);
    assert!(src.too_large);
    assert_eq!(src.size_bytes, size as u64);
    let at_cap = w.svc.spec_source_capped(&w.name, size).expect("source");
    assert!(at_cap.text.is_some());
    assert!(!at_cap.too_large);
}

#[tokio::test]
async fn a_json_spec_with_a_bom_is_json() {
    let w = world().await;
    set_applied(&w, "\u{feff}{\"openapi\": \"3.0.3\"}".as_bytes());
    let src = w.svc.spec_source(&w.name).expect("source");
    assert_eq!(src.format, SpecSourceFormat::Json);
    assert_eq!(src.text.as_deref(), Some("{\"openapi\": \"3.0.3\"}"));
}

#[tokio::test]
async fn non_utf8_bytes_give_the_fixed_sentence() {
    let w = world().await;
    set_applied(&w, &[0x7b, 0xff, 0xfe, 0x7d]);
    let err = w.svc.spec_source(&w.name).expect_err("must fail");
    assert!(
        err.to_string().contains("The saved spec cannot be shown."),
        "{err}"
    );
}
