use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use rocket_grpc::{ProtoFileReader, ProtoLoader, ProtoRegistry};
use rocket_shared::error::{DomainError, DomainResult};

/// Reads `.proto` imports from a list of include directories.
///
/// An import name must be a plain relative path. A name with `..`, a root or a
/// drive prefix is never read, and neither is a file whose real path (after
/// symlinks) leaves its include directory.
pub struct FsProtoFileReader {
    include_dirs: Vec<PathBuf>,
}

impl FsProtoFileReader {
    pub fn new(include_dirs: Vec<PathBuf>) -> Self {
        Self { include_dirs }
    }
}

impl ProtoFileReader for FsProtoFileReader {
    fn read(&self, name: &str) -> Option<String> {
        let relative = Path::new(name);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return None;
        }
        for dir in &self.include_dirs {
            let Ok(base) = dir.canonicalize() else {
                continue;
            };
            let Ok(real) = dir.join(relative).canonicalize() else {
                continue;
            };
            if !real.starts_with(&base) || !real.is_file() {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&real) {
                return Some(text);
            }
        }
        None
    }
}

/// Compiles a `.proto` file from disk.
pub struct FsProtoLoader;

impl ProtoLoader for FsProtoLoader {
    fn load(
        &self,
        proto_file: &Path,
        extra_include_dirs: &[PathBuf],
    ) -> DomainResult<ProtoRegistry> {
        if !proto_file.is_file() {
            return Err(DomainError::NotFound(format!(
                "proto file '{}'",
                proto_file.display()
            )));
        }
        let parent = proto_file.parent().ok_or_else(|| {
            DomainError::InvalidInput(format!("'{}' has no directory", proto_file.display()))
        })?;
        let entry = proto_file
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                DomainError::InvalidInput(format!("'{}' is not a file name", proto_file.display()))
            })?;
        let mut dirs = vec![parent.to_path_buf()];
        dirs.extend(extra_include_dirs.iter().cloned());
        ProtoRegistry::compile(entry, Arc::new(FsProtoFileReader::new(dirs)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::GrpcMethodType;
    use tempfile::TempDir;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/grpc")
    }

    #[test]
    fn loads_the_greeter_fixture_with_its_imports() {
        let registry = FsProtoLoader
            .load(&fixtures().join("greeter.proto"), &[])
            .expect("load");
        let services = registry.services();
        assert_eq!(services[0].name, "demo.greeter.v1.Greeter");
        assert_eq!(
            services[0].methods[3].method_type,
            GrpcMethodType::BidiStreaming
        );
    }

    #[test]
    fn extra_include_dirs_resolve_imports_that_live_elsewhere() {
        let dir = TempDir::new().expect("tempdir");
        let shared = dir.path().join("protos");
        let nested = shared.join("deep");
        fs::create_dir_all(&nested).expect("mkdir");
        fs::write(
            shared.join("shared.proto"),
            "syntax = \"proto3\";\nmessage S { string v = 1; }\n",
        )
        .expect("write");
        fs::write(
            nested.join("deep.proto"),
            "syntax = \"proto3\";\nimport \"protos/shared.proto\";\nmessage D { S s = 1; }\n",
        )
        .expect("write");

        let entry = nested.join("deep.proto");
        assert!(
            FsProtoLoader.load(&entry, &[]).is_err(),
            "the import is not under the proto's own directory"
        );
        assert!(FsProtoLoader
            .load(&entry, &[dir.path().to_path_buf()])
            .is_ok());
    }

    #[test]
    fn a_missing_proto_file_is_not_found() {
        let err = FsProtoLoader
            .load(&fixtures().join("nope.proto"), &[])
            .err()
            .expect("error");
        assert!(matches!(err, DomainError::NotFound(_)), "got: {err:?}");
    }

    #[test]
    fn imports_cannot_escape_the_include_directories() {
        let dir = TempDir::new().expect("tempdir");
        let inner = dir.path().join("inner");
        fs::create_dir_all(&inner).expect("mkdir");
        fs::write(dir.path().join("secret.proto"), "syntax = \"proto3\";\n").expect("write");
        let reader = FsProtoFileReader::new(vec![inner.clone()]);
        assert!(reader.read("../secret.proto").is_none());
        assert!(reader
            .read(&dir.path().join("secret.proto").to_string_lossy())
            .is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_leaves_the_include_directory_is_not_followed() {
        let dir = TempDir::new().expect("tempdir");
        let inner = dir.path().join("inner");
        fs::create_dir_all(&inner).expect("mkdir");
        fs::write(dir.path().join("outside.proto"), "syntax = \"proto3\";\n").expect("write");
        std::os::unix::fs::symlink(dir.path().join("outside.proto"), inner.join("link.proto"))
            .expect("symlink");
        let reader = FsProtoFileReader::new(vec![inner]);
        assert!(reader.read("link.proto").is_none());
    }
}
