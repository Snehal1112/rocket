use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use prost_reflect::{DescriptorPool, MethodDescriptor};
use prost_types::{FileDescriptorProto, FileDescriptorSet};
use protox::file::{ChainFileResolver, File, FileResolver, GoogleFileResolver};
use protox::Compiler;
use rocket_collection::GrpcMethodType;
use rocket_shared::error::{DomainError, DomainResult};
use serde::Serialize;

/// Reads `.proto` source text by import name, for example `common.proto`.
/// Returning `None` means the file is not available.
pub trait ProtoFileReader: Send + Sync {
    fn read(&self, name: &str) -> Option<String>;
}

/// Loads a registry from a `.proto` file on disk. Implemented in `rocket-infra`.
pub trait ProtoLoader: Send + Sync {
    /// `proto_file` is an absolute path. `extra_include_dirs` are searched for
    /// imports after the directory that holds `proto_file`.
    fn load(
        &self,
        proto_file: &Path,
        extra_include_dirs: &[PathBuf],
    ) -> DomainResult<ProtoRegistry>;
}

/// One RPC method, as shown in the method picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcMethodInfo {
    pub name: String,
    /// `package.Service/Method`, the value stored in `GrpcRequest.method`.
    pub full_name: String,
    pub method_type: GrpcMethodType,
    pub input_type: String,
    pub output_type: String,
}

/// One service and its methods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcServiceInfo {
    pub name: String,
    pub methods: Vec<GrpcMethodInfo>,
}

struct ReaderResolver(Arc<dyn ProtoFileReader>);

impl FileResolver for ReaderResolver {
    fn open_file(&self, name: &str) -> Result<File, protox::Error> {
        match self.0.read(name) {
            Some(source) => File::from_source(name, &source),
            None => Err(protox::Error::file_not_found(name)),
        }
    }
}

/// A set of parsed protobuf descriptors. Cheap to clone.
#[derive(Clone)]
pub struct ProtoRegistry {
    pool: DescriptorPool,
}

impl ProtoRegistry {
    /// Compiles `entry` and everything it imports. Well-known imports such as
    /// `google/protobuf/timestamp.proto` resolve without the reader.
    pub fn compile(entry: &str, reader: Arc<dyn ProtoFileReader>) -> DomainResult<Self> {
        let mut resolver = ChainFileResolver::new();
        resolver.add(ReaderResolver(reader));
        resolver.add(GoogleFileResolver::new());
        let mut compiler = Compiler::with_file_resolver(resolver);
        compiler.include_imports(true);
        compiler
            .open_file(entry)
            .map_err(|e| DomainError::InvalidInput(format!("could not compile '{entry}': {e}")))?;
        Ok(Self {
            pool: compiler.descriptor_pool(),
        })
    }

