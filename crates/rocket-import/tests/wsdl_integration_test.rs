use rocket_environment::EnvironmentRepository;
use rocket_import::{EnvironmentRepositoryFactory, ImportService};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct FsEnvFactory(PathBuf);
impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::new(
            self.0
                .join("collections")
                .join(collection_name)
                .join("environments"),
        ))
    }
}

fn make_service(workspace_path: &Path) -> ImportService {
    let path = workspace_path.to_path_buf();
    ImportService::new(
        path.clone(),
        Box::new(FsCollectionRepo::new_standalone(path.join("collections"))),
        Box::new(FsEnvFactory(path)),
    )
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wsdl")
        .join(name)
}

#[test]
fn imports_wsdl_through_the_public_api() {
    let ws = TempDir::new().expect("tempdir");
    let report = make_service(ws.path())
        .import_wsdl(&fixture("calc.wsdl"), "default")
        .expect("should import");
    assert_eq!(report.imported, 4);
    assert_eq!(report.created_collections, vec!["calc".to_string()]);
    assert!(ws.path().join("collections/calc/Calculator/CalcSoap").is_dir());
}

#[test]
fn missing_file_is_an_io_error_not_a_panic() {
    let ws = TempDir::new().expect("tempdir");
    let err = make_service(ws.path())
        .import_wsdl(&fixture("does-not-exist.wsdl"), "default")
        .expect_err("must fail");
    assert!(err.to_string().to_lowercase().contains("io error"), "got: {err}");
}

#[test]
fn non_wsdl_xml_is_a_parse_error() {
    let ws = TempDir::new().expect("tempdir");
    let src = TempDir::new().expect("tempdir");
    let path = src.path().join("not.wsdl");
    std::fs::write(&path, "<html/>").expect("write");
    let err = make_service(ws.path())
        .import_wsdl(&path, "default")
        .expect_err("must fail");
    assert!(err.to_string().contains("not a WSDL 1.1 document"), "got: {err}");
}