    /// Builds a registry from descriptors in any order, such as the files a
    /// reflection server returns. Every import must be in `files`, except the
    /// well-known `google/protobuf/*` files, which are added when missing.
    pub fn from_file_descriptors(files: Vec<FileDescriptorProto>) -> DomainResult<Self> {
        let mut by_name: HashMap<String, FileDescriptorProto> = HashMap::new();
        for file in files {
            by_name.insert(file.name().to_string(), file);
        }
        let mut ordered = Vec::new();
        let mut seen = HashSet::new();
        let mut names: Vec<String> = by_name.keys().cloned().collect();
        names.sort();
        for name in names {
            visit(&name, &by_name, &mut seen, &mut ordered);
        }
        let mut pool = DescriptorPool::new();
        for file in well_known_files_needed(&ordered)? {
            pool.add_file_descriptor_proto(file)
                .map_err(|e| DomainError::InvalidInput(format!("invalid descriptors: {e}")))?;
        }
        pool.add_file_descriptor_set(FileDescriptorSet { file: ordered })
            .map_err(|e| DomainError::InvalidInput(format!("invalid descriptors: {e}")))?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &DescriptorPool {
        &self.pool
    }

    /// All services with their methods, sorted by service name then source order.
    pub fn services(&self) -> Vec<GrpcServiceInfo> {
        let mut out: Vec<GrpcServiceInfo> = self
            .pool
            .services()
            .map(|service| GrpcServiceInfo {
                name: service.full_name().to_string(),
                methods: service
                    .methods()
                    .map(|m| GrpcMethodInfo {
                        name: m.name().to_string(),
                        full_name: format!("{}/{}", service.full_name(), m.name()),
                        method_type: GrpcMethodType::from_streaming_flags(
                            m.is_client_streaming(),
                            m.is_server_streaming(),
                        ),
                        input_type: m.input().full_name().to_string(),
                        output_type: m.output().full_name().to_string(),
                    })
                    .collect(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Finds a method by `package.Service/Method`. A leading `/` is accepted.
    pub fn method(&self, full_name: &str) -> DomainResult<MethodDescriptor> {
        let trimmed = full_name.trim_start_matches('/');
        let (service, method) = trimmed.split_once('/').ok_or_else(|| {
            DomainError::InvalidInput(format!(
                "method '{full_name}' must look like package.Service/Method"
            ))
        })?;
        self.pool
            .get_service_by_name(service)
            .and_then(|s| s.methods().find(|m| m.name() == method))
            .ok_or_else(|| DomainError::NotFound(format!("gRPC method '{trimmed}'")))
    }
}

fn visit(
    name: &str,
    files: &HashMap<String, FileDescriptorProto>,
    seen: &mut HashSet<String>,
    out: &mut Vec<FileDescriptorProto>,
) {
    if !seen.insert(name.to_string()) {
        return;
    }
    if let Some(file) = files.get(name) {
        for dependency in &file.dependency {
            visit(dependency, files, seen, out);
        }
        out.push(file.clone());
    }
}

/// Returns the well-known files that `ordered` imports but does not contain.
fn well_known_files_needed(
    ordered: &[FileDescriptorProto],
) -> DomainResult<Vec<FileDescriptorProto>> {
    let present: HashSet<&str> = ordered.iter().map(|f| f.name()).collect();
    let mut missing: Vec<String> = ordered
        .iter()
        .flat_map(|f| f.dependency.iter().cloned())
        .filter(|d| d.starts_with("google/protobuf/") && !present.contains(d.as_str()))
        .collect();
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        return Ok(Vec::new());
    }
    let mut compiler = Compiler::with_file_resolver(GoogleFileResolver::new());
    compiler.include_imports(true);
    for name in &missing {
        compiler
            .open_file(name)
            .map_err(|e| DomainError::InvalidInput(format!("could not load '{name}': {e}")))?;
    }
    Ok(compiler.file_descriptor_set().file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{greeter_registry, reader, COMMON_PROTO, GREETER_PROTO};

    #[test]
    fn lists_services_methods_and_their_call_shapes() {
        let services = greeter_registry().services();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "demo.greeter.v1.Greeter");
        let shapes: Vec<(&str, GrpcMethodType)> = services[0]
            .methods
            .iter()
            .map(|m| (m.name.as_str(), m.method_type))
            .collect();
        assert_eq!(
            shapes,
            vec![
                ("SayHello", GrpcMethodType::Unary),
                ("ListGreetings", GrpcMethodType::ServerStreaming),
                ("CollectNames", GrpcMethodType::ClientStreaming),
                ("Chat", GrpcMethodType::BidiStreaming),
            ]
        );
        assert_eq!(
            services[0].methods[0].full_name,
            "demo.greeter.v1.Greeter/SayHello"
        );
        assert_eq!(
            services[0].methods[0].input_type,
            "demo.greeter.v1.HelloRequest"
        );
    }

    #[test]
    fn method_lookup_accepts_a_leading_slash_and_rejects_bad_names() {
        let registry = greeter_registry();
        assert!(registry.method("/demo.greeter.v1.Greeter/Chat").is_ok());
        assert!(matches!(
            registry.method("demo.greeter.v1.Greeter"),
            Err(DomainError::InvalidInput(_))
        ));
        assert!(matches!(
            registry.method("demo.greeter.v1.Greeter/Nope"),
            Err(DomainError::NotFound(_))
        ));
        assert!(matches!(
            registry.method("other.Service/Chat"),
            Err(DomainError::NotFound(_))
        ));
    }

    #[test]
    fn a_missing_import_is_an_invalid_input_that_names_the_file() {
        let result =
            ProtoRegistry::compile("greeter.proto", reader(&[("greeter.proto", GREETER_PROTO)]));
        match result {
            Err(DomainError::InvalidInput(msg)) => {
                assert!(msg.contains("common.proto"), "got: {msg}");
                assert!(msg.contains("greeter.proto"), "got: {msg}");
            }
            other => panic!("expected InvalidInput, got {:?}", other.err()),
        }
    }

    #[test]
    fn a_syntax_error_is_an_invalid_input() {
        let result = ProtoRegistry::compile(
            "bad.proto",
            reader(&[("bad.proto", "syntax = \"proto3\";\nmessage {")]),
        );
        assert!(matches!(result, Err(DomainError::InvalidInput(_))));
    }

    #[test]
    fn an_entry_that_the_reader_does_not_have_is_an_error() {
        let result = ProtoRegistry::compile("nope.proto", reader(&[]));
        assert!(matches!(result, Err(DomainError::InvalidInput(_))));
    }

    #[test]
    fn descriptors_in_reverse_order_still_build_a_registry() {
        let compiled = ProtoRegistry::compile(
            "greeter.proto",
            reader(&[
                ("greeter.proto", GREETER_PROTO),
                ("common.proto", COMMON_PROTO),
            ]),
        )
        .expect("compile");
        let mut files: Vec<FileDescriptorProto> =
            compiled.pool().file_descriptor_protos().cloned().collect();
        files.reverse();
        let rebuilt = ProtoRegistry::from_file_descriptors(files).expect("rebuild");
        assert_eq!(rebuilt.services(), compiled.services());
    }

    #[test]
    fn missing_well_known_files_are_added_when_rebuilding() {
        let compiled = ProtoRegistry::compile(
            "greeter.proto",
            reader(&[
                ("greeter.proto", GREETER_PROTO),
                ("common.proto", COMMON_PROTO),
            ]),
        )
        .expect("compile");
        let files: Vec<FileDescriptorProto> = compiled
            .pool()
            .file_descriptor_protos()
            .filter(|f| !f.name().starts_with("google/protobuf/"))
            .cloned()
            .collect();
        let rebuilt = ProtoRegistry::from_file_descriptors(files).expect("rebuild");
        assert!(rebuilt
            .pool()
            .get_message_by_name("google.protobuf.Timestamp")
            .is_some());
    }
}
