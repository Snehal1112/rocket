# Protocol parity, Plan 04: SOAP via WSDL import

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user import a WSDL 1.1 file and get a collection with one ready-to-send SOAP request per operation (envelope body, correct `Content-Type`, correct `SOAPAction`), matching what Bruno's WSDL import offers.

**Architecture:** SOAP stays a plain HTTP `POST` with `BodyMode::Xml`, exactly as in Bruno. No executor, domain or persistence change. A new `wsdl` module in `rocket-import` parses WSDL 1.1 plus the local XSD files it imports into owned structures and renders sample XML from the schema. A new `converter::wsdl` turns each SOAP operation into a `rocket_collection::Request` and sets the SOAP 1.1 or 1.2 headers explicitly (the executor already skips its `text/xml` default when a `Content-Type` header is present). `ImportService::import_wsdl` writes `<service>/<port>/<operation>` through `CollectionRepository`. One new Tauri command and a third source in the existing import dialog expose it.

**Tech Stack:** Rust, `roxmltree` (new dependency, see below), `cargo test -j4 -p rocket-import <name>`, Tauri command in `src-tauri`, React + TypeScript, shadcn/ui, lucide-react, Vitest.

**Spec:** [../../specs/opencollection-spec-reference.md](../../specs/opencollection-spec-reference.md) (request shape, headers, `body.type: xml`), [../../specs/2026-04-04-bruno-import-design.md](../../specs/2026-04-04-bruno-import-design.md) (importer layering, `ImportReport` semantics). Bruno behaviour reference: https://docs.usebruno.com/converters/wsdl-to-bruno.

## Facts verified against the repo (do not re-derive)

- `rocket-import` has two importers, Bruno (`ImportService::import_auto*`) and Postman (`import_postman_collection`, `import_postman_environment`). There is no WSDL path.
- `ImportService` fields: `workspace_path`, `collection_repo: Box<dyn CollectionRepository>`, `env_factory`. Helpers reused here: `resolve_collection_name(&self, &str) -> ImportResult<String>` and the free function `sanitize_postman_filename(&str) -> String`, both private to `crates/rocket-import/src/importer.rs` (the new method lives in the same file, so both are in scope).
- `CollectionRepository::create(&str)`, `create_folder(collection, path)`, `save_request(collection, path, &Request) -> DomainResult<String>`. `save_request` returns the real file name, which may differ from `path`, so tests locate files by walking the directory instead of guessing extensions.
- `ImportError` has `ParseError { path: PathBuf, message: String }` and `IoError`. `SkipReason` has `UnsupportedRequestType(String)`, `UnsupportedAuthType(String)`, `ParseError(String)` and is mirrored by a TypeScript union in `src/lib/tauri-api.ts`. This plan adds no new variant, so the TypeScript union does not change.
- `ReqwestExecutor::apply_body` (`crates/rocket-infra/src/reqwest_executor.rs`) sets `Content-Type: text/xml` for `BodyMode::Xml` only when `has_explicit_content_type` is false. An explicit header wins, which is why the importer sets the header and the executor is left alone.
- Tauri commands live in `src-tauri/src/commands/import.rs` (`make_import_service(base)`, workspace path from `State<'_, Arc<Mutex<PathBuf>>>`) and are registered in `src-tauri/src/lib.rs` next to `commands::import::import_postman_collection`.
- Frontend: `importPostmanCollection` etc. in `src/lib/tauri-api.ts` (section "Collection import"), dialog in `src/components/import/ImportCollectionDialog.tsx` (`ImportSource = 'bruno' | 'postman'`, `SourceKind = 'folder' | 'zip' | 'postman-json'`). No test file exists for the dialog yet. Vitest + Testing Library are installed.

## XML crate decision

`roxmltree = "0.20"` is a new dependency of `rocket-import` only. Reasons:

- `quick-xml` is in `Cargo.lock` (0.37.5 and 0.38.4) but only transitively through `plist` and `tauri-winrt-notification`. It is not a normal dependency of any workspace crate, so using it adds no real saving, and it is a pull parser: namespace stacks, QName-valued attribute resolution (`type="t:Operands"`) and tree walking would all be hand-written.
- `roxmltree` is a read-only DOM with namespace resolution built in (`tag_name().namespace()`, `lookup_namespace_uri`), zero dependencies, and it rejects `<!DOCTYPE>` by default, which removes XXE and entity-expansion (billion laughs) from a parser that reads user-supplied files.
- The user should confirm the new dependency (see the report to the caller).

## Global Constraints

- Always pass `-j4` to cargo: `cargo test -j4 -p rocket-import <name>`, `cargo check -j4`. Never run `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill (never a freeform `git commit -m`), stage by explicit path only (peer sessions share this repo's index), conventional commit prefixes.
- Production code never uses `unwrap()` or `expect()`. Tests may use `.expect("reason")`.
- No `#[serde(rename_all = "camelCase")]` on persistence structs. This plan adds no serde types.
- Do not change executor defaults. SOAP headers are set by the importer, per binding version.
- The importer never fetches over the network. Remote `schemaLocation` or `location` values are recorded as report items, not fetched.
- Frontend: shadcn/ui primitives and `lucide-react` icons only. Do not add raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>` or emoji icons in new code. Do not fully destructure Zustand state (the dialog uses none).
- Verification before each commit that touches the area: `cargo check -j4`, the focused `cargo test -j4 -p rocket-import <name>`, and for Task 3 also `yarn tsc --noEmit` and `yarn check`.
- Code comments are short full sentences ending with a period.

## Review Focus

Five inputs most likely to break this feature. Each has a pinning test in the owning task.

1. **Namespace prefixes are arbitrary.** Real WSDLs use `w:`, `s:`, `soap12:`, `xs:`, `xsd:` interchangeably, and QName values (`type="t:Operands"`, `binding="tns:CalcSoap"`) must resolve through the in-scope declarations, never by prefix text. Pinned by `prefix_names_do_not_matter` (Task 1).
2. **Recursive and deeply nested XSD types.** A self-referencing type or a long anonymous chain must terminate with a marker, not overflow the stack. Pinned by `recursive_type_terminates_with_marker` and `deep_nesting_hits_depth_limit` (Task 1).
3. **SOAP 1.1 vs 1.2 headers.** 1.1 needs `Content-Type: text/xml; charset=utf-8` plus a quoted `SOAPAction`. 1.2 needs `application/soap+xml; charset=utf-8; action="..."` and must NOT send a `SOAPAction` header. Envelope namespaces differ. Pinned by `soap11_headers`, `soap12_headers_have_no_soapaction_header` and `envelope_namespace_follows_version` (Task 2).
4. **Document/literal vs rpc/literal.** Document style puts the part's element directly in `Body`. RPC style wraps the parts in an element named after the operation, in the `soap:body namespace`, with unqualified part-name children. Pinned by `rpc_binding_is_detected` (Task 1) and `rpc_envelope_wraps_parts_in_operation_element` (Task 2).
5. **Untrusted XML in, XML out.** `default`/`fixed`/enumeration values and a hostile `<!DOCTYPE>` come from the file. Sample values must be escaped, and DTDs must be rejected. Pinned by `sample_values_are_escaped` and `dtd_is_rejected` (Task 1). Operation-name collisions inside one port (overloaded operations) must not overwrite each other, pinned by `duplicate_operation_names_get_suffixes` (Task 2).

## File Structure

| File | Task | Responsibility |
|---|---|---|
| `crates/rocket-import/Cargo.toml` | 1 | add `roxmltree` |
| `crates/rocket-import/src/lib.rs` | 1 | `pub(crate) mod wsdl;` |
| `crates/rocket-import/src/wsdl/mod.rs` | 1 | module wiring, re-exports |
| `crates/rocket-import/src/wsdl/ast.rs` | 1 | `QName`, `SoapVersion`, model structs, `qname_attr` |
| `crates/rocket-import/src/wsdl/schema.rs` | 1 | owned XSD model + `SchemaSet::add_schema` |
| `crates/rocket-import/src/wsdl/sampler.rs` | 1 | sample XML generation, escaping |
| `crates/rocket-import/src/wsdl/parser.rs` | 1 | source loading, WSDL 1.1 parse, `parse_wsdl_file`, `parse_wsdl_str` |
| `crates/rocket-import/tests/fixtures/wsdl/calc.wsdl`, `calc-types.xsd` | 1 | fixture with SOAP 1.1 and 1.2 ports and an imported XSD |
| `crates/rocket-import/src/converter/wsdl.rs` | 2 | envelope, headers, `Request` per operation |
| `crates/rocket-import/src/converter/mod.rs` | 2 | `pub(crate) mod wsdl;` |
| `crates/rocket-import/src/importer.rs` | 2 | `ImportService::import_wsdl` |
| `src-tauri/src/commands/import.rs`, `src-tauri/src/lib.rs` | 3 | `import_wsdl` command and registration |
| `src/lib/tauri-api.ts` | 3 | `importWsdl` |
| `src/components/import/importSources.ts` | 3 | pure helpers for source labels |
| `src/components/import/ImportCollectionDialog.tsx` | 3 | third source, WSDL |
| `src/components/import/ImportCollectionDialog.test.tsx` | 3 | dialog test |
| `crates/rocket-import/tests/wsdl_integration_test.rs` | 3 | end-to-end through `FsCollectionRepo` |
| `crates/rocket-import/CLAUDE.md` | 3 | module map and WSDL notes |

---

## Task 1: WSDL parser and schema sampler in `rocket-import`

**Files:**
- Modify: `crates/rocket-import/Cargo.toml`, `crates/rocket-import/src/lib.rs`
- Create: `crates/rocket-import/src/wsdl/{mod,ast,schema,sampler,parser}.rs`
- Create: `crates/rocket-import/tests/fixtures/wsdl/calc.wsdl`, `crates/rocket-import/tests/fixtures/wsdl/calc-types.xsd`

**Interfaces:**
- Consumes: `crate::error::{ImportError, ImportResult}`.
- Produces (all `pub(crate)`):
  - `wsdl::parse_wsdl_file(path: &Path) -> ImportResult<WsdlModel>`
  - `wsdl::parse_wsdl_str(text: &str, origin: &Path) -> ImportResult<WsdlModel>`
  - `WsdlModel { services: Vec<WsdlService>, schemas: SchemaSet, warnings: Vec<String> }`
  - `WsdlService { name, ports: Vec<WsdlPort> }`, `WsdlPort { name, address: String, soap: SoapVersion, operations: Vec<WsdlOperation> }`
  - `WsdlOperation { name, soap_action: String, style: BindingStyle, rpc_namespace: Option<String>, input_parts: Vec<MessagePart> }`
  - `MessagePart { name: String, kind: PartKind }`, `PartKind::{Element(QName), Type(QName)}`
  - `Sampler::new(&SchemaSet)`, `.element(&QName) -> String`, `.typed_element(&str, &QName) -> String`, `.wrap(ns, local, inner) -> String`, `.namespaces() -> &BTreeMap<String, String>`, plus `esc`, `esc_attr`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Add the dependency and module stubs**

In `crates/rocket-import/Cargo.toml` under `[dependencies]` add:

```toml
roxmltree = "0.20"
```

In `crates/rocket-import/src/lib.rs` add `pub(crate) mod wsdl;` after `pub mod report;`.

Create `crates/rocket-import/src/wsdl/mod.rs`:

```rust
//! WSDL 1.1 reader: services, ports, SOAP operations and sample XML from XSD.

pub(crate) mod ast;
pub(crate) mod parser;
pub(crate) mod sampler;
pub(crate) mod schema;

pub(crate) use ast::{
    BindingStyle, MessagePart, PartKind, QName, SoapVersion, WsdlModel, WsdlOperation, WsdlPort,
    WsdlService,
};
pub(crate) use parser::{parse_wsdl_file, parse_wsdl_str};
pub(crate) use sampler::{esc, esc_attr, Sampler};
pub(crate) use schema::SchemaSet;
```

- [ ] **Step 3: Write the fixtures**

`crates/rocket-import/tests/fixtures/wsdl/calc-types.xsd`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:t="http://example.com/types"
           targetNamespace="http://example.com/types"
           elementFormDefault="qualified">
  <xs:complexType name="Operands">
    <xs:sequence>
      <xs:element name="a" type="xs:int"/>
      <xs:element name="b" type="xs:int"/>
      <xs:element name="mode" type="t:Mode" minOccurs="0"/>
    </xs:sequence>
  </xs:complexType>
  <xs:simpleType name="Mode">
    <xs:restriction base="xs:string">
      <xs:enumeration value="fast"/>
      <xs:enumeration value="exact"/>
    </xs:restriction>
  </xs:simpleType>
</xs:schema>
```

`crates/rocket-import/tests/fixtures/wsdl/calc.wsdl`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
                  xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
                  xmlns:soap12="http://schemas.xmlsoap.org/wsdl/soap12/"
                  xmlns:xsd="http://www.w3.org/2001/XMLSchema"
                  xmlns:t="http://example.com/types"
                  xmlns:tns="http://example.com/calc"
                  targetNamespace="http://example.com/calc">
  <wsdl:types>
    <xsd:schema targetNamespace="http://example.com/calc" elementFormDefault="qualified">
      <xsd:import namespace="http://example.com/types" schemaLocation="calc-types.xsd"/>
      <xsd:element name="Add">
        <xsd:complexType>
          <xsd:sequence>
            <xsd:element name="operands" type="t:Operands"/>
          </xsd:sequence>
        </xsd:complexType>
      </xsd:element>
      <xsd:element name="Subtract">
        <xsd:complexType>
          <xsd:sequence>
            <xsd:element name="operands" type="t:Operands"/>
          </xsd:sequence>
        </xsd:complexType>
      </xsd:element>
      <xsd:element name="Result" type="xsd:int"/>
    </xsd:schema>
  </wsdl:types>
  <wsdl:message name="AddIn"><wsdl:part name="parameters" element="tns:Add"/></wsdl:message>
  <wsdl:message name="AddOut"><wsdl:part name="parameters" element="tns:Result"/></wsdl:message>
  <wsdl:message name="SubtractIn"><wsdl:part name="parameters" element="tns:Subtract"/></wsdl:message>
  <wsdl:message name="SubtractOut"><wsdl:part name="parameters" element="tns:Result"/></wsdl:message>
  <wsdl:portType name="CalcPortType">
    <wsdl:operation name="Add">
      <wsdl:input message="tns:AddIn"/>
      <wsdl:output message="tns:AddOut"/>
    </wsdl:operation>
    <wsdl:operation name="Subtract">
      <wsdl:input message="tns:SubtractIn"/>
      <wsdl:output message="tns:SubtractOut"/>
    </wsdl:operation>
  </wsdl:portType>
  <wsdl:binding name="CalcSoap" type="tns:CalcPortType">
    <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <wsdl:operation name="Add">
      <soap:operation soapAction="http://example.com/calc/Add"/>
      <wsdl:input><soap:body use="literal"/></wsdl:input>
      <wsdl:output><soap:body use="literal"/></wsdl:output>
    </wsdl:operation>
    <wsdl:operation name="Subtract">
      <soap:operation soapAction="http://example.com/calc/Subtract"/>
      <wsdl:input><soap:body use="literal"/></wsdl:input>
      <wsdl:output><soap:body use="literal"/></wsdl:output>
    </wsdl:operation>
  </wsdl:binding>
  <wsdl:binding name="CalcSoap12" type="tns:CalcPortType">
    <soap12:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <wsdl:operation name="Add">
      <soap12:operation soapAction="http://example.com/calc/Add"/>
      <wsdl:input><soap12:body use="literal"/></wsdl:input>
      <wsdl:output><soap12:body use="literal"/></wsdl:output>
    </wsdl:operation>
    <wsdl:operation name="Subtract">
      <soap12:operation soapAction="http://example.com/calc/Subtract"/>
      <wsdl:input><soap12:body use="literal"/></wsdl:input>
      <wsdl:output><soap12:body use="literal"/></wsdl:output>
    </wsdl:operation>
  </wsdl:binding>
  <wsdl:service name="Calculator">
    <wsdl:port name="CalcSoap" binding="tns:CalcSoap">
      <soap:address location="http://localhost:8080/calc"/>
    </wsdl:port>
    <wsdl:port name="CalcSoap12" binding="tns:CalcSoap12">
      <soap12:address location="http://localhost:8080/calc12"/>
    </wsdl:port>
  </wsdl:service>
</wsdl:definitions>
```

- [ ] **Step 4: Write the failing parser tests**

Create `crates/rocket-import/src/wsdl/parser.rs` containing only the tests module for now (the implementation arrives in Step 6). Add at the bottom of the file (the file will also hold the implementation later):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::wsdl::{BindingStyle, PartKind, SoapVersion};
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/wsdl")
            .join(name)
    }

    #[test]
    fn parses_services_ports_and_operations() {
        let model = parse_wsdl_file(&fixture("calc.wsdl")).expect("fixture parses");
        assert_eq!(model.services.len(), 1);
        let svc = &model.services[0];
        assert_eq!(svc.name, "Calculator");
        assert_eq!(svc.ports.len(), 2);

        let p11 = &svc.ports[0];
        assert_eq!(p11.name, "CalcSoap");
        assert_eq!(p11.soap, SoapVersion::V11);
        assert_eq!(p11.address, "http://localhost:8080/calc");
        let names: Vec<&str> = p11.operations.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, vec!["Add", "Subtract"]);
        assert_eq!(p11.operations[0].soap_action, "http://example.com/calc/Add");
        assert_eq!(p11.operations[0].style, BindingStyle::Document);

        let p12 = &svc.ports[1];
        assert_eq!(p12.soap, SoapVersion::V12);
        assert_eq!(p12.address, "http://localhost:8080/calc12");
    }

    #[test]
    fn input_part_points_at_the_wrapper_element() {
        let model = parse_wsdl_file(&fixture("calc.wsdl")).expect("fixture parses");
        let part = &model.services[0].ports[0].operations[0].input_parts[0];
        assert_eq!(part.name, "parameters");
        match &part.kind {
            PartKind::Element(q) => {
                assert_eq!(q.ns, "http://example.com/calc");
                assert_eq!(q.local, "Add");
            }
            other => panic!("expected an element part, got {other:?}"),
        }
    }

    #[test]
    fn imported_xsd_types_are_loaded() {
        let model = parse_wsdl_file(&fixture("calc.wsdl")).expect("fixture parses");
        let operands = crate::wsdl::QName::new("http://example.com/types", "Operands");
        assert!(model.schemas.types.contains_key(&operands));
        assert!(model.warnings.is_empty(), "got: {:?}", model.warnings);
    }

    const WEIRD_PREFIXES: &str = r#"<w:definitions xmlns:w="http://schemas.xmlsoap.org/wsdl/"
        xmlns:s="http://schemas.xmlsoap.org/wsdl/soap/"
        xmlns:q="urn:echo" targetNamespace="urn:echo">
      <w:message name="EchoIn"><w:part name="msg" type="q:Text"/></w:message>
      <w:portType name="P"><w:operation name="Echo"><w:input message="q:EchoIn"/></w:operation></w:portType>
      <w:binding name="B" type="q:P">
        <s:binding style="document"/>
        <w:operation name="Echo"><s:operation soapAction="urn:echo#Echo"/>
          <w:input><s:body use="literal"/></w:input></w:operation>
      </w:binding>
      <w:service name="S"><w:port name="Pt" binding="q:B"><s:address location="http://h/echo"/></w:port></w:service>
    </w:definitions>"#;

    #[test]
    fn prefix_names_do_not_matter() {
        let model = parse_wsdl_str(WEIRD_PREFIXES, Path::new("weird.wsdl")).expect("parses");
        let port = &model.services[0].ports[0];
        assert_eq!(port.soap, SoapVersion::V11);
        assert_eq!(port.operations.len(), 1);
        assert_eq!(port.operations[0].soap_action, "urn:echo#Echo");
        match &port.operations[0].input_parts[0].kind {
            PartKind::Type(q) => assert_eq!((q.ns.as_str(), q.local.as_str()), ("urn:echo", "Text")),
            other => panic!("expected a type part, got {other:?}"),
        }
    }

    const RPC: &str = r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
        xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
        xmlns:xsd="http://www.w3.org/2001/XMLSchema"
        xmlns:tns="urn:echo" targetNamespace="urn:echo">
      <message name="EchoIn"><part name="msg" type="xsd:string"/></message>
      <portType name="P"><operation name="Echo"><input message="tns:EchoIn"/></operation></portType>
      <binding name="B" type="tns:P">
        <soap:binding style="rpc"/>
        <operation name="Echo"><soap:operation soapAction=""/>
          <input><soap:body use="literal" namespace="urn:echo-rpc"/></input></operation>
      </binding>
      <service name="S"><port name="Pt" binding="tns:B"><soap:address location="http://h/echo"/></port></service>
    </definitions>"#;

    #[test]
    fn rpc_binding_is_detected() {
        let model = parse_wsdl_str(RPC, Path::new("rpc.wsdl")).expect("parses");
        let op = &model.services[0].ports[0].operations[0];
        assert_eq!(op.style, BindingStyle::Rpc);
        assert_eq!(op.rpc_namespace.as_deref(), Some("urn:echo-rpc"));
        assert_eq!(op.soap_action, "");
    }

    #[test]
    fn non_soap_binding_is_reported_not_imported() {
        let wsdl = RPC.replace(
            r#"<soap:binding style="rpc"/>"#,
            r#"<http:binding xmlns:http="http://schemas.xmlsoap.org/wsdl/http/" verb="GET"/>"#,
        );
        let model = parse_wsdl_str(&wsdl, Path::new("http.wsdl")).expect("parses");
        assert!(model.services[0].ports.is_empty());
        assert!(
            model.warnings.iter().any(|w| w.contains("non-SOAP")),
            "got: {:?}",
            model.warnings
        );
    }

    #[test]
    fn wsdl2_is_rejected_with_a_clear_message() {
        let wsdl = r#"<description xmlns="http://www.w3.org/ns/wsdl" targetNamespace="urn:x"/>"#;
        let err = parse_wsdl_str(wsdl, Path::new("v2.wsdl")).expect_err("must fail");
        assert!(err.to_string().contains("WSDL 2.0"), "got: {err}");
    }

    #[test]
    fn dtd_is_rejected() {
        let wsdl = r#"<?xml version="1.0"?>
<!DOCTYPE d [<!ENTITY x "boom">]>
<definitions xmlns="http://schemas.xmlsoap.org/wsdl/">&x;</definitions>"#;
        assert!(parse_wsdl_str(wsdl, Path::new("dtd.wsdl")).is_err());
    }

    #[test]
    fn remote_imports_are_warned_not_fetched() {
        let wsdl = r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/" targetNamespace="urn:r">
          <import namespace="urn:o" location="http://example.invalid/other.wsdl"/>
        </definitions>"#;
        let model = parse_wsdl_str(wsdl, Path::new("remote.wsdl")).expect("parses");
        assert!(
            model.warnings.iter().any(|w| w.contains("not fetched")),
            "got: {:?}",
            model.warnings
        );
    }
}
```

Note: `parse_wsdl_str(&wsdl, ...)` takes `&str`, so passing `&String` works through deref.

- [ ] **Step 5: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-import wsdl::parser`
Expected: FAIL to compile (`parse_wsdl_file`, `wsdl::ast` and friends do not exist yet).

- [ ] **Step 6: Implement the model, the schema reader and the parser**

`crates/rocket-import/src/wsdl/ast.rs`:

```rust
use roxmltree::Node;

use super::schema::SchemaSet;

pub(crate) const WSDL_NS: &str = "http://schemas.xmlsoap.org/wsdl/";
pub(crate) const WSDL2_NS: &str = "http://www.w3.org/ns/wsdl";
pub(crate) const SOAP11_NS: &str = "http://schemas.xmlsoap.org/wsdl/soap/";
pub(crate) const SOAP12_NS: &str = "http://schemas.xmlsoap.org/wsdl/soap12/";
pub(crate) const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema";
pub(crate) const SOAP11_ENVELOPE_NS: &str = "http://schemas.xmlsoap.org/soap/envelope/";
pub(crate) const SOAP12_ENVELOPE_NS: &str = "http://www.w3.org/2003/05/soap-envelope";

/// A namespace-qualified name. Prefixes never appear here.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct QName {
    pub ns: String,
    pub local: String,
}

impl QName {
    pub(crate) fn new(ns: impl Into<String>, local: impl Into<String>) -> Self {
        Self {
            ns: ns.into(),
            local: local.into(),
        }
    }
}

/// Resolve a QName-valued attribute (`type="t:Foo"`) through the in-scope namespaces.
pub(crate) fn qname_attr(node: Node, attr: &str) -> Option<QName> {
    let raw = node.attribute(attr)?;
    let (prefix, local) = match raw.split_once(':') {
        Some((p, l)) => (Some(p), l),
        None => (None, raw),
    };
    let ns = node.lookup_namespace_uri(prefix).unwrap_or("");
    Some(QName::new(ns, local))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SoapVersion {
    V11,
    V12,
}

impl SoapVersion {
    pub(crate) fn envelope_ns(self) -> &'static str {
        match self {
            SoapVersion::V11 => SOAP11_ENVELOPE_NS,
            SoapVersion::V12 => SOAP12_ENVELOPE_NS,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            SoapVersion::V11 => "SOAP 1.1",
            SoapVersion::V12 => "SOAP 1.2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingStyle {
    Document,
    Rpc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PartKind {
    Element(QName),
    Type(QName),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessagePart {
    pub name: String,
    pub kind: PartKind,
}

#[derive(Debug, Clone)]
pub(crate) struct WsdlOperation {
    pub name: String,
    /// Empty when the binding has no `soapAction`.
    pub soap_action: String,
    pub style: BindingStyle,
    /// The `namespace` attribute of the input `soap:body`, used by RPC style.
    pub rpc_namespace: Option<String>,
    pub input_parts: Vec<MessagePart>,
}

#[derive(Debug, Clone)]
pub(crate) struct WsdlPort {
    pub name: String,
    pub address: String,
    pub soap: SoapVersion,
    pub operations: Vec<WsdlOperation>,
}

#[derive(Debug, Clone)]
pub(crate) struct WsdlService {
    pub name: String,
    pub ports: Vec<WsdlPort>,
}

#[derive(Debug)]
pub(crate) struct WsdlModel {
    pub services: Vec<WsdlService>,
    pub schemas: SchemaSet,
    /// Non-fatal problems. The converter turns these into report items.
    pub warnings: Vec<String>,
}
```

`crates/rocket-import/src/wsdl/schema.rs`:

```rust
use std::collections::HashMap;

use roxmltree::Node;

use super::ast::{qname_attr, QName, XSD_NS};

#[derive(Debug, Clone)]
pub(crate) enum ElementType {
    Named(QName),
    Inline(Box<TypeDef>),
    Unspecified,
}

#[derive(Debug, Clone)]
pub(crate) struct ElementDecl {
    pub name: String,
    /// Empty for unqualified local elements.
    pub ns: String,
    pub ty: ElementType,
    /// Set for `<element ref="...">`. The sampler follows it to the top-level declaration.
    pub reference: Option<QName>,
    /// `fixed` wins over `default`.
    pub value: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum TypeDef {
    Simple(SimpleType),
    Complex(ComplexType),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SimpleType {
    pub first_enum: Option<String>,
    pub base: Option<QName>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ComplexType {
    /// `complexContent/extension@base`.
    pub base: Option<QName>,
    /// `simpleContent/*@base`.
    pub simple_base: Option<QName>,
    /// Flattened particles. A `choice` contributes only its first alternative.
    pub particles: Vec<ElementDecl>,
}

/// Owned XSD declarations gathered from every schema in the WSDL and its imports.
#[derive(Debug, Default)]
pub(crate) struct SchemaSet {
    pub elements: HashMap<QName, ElementDecl>,
    pub types: HashMap<QName, TypeDef>,
}

fn is_xsd(n: &Node, name: &str) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(XSD_NS) && n.tag_name().name() == name
}

impl SchemaSet {
    /// Read one `xsd:schema` element. Attributes, wildcards and groups are ignored.
    pub(crate) fn add_schema(&mut self, schema: Node) {
        let tns = schema.attribute("targetNamespace").unwrap_or("").to_string();
        let qualified = schema.attribute("elementFormDefault") == Some("qualified");
        for child in schema.children().filter(|n| n.is_element()) {
            if is_xsd(&child, "element") {
                if let Some(decl) = parse_element(child, &tns, qualified, true) {
                    self.elements.insert(QName::new(tns.clone(), decl.name.clone()), decl);
                }
            } else if is_xsd(&child, "complexType") {
                if let Some(name) = child.attribute("name") {
                    self.types.insert(
                        QName::new(tns.clone(), name),
                        TypeDef::Complex(parse_complex(child, &tns, qualified)),
                    );
                }
            } else if is_xsd(&child, "simpleType") {
                if let Some(name) = child.attribute("name") {
                    self.types
                        .insert(QName::new(tns.clone(), name), TypeDef::Simple(parse_simple(child)));
                }
            }
        }
    }
}

fn parse_element(node: Node, tns: &str, qualified_default: bool, top: bool) -> Option<ElementDecl> {
    let value = node
        .attribute("fixed")
        .or_else(|| node.attribute("default"))
        .map(str::to_string);
    if let Some(r) = qname_attr(node, "ref") {
        return Some(ElementDecl {
            name: r.local.clone(),
            ns: r.ns.clone(),
            ty: ElementType::Unspecified,
            reference: Some(r),
            value,
        });
    }
    let name = node.attribute("name")?.to_string();
    let qualified = top
        || match node.attribute("form") {
            Some("qualified") => true,
            Some("unqualified") => false,
            _ => qualified_default,
        };
    let ns = if qualified { tns.to_string() } else { String::new() };
    let ty = if let Some(q) = qname_attr(node, "type") {
        ElementType::Named(q)
    } else if let Some(c) = node.children().find(|n| is_xsd(n, "complexType")) {
        ElementType::Inline(Box::new(TypeDef::Complex(parse_complex(c, tns, qualified_default))))
    } else if let Some(s) = node.children().find(|n| is_xsd(n, "simpleType")) {
        ElementType::Inline(Box::new(TypeDef::Simple(parse_simple(s))))
    } else {
        ElementType::Unspecified
    };
    Some(ElementDecl {
        name,
        ns,
        ty,
        reference: None,
        value,
    })
}

fn handle_particle(n: Node, tns: &str, q: bool, out: &mut Vec<ElementDecl>) {
    if is_xsd(&n, "element") {
        if let Some(d) = parse_element(n, tns, q, false) {
            out.push(d);
        }
    } else if is_xsd(&n, "sequence") || is_xsd(&n, "all") {
        for c in n.children().filter(|c| c.is_element()) {
            handle_particle(c, tns, q, out);
        }
    } else if is_xsd(&n, "choice") {
        if let Some(c) = n.children().find(|c| c.is_element()) {
            handle_particle(c, tns, q, out);
        }
    }
}

fn parse_complex(node: Node, tns: &str, q: bool) -> ComplexType {
    let mut ct = ComplexType::default();
    for child in node.children().filter(|n| n.is_element()) {
        if is_xsd(&child, "complexContent") {
            for d in child.children().filter(|n| is_xsd(n, "extension")) {
                ct.base = qname_attr(d, "base");
                for p in d.children().filter(|n| n.is_element()) {
                    handle_particle(p, tns, q, &mut ct.particles);
                }
            }
        } else if is_xsd(&child, "simpleContent") {
            for d in child
                .children()
                .filter(|n| is_xsd(n, "extension") || is_xsd(n, "restriction"))
            {
                ct.simple_base = qname_attr(d, "base");
            }
        } else {
            handle_particle(child, tns, q, &mut ct.particles);
        }
    }
    ct
}

fn parse_simple(node: Node) -> SimpleType {
    let mut st = SimpleType::default();
    if let Some(r) = node.children().find(|n| is_xsd(n, "restriction")) {
        st.base = qname_attr(r, "base");
        st.first_enum = r
            .children()
            .find(|n| is_xsd(n, "enumeration"))
            .and_then(|e| e.attribute("value"))
            .map(str::to_string);
    }
    st
}
```

`crates/rocket-import/src/wsdl/sampler.rs` (tests are added in Step 8, implementation now):

```rust
use std::collections::BTreeMap;

use super::ast::{QName, XSD_NS};
use super::schema::{ElementDecl, ElementType, SchemaSet, SimpleType, TypeDef};

/// Element nesting beyond this depth is replaced by a marker comment.
pub(crate) const MAX_DEPTH: usize = 8;

/// Escape text content.
pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Escape an attribute value.
pub(crate) fn esc_attr(s: &str) -> String {
    esc(s).replace('"', "&quot;")
}

/// Type names may legally contain `--`, which is illegal inside an XML comment.
fn comment_safe(s: &str) -> String {
    s.replace("--", "-")
}

fn builtin_sample(local: &str) -> &'static str {
    match local {
        "boolean" => "false",
        "int" | "integer" | "long" | "short" | "byte" | "unsignedInt" | "unsignedLong"
        | "unsignedShort" | "unsignedByte" | "nonNegativeInteger" | "nonPositiveInteger" => "0",
        "positiveInteger" => "1",
        "negativeInteger" => "-1",
        "decimal" | "float" | "double" => "0.0",
        "dateTime" => "2024-01-01T00:00:00Z",
        "date" => "2024-01-01",
        "time" => "00:00:00",
        "duration" => "P1D",
        "base64Binary" => "AAAA",
        "hexBinary" => "00",
        "anyURI" => "http://example.com",
        _ => "string",
    }
}

/// Renders sample XML for schema elements and types. Namespace prefixes (`ns1`, `ns2`, ...)
/// are handed out in first-use order and exposed so the caller can declare them.
pub(crate) struct Sampler<'a> {
    schemas: &'a SchemaSet,
    prefixes: BTreeMap<String, String>,
    stack: Vec<QName>,
}

impl<'a> Sampler<'a> {
    pub(crate) fn new(schemas: &'a SchemaSet) -> Self {
        Self {
            schemas,
            prefixes: BTreeMap::new(),
            stack: Vec::new(),
        }
    }

    /// Namespace URI to prefix, for every namespace used so far.
    pub(crate) fn namespaces(&self) -> &BTreeMap<String, String> {
        &self.prefixes
    }

    fn prefix_for(&mut self, ns: &str) -> Option<String> {
        if ns.is_empty() {
            return None;
        }
        let next = self.prefixes.len() + 1;
        Some(
            self.prefixes
                .entry(ns.to_string())
                .or_insert_with(|| format!("ns{next}"))
                .clone(),
        )
    }

    fn tag(&mut self, ns: &str, local: &str) -> String {
        match self.prefix_for(ns) {
            Some(p) => format!("{p}:{local}"),
            None => local.to_string(),
        }
    }

    /// Sample for a top-level element declaration. An unknown element renders empty.
    pub(crate) fn element(&mut self, name: &QName) -> String {
        let schemas = self.schemas;
        match schemas.elements.get(name) {
            Some(decl) => self.render_decl(decl, 0),
            None => {
                let tag = self.tag(&name.ns, &name.local);
                format!("<{tag}/>")
            }
        }
    }

    /// Unqualified element of a given type, used for `type=` message parts.
    pub(crate) fn typed_element(&mut self, local: &str, ty: &QName) -> String {
        let content = self.render_named(ty, 0);
        format!("<{local}>{content}</{local}>")
    }

    /// Wrap already rendered content in a (possibly qualified) element.
    pub(crate) fn wrap(&mut self, ns: &str, local: &str, inner: &str) -> String {
        let tag = self.tag(ns, local);
        format!("<{tag}>{inner}</{tag}>")
    }

    fn render_decl(&mut self, d: &'a ElementDecl, depth: usize) -> String {
        let schemas = self.schemas;
        let d = match &d.reference {
            Some(r) => schemas.elements.get(r).unwrap_or(d),
            None => d,
        };
        let tag = self.tag(&d.ns, &d.name);
        if depth >= MAX_DEPTH {
            return format!("<{tag}><!-- depth limit --></{tag}>");
        }
        if let Some(v) = &d.value {
            return format!("<{tag}>{}</{tag}>", esc(v));
        }
        let content = match &d.ty {
            ElementType::Named(q) => self.render_named(q, depth),
            ElementType::Inline(def) => self.render_def(def, depth),
            ElementType::Unspecified => String::new(),
        };
        format!("<{tag}>{content}</{tag}>")
    }

    fn render_named(&mut self, q: &QName, depth: usize) -> String {
        if q.ns == XSD_NS {
            return esc(builtin_sample(&q.local));
        }
        let schemas = self.schemas;
        let Some(def) = schemas.types.get(q) else {
            return esc("?");
        };
        if self.stack.contains(q) {
            return format!("<!-- recursive type: {} -->", comment_safe(&q.local));
        }
        self.stack.push(q.clone());
        let out = self.render_def(def, depth);
        self.stack.pop();
        out
    }

    fn render_def(&mut self, def: &'a TypeDef, depth: usize) -> String {
        match def {
            TypeDef::Simple(s) => self.simple_text(s, 0),
            TypeDef::Complex(c) => {
                let mut out = String::new();
                if let Some(b) = &c.base {
                    if b.ns != XSD_NS {
                        out.push_str(&self.render_named(b, depth));
                    }
                }
                if let Some(sb) = &c.simple_base {
                    out.push_str(&self.simple_base_text(sb, 0));
                }
                for p in &c.particles {
                    out.push_str(&self.render_decl(p, depth + 1));
                }
                out
            }
        }
    }

    fn simple_text(&self, s: &SimpleType, hops: usize) -> String {
        if hops > MAX_DEPTH {
            return esc("string");
        }
        if let Some(e) = &s.first_enum {
            return esc(e);
        }
        match &s.base {
            Some(b) => self.simple_base_text(b, hops + 1),
            None => esc("string"),
        }
    }

    fn simple_base_text(&self, q: &QName, hops: usize) -> String {
        if q.ns == XSD_NS {
            return esc(builtin_sample(&q.local));
        }
        match self.schemas.types.get(q) {
            Some(TypeDef::Simple(s)) => self.simple_text(s, hops),
            _ => esc("string"),
        }
    }
}
```

`crates/rocket-import/src/wsdl/parser.rs` implementation (place above the `#[cfg(test)] mod tests` block written in Step 4):

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use roxmltree::{Document, Node};

use super::ast::*;
use super::schema::SchemaSet;
use crate::error::{ImportError, ImportResult};

const MAX_SOURCES: usize = 32;
const MAX_SOURCE_BYTES: u64 = 5 * 1024 * 1024;

fn parse_err(path: &Path, message: impl Into<String>) -> ImportError {
    ImportError::ParseError {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn read_limited(path: &Path) -> ImportResult<String> {
    let len = std::fs::metadata(path)?.len();
    if len > MAX_SOURCE_BYTES {
        return Err(parse_err(path, "file is larger than 5 MiB"));
    }
    Ok(std::fs::read_to_string(path)?)
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn in_ns(n: &Node, ns: &str, name: &str) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(ns) && n.tag_name().name() == name
}

/// Locations referenced by `wsdl:import`, `xsd:import`, `xsd:include` and `xsd:redefine`.
fn import_locations(path: &Path, text: &str) -> ImportResult<Vec<String>> {
    let doc = Document::parse(text).map_err(|e| parse_err(path, e.to_string()))?;
    let mut out = Vec::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        let ns = n.tag_name().namespace();
        let name = n.tag_name().name();
        if ns == Some(XSD_NS) && matches!(name, "import" | "include" | "redefine") {
            if let Some(l) = n.attribute("schemaLocation") {
                out.push(l.to_string());
            }
        } else if ns == Some(WSDL_NS) && name == "import" {
            if let Some(l) = n.attribute("location") {
                out.push(l.to_string());
            }
        }
    }
    Ok(out)
}

/// Read the root document and every local file it imports. Remote locations are only reported.
fn load_sources(
    origin: &Path,
    root_text: String,
    warnings: &mut Vec<String>,
) -> ImportResult<Vec<(PathBuf, String)>> {
    let mut out = vec![(origin.to_path_buf(), root_text)];
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(canonical(origin));
    let mut i = 0;
    while i < out.len() {
        let (path, text) = (out[i].0.clone(), out[i].1.clone());
        for loc in import_locations(&path, &text)? {
            if loc.contains("://") {
                warnings.push(format!("remote import not fetched: {loc}"));
                continue;
            }
            let target = path.parent().unwrap_or_else(|| Path::new("")).join(&loc);
            if !seen.insert(canonical(&target)) {
                continue;
            }
            if out.len() >= MAX_SOURCES {
                warnings.push(format!("import limit reached, skipped: {loc}"));
                continue;
            }
            match read_limited(&target) {
                Ok(t) => out.push((target, t)),
                Err(e) => warnings.push(format!("could not read import {loc}: {e}")),
            }
        }
        i += 1;
    }
    Ok(out)
}

struct RawPortOp {
    name: String,
    input_name: Option<String>,
    input_message: Option<QName>,
}

struct RawBindingOp {
    name: String,
    input_name: Option<String>,
    soap_action: String,
    style: Option<BindingStyle>,
    rpc_ns: Option<String>,
}

struct RawBinding {
    port_type: QName,
    soap: Option<SoapVersion>,
    style: BindingStyle,
    ops: Vec<RawBindingOp>,
}

struct RawPort {
    name: String,
    binding: QName,
    address: Option<String>,
}

struct RawService {
    name: String,
    ports: Vec<RawPort>,
}

#[derive(Default)]
struct Raw {
    messages: HashMap<QName, Vec<MessagePart>>,
    port_types: HashMap<QName, Vec<RawPortOp>>,
    bindings: HashMap<QName, RawBinding>,
    services: Vec<RawService>,
}

fn soap_version_of(n: &Node) -> Option<SoapVersion> {
    match n.tag_name().namespace() {
        Some(SOAP11_NS) => Some(SoapVersion::V11),
        Some(SOAP12_NS) => Some(SoapVersion::V12),
        _ => None,
    }
}

fn style_of(s: Option<&str>) -> Option<BindingStyle> {
    match s {
        Some("rpc") => Some(BindingStyle::Rpc),
        Some("document") => Some(BindingStyle::Document),
        _ => None,
    }
}

impl Raw {
    fn absorb(&mut self, root: Node, schemas: &mut SchemaSet, path: &Path) -> ImportResult<()> {
        if root.tag_name().namespace() == Some(WSDL2_NS) {
            return Err(parse_err(path, "WSDL 2.0 is not supported, use a WSDL 1.1 document"));
        }
        if in_ns(&root, XSD_NS, "schema") {
            schemas.add_schema(root);
            return Ok(());
        }
        if !in_ns(&root, WSDL_NS, "definitions") {
            return Err(parse_err(path, "not a WSDL 1.1 document (no wsdl:definitions root)"));
        }
        let tns = root.attribute("targetNamespace").unwrap_or("").to_string();
        for child in root.children().filter(|n| n.is_element()) {
            if child.tag_name().namespace() != Some(WSDL_NS) {
                continue;
            }
            match child.tag_name().name() {
                "types" => {
                    for s in child.children().filter(|n| in_ns(n, XSD_NS, "schema")) {
                        schemas.add_schema(s);
                    }
                }
                "message" => self.absorb_message(child, &tns),
                "portType" => self.absorb_port_type(child, &tns),
                "binding" => self.absorb_binding(child, &tns),
                "service" => self.absorb_service(child),
                _ => {}
            }
        }
        Ok(())
    }

    fn absorb_message(&mut self, node: Node, tns: &str) {
        let Some(name) = node.attribute("name") else { return };
        let mut parts = Vec::new();
        for p in node.children().filter(|n| in_ns(n, WSDL_NS, "part")) {
            let Some(pname) = p.attribute("name") else { continue };
            let kind = if let Some(q) = qname_attr(p, "element") {
                PartKind::Element(q)
            } else if let Some(q) = qname_attr(p, "type") {
                PartKind::Type(q)
            } else {
                continue;
            };
            parts.push(MessagePart {
                name: pname.to_string(),
                kind,
            });
        }
        self.messages.insert(QName::new(tns, name), parts);
    }

    fn absorb_port_type(&mut self, node: Node, tns: &str) {
        let Some(name) = node.attribute("name") else { return };
        let mut ops = Vec::new();
        for op in node.children().filter(|n| in_ns(n, WSDL_NS, "operation")) {
            let Some(oname) = op.attribute("name") else { continue };
            let input = op.children().find(|n| in_ns(n, WSDL_NS, "input"));
            ops.push(RawPortOp {
                name: oname.to_string(),
                input_name: input.and_then(|i| i.attribute("name")).map(str::to_string),
                input_message: input.and_then(|i| qname_attr(i, "message")),
            });
        }
        self.port_types.insert(QName::new(tns, name), ops);
    }

    fn absorb_binding(&mut self, node: Node, tns: &str) {
        let (Some(name), Some(port_type)) = (node.attribute("name"), qname_attr(node, "type"))
        else {
            return;
        };
        let soap_el = node
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "binding" && soap_version_of(n).is_some());
        let soap = soap_el.as_ref().and_then(soap_version_of);
        let style = style_of(soap_el.and_then(|e| e.attribute("style"))).unwrap_or(BindingStyle::Document);
        let mut ops = Vec::new();
        for op in node.children().filter(|n| in_ns(n, WSDL_NS, "operation")) {
            let Some(oname) = op.attribute("name") else { continue };
            let soap_op = op
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "operation" && soap_version_of(n).is_some());
            let input = op.children().find(|n| in_ns(n, WSDL_NS, "input"));
            let body = input.and_then(|i| {
                i.children()
                    .find(|n| n.is_element() && n.tag_name().name() == "body" && soap_version_of(n).is_some())
            });
            ops.push(RawBindingOp {
                name: oname.to_string(),
                input_name: input.and_then(|i| i.attribute("name")).map(str::to_string),
                soap_action: soap_op
                    .and_then(|s| s.attribute("soapAction"))
                    .unwrap_or("")
                    .to_string(),
                style: soap_op.and_then(|s| style_of(s.attribute("style"))),
                rpc_ns: body.and_then(|b| b.attribute("namespace")).map(str::to_string),
            });
        }
        self.bindings.insert(
            QName::new(tns, name),
            RawBinding {
                port_type,
                soap,
                style,
                ops,
            },
        );
    }

    fn absorb_service(&mut self, node: Node) {
        let Some(name) = node.attribute("name") else { return };
        let mut ports = Vec::new();
        for p in node.children().filter(|n| in_ns(n, WSDL_NS, "port")) {
            let (Some(pname), Some(binding)) = (p.attribute("name"), qname_attr(p, "binding"))
            else {
                continue;
            };
            let address = p
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "address" && soap_version_of(n).is_some())
                .and_then(|a| a.attribute("location"))
                .map(str::to_string);
            ports.push(RawPort {
                name: pname.to_string(),
                binding,
                address,
            });
        }
        self.services.push(RawService {
            name: name.to_string(),
            ports,
        });
    }
}

fn build_model(raw: Raw, schemas: SchemaSet, mut warnings: Vec<String>) -> WsdlModel {
    let mut services = Vec::new();
    for svc in &raw.services {
        let mut ports = Vec::new();
        for port in &svc.ports {
            let Some(binding) = raw.bindings.get(&port.binding) else {
                warnings.push(format!("port {} references an unknown binding", port.name));
                continue;
            };
            let Some(soap) = binding.soap else {
                warnings.push(format!(
                    "port {} uses a non-SOAP binding ({}), skipped",
                    port.name, port.binding.local
                ));
                continue;
            };
            let Some(port_type) = raw.port_types.get(&binding.port_type) else {
                warnings.push(format!("port {} references an unknown portType", port.name));
                continue;
            };
            let address = match &port.address {
                Some(a) => a.clone(),
                None => {
                    warnings.push(format!(
                        "port {} has no address, using {{{{baseUrl}}}}",
                        port.name
                    ));
                    "{{baseUrl}}".to_string()
                }
            };
            let mut operations = Vec::new();
            for bop in &binding.ops {
                let pt_op = port_type.iter().find(|o| {
                    o.name == bop.name
                        && (bop.input_name.is_none() || o.input_name == bop.input_name)
                });
                let Some(msg_name) = pt_op.and_then(|o| o.input_message.as_ref()) else {
                    warnings.push(format!(
                        "operation {} has no input message, skipped",
                        bop.name
                    ));
                    continue;
                };
                let input_parts = raw.messages.get(msg_name).cloned().unwrap_or_default();
                operations.push(WsdlOperation {
                    name: bop.name.clone(),
                    soap_action: bop.soap_action.clone(),
                    style: bop.style.unwrap_or(binding.style),
                    rpc_namespace: bop.rpc_ns.clone(),
                    input_parts,
                });
            }
            ports.push(WsdlPort {
                name: port.name.clone(),
                address,
                soap,
                operations,
            });
        }
        services.push(WsdlService {
            name: svc.name.clone(),
            ports,
        });
    }
    WsdlModel {
        services,
        schemas,
        warnings,
    }
}

/// Parse a WSDL 1.1 file and the local files it imports.
pub(crate) fn parse_wsdl_file(path: &Path) -> ImportResult<WsdlModel> {
    let text = read_limited(path)?;
    parse_wsdl_str(&text, path)
}

/// Parse WSDL text. `origin` is the file path used to resolve relative imports and name errors.
pub(crate) fn parse_wsdl_str(text: &str, origin: &Path) -> ImportResult<WsdlModel> {
    let mut warnings = Vec::new();
    let sources = load_sources(origin, text.to_string(), &mut warnings)?;
    let mut raw = Raw::default();
    let mut schemas = SchemaSet::default();
    for (path, src) in &sources {
        // The default options reject DTDs, which closes XXE and entity expansion.
        let doc = Document::parse(src).map_err(|e| parse_err(path, e.to_string()))?;
        raw.absorb(doc.root_element(), &mut schemas, path)?;
    }
    Ok(build_model(raw, schemas, warnings))
}
```

- [ ] **Step 7: Run the parser tests to verify they pass**

Run: `cargo test -j4 -p rocket-import wsdl::parser`
Expected: PASS (8 tests). If `unwrap_or(d)` in the sampler complains about lifetimes, keep the `schemas` local copy as written.

- [ ] **Step 8: Write the failing sampler tests**

Append to `crates/rocket-import/src/wsdl/sampler.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::wsdl::parse_wsdl_str;
    use std::path::Path;

    fn model_with_schema(xsd: &str) -> crate::wsdl::WsdlModel {
        let wsdl = format!(
            r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
                 xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:t="urn:t" targetNamespace="urn:t">
               <types><xsd:schema targetNamespace="urn:t" elementFormDefault="qualified">{xsd}</xsd:schema></types>
             </definitions>"#
        );
        parse_wsdl_str(&wsdl, Path::new("inline.wsdl")).expect("parses")
    }

    #[test]
    fn samples_nested_types_with_enum_and_builtins() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/wsdl/calc.wsdl");
        let model = crate::wsdl::parse_wsdl_file(&path).expect("fixture parses");
        let mut s = Sampler::new(&model.schemas);
        let xml = s.element(&QName::new("http://example.com/calc", "Add"));
        assert_eq!(
            xml,
            "<ns1:Add><ns1:operands><ns2:a>0</ns2:a><ns2:b>0</ns2:b><ns2:mode>fast</ns2:mode></ns1:operands></ns1:Add>"
        );
        assert_eq!(s.namespaces().get("http://example.com/calc").map(String::as_str), Some("ns1"));
        assert_eq!(s.namespaces().get("http://example.com/types").map(String::as_str), Some("ns2"));
    }

    #[test]
    fn recursive_type_terminates_with_marker() {
        let model = model_with_schema(
            r#"<xsd:complexType name="Node"><xsd:sequence>
                 <xsd:element name="child" type="t:Node"/>
                 <xsd:element name="label" type="xsd:string"/>
               </xsd:sequence></xsd:complexType>
               <xsd:element name="Root" type="t:Node"/>"#,
        );
        let mut s = Sampler::new(&model.schemas);
        let xml = s.element(&QName::new("urn:t", "Root"));
        assert!(xml.contains("<!-- recursive type: Node -->"), "got: {xml}");
        assert!(xml.contains("<ns1:label>string</ns1:label>"), "got: {xml}");
    }

    #[test]
    fn deep_nesting_hits_depth_limit() {
        // Twelve distinct anonymous levels, no type cycle, so only the depth guard can stop it.
        let mut xsd = String::from(r#"<xsd:element name="Root">"#);
        for i in 0..12 {
            xsd.push_str(&format!(
                r#"<xsd:complexType><xsd:sequence><xsd:element name="L{i}">"#
            ));
        }
        xsd.push_str(r#"<xsd:complexType><xsd:sequence/></xsd:complexType>"#);
        for _ in 0..12 {
            xsd.push_str("</xsd:element></xsd:sequence></xsd:complexType>");
        }
        xsd.push_str("</xsd:element>");
        let model = model_with_schema(&xsd);
        let mut s = Sampler::new(&model.schemas);
        let xml = s.element(&QName::new("urn:t", "Root"));
        assert!(xml.contains("<!-- depth limit -->"), "got: {xml}");
        assert!(!xml.contains("L11"), "must stop before the deepest level: {xml}");
    }

    #[test]
    fn sample_values_are_escaped() {
        let model = model_with_schema(
            r#"<xsd:element name="x" type="xsd:string" default="a&lt;b&amp;c"/>"#,
        );
        let mut s = Sampler::new(&model.schemas);
        let xml = s.element(&QName::new("urn:t", "x"));
        assert_eq!(xml, "<ns1:x>a&lt;b&amp;c</ns1:x>");
    }

    #[test]
    fn choice_takes_first_alternative_and_ref_is_followed() {
        let model = model_with_schema(
            r#"<xsd:element name="Id" type="xsd:long"/>
               <xsd:element name="Pick"><xsd:complexType><xsd:choice>
                 <xsd:element ref="t:Id"/>
                 <xsd:element name="other" type="xsd:string"/>
               </xsd:choice></xsd:complexType></xsd:element>"#,
        );
        let mut s = Sampler::new(&model.schemas);
        let xml = s.element(&QName::new("urn:t", "Pick"));
        assert_eq!(xml, "<ns1:Pick><ns1:Id>0</ns1:Id></ns1:Pick>");
    }

    #[test]
    fn comment_safe_strips_double_dash() {
        assert_eq!(comment_safe("a--b"), "a-b");
    }
}
```

- [ ] **Step 9: Run the sampler tests**

Run: `cargo test -j4 -p rocket-import wsdl::sampler`
Expected: PASS (the implementation from Step 6 already exists, so this step confirms the tests are meaningful). If one fails, fix the implementation, not the expectation, unless the expectation is demonstrably wrong against the fixture. To confirm each test can fail, temporarily change `MAX_DEPTH` to `64` and see `deep_nesting_hits_depth_limit` fail, then restore it.

- [ ] **Step 10: Verify and commit**

Run: `cargo check -j4 -p rocket-import` and `cargo test -j4 -p rocket-import wsdl`
Expected: both succeed, no warnings from the new module (`dead_code` warnings on `label`, `esc_attr` are fine until Task 2; if clippy gates the repo, add `#[allow(dead_code)]` with a one-line comment "Used by the converter." and remove it in Task 2).

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage by path:
`git add crates/rocket-import/Cargo.toml Cargo.lock crates/rocket-import/src/lib.rs crates/rocket-import/src/wsdl crates/rocket-import/tests/fixtures/wsdl`
Suggested message subject: `feat(import): parse WSDL 1.1 and sample XSD types`.

---

## Task 2: Converter to domain types and `ImportService::import_wsdl`

**Files:**
- Create: `crates/rocket-import/src/converter/wsdl.rs`
- Modify: `crates/rocket-import/src/converter/mod.rs`, `crates/rocket-import/src/importer.rs`

**Interfaces:**
- Consumes: `wsdl::{parse_wsdl_file, WsdlModel, WsdlPort, WsdlOperation, SoapVersion, BindingStyle, PartKind, Sampler, esc_attr}` (Task 1); `rocket_collection::Request`; `rocket_shared::types::{Body, BodyMode, Header, HttpMethod}`; `rocket_shared::description::Description`.
- Produces:
  - `converter::wsdl::build_envelope(version: SoapVersion, op: &WsdlOperation, schemas: &SchemaSet) -> String`
  - `converter::wsdl::soap_headers(version: SoapVersion, action: &str) -> Vec<Header>`
  - `converter::wsdl::convert_operation(port: &WsdlPort, op: &WsdlOperation, schemas: &SchemaSet, seq: u32) -> Request`
  - `ImportService::import_wsdl(&self, path: &Path, workspace_id: &str) -> ImportResult<ImportReport>`

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Register the module and write the failing converter tests**

In `crates/rocket-import/src/converter/mod.rs` add `pub(crate) mod wsdl;` (keep alphabetical: after `request`).

Create `crates/rocket-import/src/converter/wsdl.rs` with only this test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::wsdl::{parse_wsdl_file, parse_wsdl_str, BindingStyle, SoapVersion};
    use rocket_shared::types::{BodyMode, HttpMethod};
    use std::path::{Path, PathBuf};

    fn calc() -> crate::wsdl::WsdlModel {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wsdl/calc.wsdl");
        parse_wsdl_file(&path).expect("fixture parses")
    }

    fn header<'a>(req: &'a rocket_collection::Request, key: &str) -> Option<&'a str> {
        req.headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case(key))
            .map(|h| h.value.as_str())
    }

    #[test]
    fn soap11_headers() {
        let h = soap_headers(SoapVersion::V11, "http://example.com/calc/Add");
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].key, "Content-Type");
        assert_eq!(h[0].value, "text/xml; charset=utf-8");
        assert_eq!(h[1].key, "SOAPAction");
        assert_eq!(h[1].value, "\"http://example.com/calc/Add\"");
    }

    #[test]
    fn soap11_empty_action_is_still_a_quoted_empty_string() {
        let h = soap_headers(SoapVersion::V11, "");
        assert_eq!(h[1].value, "\"\"");
    }

    #[test]
    fn soap12_headers_have_no_soapaction_header() {
        let h = soap_headers(SoapVersion::V12, "urn:do");
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].key, "Content-Type");
        assert_eq!(h[0].value, "application/soap+xml; charset=utf-8; action=\"urn:do\"");
        let none = soap_headers(SoapVersion::V12, "");
        assert_eq!(none[0].value, "application/soap+xml; charset=utf-8");
    }

    #[test]
    fn action_with_quote_cannot_break_out_of_the_header() {
        let h = soap_headers(SoapVersion::V12, "a\"b");
        assert!(!h[0].value.contains("a\"b"), "got: {}", h[0].value);
    }

    #[test]
    fn envelope_namespace_follows_version() {
        let model = calc();
        let op = &model.services[0].ports[0].operations[0];
        let e11 = build_envelope(SoapVersion::V11, op, &model.schemas);
        let e12 = build_envelope(SoapVersion::V12, op, &model.schemas);
        assert!(e11.contains(r#"xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/""#));
        assert!(e12.contains(r#"xmlns:soapenv="http://www.w3.org/2003/05/soap-envelope""#));
        assert!(e11.contains(r#"xmlns:ns1="http://example.com/calc""#));
        assert!(e11.contains(r#"xmlns:ns2="http://example.com/types""#));
        assert!(e11.contains("<ns1:Add><ns1:operands><ns2:a>0</ns2:a>"));
        assert!(e11.contains("<soapenv:Body>"));
    }

    #[test]
    fn envelope_is_well_formed_xml() {
        let model = calc();
        let op = &model.services[0].ports[0].operations[0];
        let env = build_envelope(SoapVersion::V11, op, &model.schemas);
        roxmltree::Document::parse(&env).expect("envelope must be well-formed");
    }

    #[test]
    fn rpc_envelope_wraps_parts_in_operation_element() {
        let wsdl = r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
            xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
            xmlns:xsd="http://www.w3.org/2001/XMLSchema"
            xmlns:tns="urn:echo" targetNamespace="urn:echo">
          <message name="EchoIn"><part name="msg" type="xsd:string"/></message>
          <portType name="P"><operation name="Echo"><input message="tns:EchoIn"/></operation></portType>
          <binding name="B" type="tns:P"><soap:binding style="rpc"/>
            <operation name="Echo"><soap:operation soapAction="urn:echo"/>
              <input><soap:body use="literal" namespace="urn:echo-rpc"/></input></operation></binding>
          <service name="S"><port name="Pt" binding="tns:B"><soap:address location="http://h/e"/></port></service>
        </definitions>"#;
        let model = parse_wsdl_str(wsdl, Path::new("rpc.wsdl")).expect("parses");
        let op = &model.services[0].ports[0].operations[0];
        assert_eq!(op.style, BindingStyle::Rpc);
        let env = build_envelope(SoapVersion::V11, op, &model.schemas);
        assert!(env.contains(r#"xmlns:ns1="urn:echo-rpc""#), "got: {env}");
        assert!(
            env.contains("<ns1:Echo><msg>string</msg></ns1:Echo>"),
            "got: {env}"
        );
    }

    #[test]
    fn convert_operation_builds_a_post_with_xml_body_and_soap_headers() {
        let model = calc();
        let port = &model.services[0].ports[0];
        let req = convert_operation(port, &port.operations[0], &model.schemas, 1);
        assert_eq!(req.name, "Add");
        assert_eq!(req.method, HttpMethod::Post);
        assert_eq!(req.url, "http://localhost:8080/calc");
        assert_eq!(req.seq, Some(1));
        let body = req.body.as_ref().expect("body");
        assert_eq!(body.mode, BodyMode::Xml);
        assert!(body.content.as_deref().unwrap_or("").contains("<soapenv:Envelope"));
        assert_eq!(header(&req, "Content-Type"), Some("text/xml; charset=utf-8"));
        assert_eq!(header(&req, "SOAPAction"), Some("\"http://example.com/calc/Add\""));
    }

    #[test]
    fn soap12_port_uses_soap_plus_xml() {
        let model = calc();
        let port = &model.services[0].ports[1];
        let req = convert_operation(port, &port.operations[0], &model.schemas, 1);
        assert_eq!(req.url, "http://localhost:8080/calc12");
        assert_eq!(
            header(&req, "Content-Type"),
            Some("application/soap+xml; charset=utf-8; action=\"http://example.com/calc/Add\"")
        );
        assert_eq!(header(&req, "SOAPAction"), None);
    }
}
```

If `HttpMethod` or `BodyMode` lacks `PartialEq`/`Debug` the `assert_eq!` will not compile. Both derive them in `crates/rocket-shared/src/types.rs` today (`BodyMode` is compared in existing converter tests); if not, compare with `matches!`.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-import converter::wsdl`
Expected: FAIL to compile (`soap_headers`, `build_envelope`, `convert_operation` not found).

- [ ] **Step 4: Implement the converter**

Add above the test module in `crates/rocket-import/src/converter/wsdl.rs`:

```rust
use rocket_collection::Request;
use rocket_shared::description::Description;
use rocket_shared::types::{Body, BodyMode, Header, HttpMethod};

use crate::wsdl::{
    esc_attr, BindingStyle, PartKind, Sampler, SchemaSet, SoapVersion, WsdlOperation, WsdlPort,
};

/// Headers for the binding's SOAP version.
///
/// SOAP 1.1 carries the action in a quoted `SOAPAction` header. SOAP 1.2 carries it as the
/// `action` parameter of the content type and has no `SOAPAction` header.
pub(crate) fn soap_headers(version: SoapVersion, action: &str) -> Vec<Header> {
    // A quote cannot appear in a valid action URI, so encode it rather than let it end the value.
    let action = action.replace('"', "%22");
    match version {
        SoapVersion::V11 => vec![
            Header::new("Content-Type", "text/xml; charset=utf-8"),
            Header::new("SOAPAction", format!("\"{action}\"")),
        ],
        SoapVersion::V12 => {
            let content_type = if action.is_empty() {
                "application/soap+xml; charset=utf-8".to_string()
            } else {
                format!("application/soap+xml; charset=utf-8; action=\"{action}\"")
            };
            vec![Header::new("Content-Type", content_type)]
        }
    }
}

/// Build a sample SOAP envelope for one operation.
pub(crate) fn build_envelope(
    version: SoapVersion,
    op: &WsdlOperation,
    schemas: &SchemaSet,
) -> String {
    let mut sampler = Sampler::new(schemas);
    let mut pieces: Vec<String> = Vec::new();
    for part in &op.input_parts {
        pieces.push(match &part.kind {
            PartKind::Element(q) => sampler.element(q),
            PartKind::Type(q) => sampler.typed_element(&part.name, q),
        });
    }
    let body = match op.style {
        BindingStyle::Document => pieces.join("\n    "),
        BindingStyle::Rpc => {
            let inner = pieces.join("");
            sampler.wrap(op.rpc_namespace.as_deref().unwrap_or(""), &op.name, &inner)
        }
    };
    let mut decls = format!(r#"xmlns:soapenv="{}""#, version.envelope_ns());
    for (ns, prefix) in sampler.namespaces() {
        decls.push_str(&format!(r#" xmlns:{prefix}="{}""#, esc_attr(ns)));
    }
    format!(
        "<soapenv:Envelope {decls}>\n  <soapenv:Header/>\n  <soapenv:Body>\n    {body}\n  </soapenv:Body>\n</soapenv:Envelope>"
    )
}

/// Convert one SOAP operation into a Rocket request.
pub(crate) fn convert_operation(
    port: &WsdlPort,
    op: &WsdlOperation,
    schemas: &SchemaSet,
    seq: u32,
) -> Request {
    let mut req = Request::new(op.name.clone(), HttpMethod::Post, port.address.clone());
    req.headers = soap_headers(port.soap, &op.soap_action);
    req.body = Some(Body {
        mode: BodyMode::Xml,
        content: Some(build_envelope(port.soap, op, schemas)),
        form_data: None,
        file_path: None,
    });
    req.seq = Some(seq);
    let style = match op.style {
        BindingStyle::Document => "document/literal",
        BindingStyle::Rpc => "rpc/literal",
    };
    req.description = Some(Description::text(format!(
        "{} operation {} ({style}). Imported from WSDL.",
        port.soap.label(),
        op.name
    )));
    req
}
```

- [ ] **Step 5: Run the converter tests to verify they pass**

Run: `cargo test -j4 -p rocket-import converter::wsdl`
Expected: PASS (9 tests). Remove any `#[allow(dead_code)]` added in Task 1.

- [ ] **Step 6: Write the failing service tests**

In `crates/rocket-import/src/importer.rs`, append a new test module at the end of the file:

```rust
#[cfg(test)]
mod wsdl_tests {
    use super::*;
    use tempfile::TempDir;

    fn calc_wsdl() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wsdl/calc.wsdl")
    }

    fn yml_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let p = entry.expect("entry").path();
            if p.is_dir() {
                yml_files(&p, out);
            } else if p.extension().and_then(|e| e.to_str()) == Some("yml") {
                out.push(p);
            }
        }
    }

    #[test]
    fn import_wsdl_writes_service_port_operation_tree() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let report = service
            .import_wsdl(&calc_wsdl(), "default")
            .expect("import should succeed");

        assert_eq!(report.detected_type, "collection");
        assert_eq!(report.created_collections, vec!["calc".to_string()]);
        assert_eq!(report.total_files, 4);
        assert_eq!(report.imported, 4);
        assert!(report.skipped.is_empty(), "got: {:?}", report.skipped);

        let col = ws.path().join("collections/calc");
        assert!(col.join("Calculator/CalcSoap").is_dir());
        assert!(col.join("Calculator/CalcSoap12").is_dir());
        let mut files = Vec::new();
        yml_files(&col.join("Calculator"), &mut files);
        assert_eq!(files.len(), 4, "got: {files:?}");

        let soap12 = std::fs::read_to_string(
            files
                .iter()
                .find(|p| p.to_string_lossy().contains("CalcSoap12"))
                .expect("a soap 1.2 request file"),
        )
        .expect("read");
        assert!(soap12.contains("application/soap+xml"), "got: {soap12}");
        assert!(!soap12.to_lowercase().contains("soapaction"), "got: {soap12}");
    }

    #[test]
    fn import_wsdl_resolves_collection_name_conflicts() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        service.import_wsdl(&calc_wsdl(), "default").expect("first");
        let second = service.import_wsdl(&calc_wsdl(), "default").expect("second");
        assert_eq!(second.created_collections, vec!["calc-1".to_string()]);
    }

    #[test]
    fn warnings_become_report_items() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let wsdl = std::fs::read_to_string(calc_wsdl())
            .expect("read fixture")
            .replace(
                r#"<xsd:import namespace="http://example.com/types" schemaLocation="calc-types.xsd"/>"#,
                r#"<xsd:import namespace="http://example.com/types" schemaLocation="https://example.invalid/t.xsd"/>"#,
            );
        let path = src.path().join("remote.wsdl");
        std::fs::write(&path, wsdl).expect("write");
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl(&path, "default")
            .expect("import should succeed");
        assert!(
            report.skipped.iter().any(|s| matches!(
                &s.reason,
                SkipReason::UnsupportedRequestType(m) if m.contains("not fetched")
            )),
            "got: {:?}",
            report.skipped
        );
    }

    #[test]
    fn duplicate_operation_names_get_suffixes() {
        let ws = TempDir::new().expect("tempdir");
        let src = TempDir::new().expect("tempdir");
        let wsdl = r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
            xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
            xmlns:xsd="http://www.w3.org/2001/XMLSchema"
            xmlns:tns="urn:o" targetNamespace="urn:o">
          <message name="A"><part name="p" type="xsd:string"/></message>
          <message name="B"><part name="p" type="xsd:int"/></message>
          <portType name="P">
            <operation name="Get"><input name="ByName" message="tns:A"/></operation>
            <operation name="Get"><input name="ById" message="tns:B"/></operation>
          </portType>
          <binding name="Bd" type="tns:P"><soap:binding style="document"/>
            <operation name="Get"><soap:operation soapAction="a"/><input name="ByName"><soap:body use="literal"/></input></operation>
            <operation name="Get"><soap:operation soapAction="b"/><input name="ById"><soap:body use="literal"/></input></operation>
          </binding>
          <service name="S"><port name="Pt" binding="tns:Bd"><soap:address location="http://h/o"/></port></service>
        </definitions>"#;
        let path = src.path().join("overload.wsdl");
        std::fs::write(&path, wsdl).expect("write");
        let report = ImportService::new_with_workspace_path(ws.path())
            .import_wsdl(&path, "default")
            .expect("import should succeed");
        assert_eq!(report.imported, 2);
        let mut files = Vec::new();
        yml_files(&ws.path().join("collections/overload"), &mut files);
        assert_eq!(files.len(), 2, "overloads must not overwrite each other: {files:?}");
    }
}
```

- [ ] **Step 7: Run to verify failure**

Run: `cargo test -j4 -p rocket-import wsdl_tests`
Expected: FAIL to compile (`import_wsdl` not found).

- [ ] **Step 8: Implement `import_wsdl`**

In `crates/rocket-import/src/importer.rs`, inside `impl ImportService`, add after `import_postman_environment` (find it with `grep -n "pub fn import_postman_environment"`):

```rust
    /// Import a WSDL 1.1 file as a collection: one folder per service, one folder per
    /// port, one request per SOAP operation. Non-SOAP bindings and unreadable imports
    /// are reported, not fatal.
    pub fn import_wsdl(&self, path: &Path, _workspace_id: &str) -> ImportResult<ImportReport> {
        use crate::converter::wsdl as wc;

        let model = crate::wsdl::parse_wsdl_file(path)?;
        let file_label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "wsdl".to_string());
        let mut report = ImportReport {
            detected_type: "collection".to_string(),
            ..Default::default()
        };
        for warning in &model.warnings {
            report.skipped.push(SkippedItem {
                path: file_label.clone(),
                reason: SkipReason::UnsupportedRequestType(warning.clone()),
            });
        }

        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "wsdl".to_string());
        let col_name = self.resolve_collection_name(&stem)?;
        self.collection_repo
            .create(&col_name)
            .map_err(ImportError::DomainError)?;
        report.created_collections.push(col_name.clone());

        for service in &model.services {
            let service_dir = wsdl_path_segment(&service.name);
            self.collection_repo
                .create_folder(&col_name, &service_dir)
                .map_err(ImportError::DomainError)?;
            for port in &service.ports {
                let port_dir = format!("{service_dir}/{}", wsdl_path_segment(&port.name));
                self.collection_repo
                    .create_folder(&col_name, &port_dir)
                    .map_err(ImportError::DomainError)?;
                let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
                for (i, op) in port.operations.iter().enumerate() {
                    report.total_files += 1;
                    let mut req = wc::convert_operation(port, op, &model.schemas, (i + 1) as u32);
                    let base = wsdl_path_segment(&op.name);
                    let mut slug = base.clone();
                    let mut n = 2u32;
                    while !used.insert(slug.clone()) {
                        slug = format!("{base}-{n}");
                        n += 1;
                    }
                    if slug != base {
                        req.name = format!("{} ({n_minus})", op.name, n_minus = n - 1);
                    }
                    self.collection_repo
                        .save_request(&col_name, &format!("{port_dir}/{slug}"), &req)
                        .map_err(ImportError::DomainError)?;
                    report.imported += 1;
                }
            }
        }
        Ok(report)
    }
```

Add the helper next to `sanitize_postman_filename` (top level of the file, after it):

```rust
/// Folder and file name segment for a WSDL name. Never empty.
fn wsdl_path_segment(name: &str) -> String {
    let s = sanitize_postman_filename(name);
    if s.is_empty() {
        "unnamed".to_string()
    } else {
        s
    }
}
```

- [ ] **Step 9: Run to verify they pass**

Run: `cargo test -j4 -p rocket-import wsdl_tests` then `cargo test -j4 -p rocket-import converter::wsdl`
Expected: PASS. If `import_wsdl_writes_service_port_operation_tree` fails on the directory layout, look at how `FsCollectionRepo::create_folder` treats nested paths (the Postman importer calls it with `a/b`, so it must work) and adjust the test to the real layout. Do not change the layout of the importer.

- [ ] **Step 10: Verify and commit**

Run: `cargo check -j4 -p rocket-import` and `cargo test -j4 -p rocket-import wsdl`
Expected: success.

Commit with the `dev-workflow-skills:1-git-commit` skill:
`git add crates/rocket-import/src/converter crates/rocket-import/src/importer.rs`
Suggested subject: `feat(import): convert WSDL operations to SOAP requests`.

---

## Task 3: IPC command, import dialog option and tests

**Files:**
- Modify: `src-tauri/src/commands/import.rs`, `src-tauri/src/lib.rs`, `src/lib/tauri-api.ts`, `src/components/import/ImportCollectionDialog.tsx`, `crates/rocket-import/CLAUDE.md`
- Create: `src/components/import/importSources.ts`, `src/components/import/ImportCollectionDialog.test.tsx`, `crates/rocket-import/tests/wsdl_integration_test.rs`

**Interfaces:**
- Consumes: `ImportService::import_wsdl(&self, &Path, &str) -> ImportResult<ImportReport>` (Task 2); `make_import_service(base)` in `src-tauri/src/commands/import.rs`.
- Produces: Tauri command `import_wsdl(path: String, target_workspace_id: String) -> Result<ImportReport, String>`; `importWsdl(path: string, targetWorkspaceId: string): Promise<ImportReport>`; `ImportSource = 'bruno' | 'postman' | 'wsdl'`; `SourceKind` gains `'wsdl-file'`; `describeSource(kind)` in `importSources.ts`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** (The command deals with collections.)

- [ ] **Step 2: Write the failing end-to-end Rust test**

Create `crates/rocket-import/tests/wsdl_integration_test.rs`:

```rust
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
```

Run: `cargo test -j4 -p rocket-import --test wsdl_integration_test`
Expected: PASS (all three exercise Task 2 code). It runs here so the public-API path is pinned before the command is added. If it fails, fix Task 2, not the test.

- [ ] **Step 3: Add the Tauri command**

In `src-tauri/src/commands/import.rs`, after `import_postman_environment`, add:

```rust
/// Import a WSDL 1.1 file as a collection of SOAP requests.
#[tauri::command]
pub async fn import_wsdl(
    path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_wsdl(&PathBuf::from(&path), &target_workspace_id)
        .map_err(|e| e.to_string())
}
```

In `src-tauri/src/lib.rs`, in the `invoke_handler` list directly after the line `commands::import::import_postman_environment,` add `commands::import::import_wsdl,`.

Run: `cargo check -j4`
Expected: success.

- [ ] **Step 4: Add the frontend API binding**

In `src/lib/tauri-api.ts`, after `importPostmanEnvironment` (end of the "Collection import" section), add:

```ts
export const importWsdl = (path: string, targetWorkspaceId: string) =>
  invoke<ImportReport>('import_wsdl', { path, targetWorkspaceId });
```

- [ ] **Step 5: Write the failing dialog tests**

Create `src/components/import/ImportCollectionDialog.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ImportCollectionDialog } from './ImportCollectionDialog';
import { describeSource } from './importSources';

const mockOpen = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args: unknown[]) => mockOpen(...args),
}));

const mockImportWsdl = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  createWorkspace: vi.fn(),
  switchWorkspace: vi.fn(),
  getAppDataDir: vi.fn(),
  importBruno: vi.fn(),
  importBrunoZip: vi.fn(),
  importPostmanCollection: vi.fn(),
  importPostmanEnvironment: vi.fn(),
  importWsdl: (...args: unknown[]) => mockImportWsdl(...args),
}));

const report = {
  totalFiles: 4,
  imported: 4,
  skipped: [],
  createdWorkspace: null,
  createdCollections: ['calc'],
  detectedType: 'collection' as const,
};

function renderDialog() {
  return render(<ImportCollectionDialog open onOpenChange={vi.fn()} workspaceId='ws1' />);
}

beforeEach(() => {
  mockOpen.mockReset();
  mockImportWsdl.mockReset();
});

describe('describeSource', () => {
  it('labels a WSDL file', () => {
    expect(describeSource('wsdl-file')).toBe('WSDL file');
  });
});

describe('ImportCollectionDialog WSDL source', () => {
  it('offers a WSDL option next to Bruno and Postman', () => {
    renderDialog();
    expect(screen.getByRole('button', { name: 'Bruno' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Postman' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'WSDL' })).toBeInTheDocument();
  });

  it('explains what WSDL import does', async () => {
    renderDialog();
    await userEvent.click(screen.getByRole('button', { name: 'WSDL' }));
    expect(screen.getByText(/one request per SOAP operation/i)).toBeInTheDocument();
  });

  it('keeps Import disabled until a file is chosen, then imports it', async () => {
    mockOpen.mockResolvedValue('/tmp/calc.wsdl');
    mockImportWsdl.mockResolvedValue(report);
    renderDialog();
    await userEvent.click(screen.getByRole('button', { name: 'WSDL' }));
    expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();

    await userEvent.click(screen.getByRole('button', { name: /choose WSDL file/i }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Import' })).toBeEnabled());
    expect(mockOpen).toHaveBeenCalledWith(
      expect.objectContaining({
        directory: false,
        filters: [{ name: 'WSDL', extensions: ['wsdl', 'xml'] }],
      }),
    );

    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    await waitFor(() => expect(mockImportWsdl).toHaveBeenCalledWith('/tmp/calc.wsdl', 'ws1'));
    expect(await screen.findByText(/4 of 4 requests imported/i)).toBeInTheDocument();
  });

  it('shows the backend error and stays on the picker', async () => {
    mockOpen.mockResolvedValue('/tmp/bad.wsdl');
    mockImportWsdl.mockRejectedValue(new Error('not a WSDL 1.1 document'));
    renderDialog();
    await userEvent.click(screen.getByRole('button', { name: 'WSDL' }));
    await userEvent.click(screen.getByRole('button', { name: /choose WSDL file/i }));
    await userEvent.click(await screen.findByRole('button', { name: 'Import' }));
    expect(await screen.findByText(/not a WSDL 1.1 document/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Import' })).toBeInTheDocument();
  });
});
```

Run: `yarn test src/components/import --run`
Expected: FAIL (`importSources` does not exist, no WSDL button).

- [ ] **Step 6: Implement the helper and the dialog change**

Create `src/components/import/importSources.ts`:

```ts
export type SourceKind = 'folder' | 'zip' | 'postman-json' | 'wsdl-file';

/** Short human label for the selected source, shown under its name. */
export function describeSource(kind: SourceKind): string {
  switch (kind) {
    case 'zip':
      return 'ZIP archive';
    case 'postman-json':
      return 'Postman Collection JSON';
    case 'wsdl-file':
      return 'WSDL file';
    default:
      return 'Folder';
  }
}
```

Edit `src/components/import/ImportCollectionDialog.tsx`:

1. Imports: add `FileCode` to the `lucide-react` import list, add `importWsdl` to the `@/lib/tauri-api` import list (keep alphabetical order Biome expects: after `importPostmanEnvironment`), and add `import { describeSource, type SourceKind } from './importSources';` after the `@/lib/tauri-api` import block.
2. Delete the local `type SourceKind = ...` line and change `type ImportSource = 'bruno' | 'postman';` to `type ImportSource = 'bruno' | 'postman' | 'wsdl';`.
3. Add after `handleChoosePostmanJson`:

```tsx
  async function handleChooseWsdl() {
    const path = await openFilePicker({
      directory: false,
      multiple: false,
      filters: [{ name: 'WSDL', extensions: ['wsdl', 'xml'] }],
    });
    if (typeof path === 'string') {
      const name = path.split('/').pop() ?? path;
      setSource({ path, kind: 'wsdl-file', name });
      setError(null);
    }
  }
```

4. In `handleImport`: change `source.name.replace(/\.zip$/i, '')` to `source.name.replace(/\.(zip|wsdl|xml)$/i, '')`, and add a branch before the `postman` branch result assignment:

```tsx
      if (importSource === 'wsdl') {
        result = await importWsdl(source.path, targetWsId);
      } else if (importSource === 'postman') {
```

(the existing `else if (source.kind === 'zip')` and final `else` stay as they are).

5. `DialogDescription`: replace the ternary with three cases, the new one being `'Select a WSDL 1.1 file. Each SOAP operation becomes a request with a ready-made envelope, one request per SOAP operation.'` The sentence must contain the text "one request per SOAP operation" because the test matches it. Use a small lookup, not a nested ternary:

```tsx
const IMPORT_DESCRIPTIONS: Record<ImportSource, string> = {
  bruno:
    'Select a Bruno collection folder or ZIP archive. Collection or workspace is detected automatically.',
  postman: 'Select a Postman Collection JSON file (v2.0 or v2.1).',
  wsdl: 'Select a WSDL 1.1 file. You get one request per SOAP operation, with the envelope and SOAP headers filled in.',
};
```

placed above the component, and `{IMPORT_DESCRIPTIONS[importSource]}` in the header.

6. Source switcher: add a third shadcn `Button` after the Postman one:

```tsx
                <Button
                  variant={importSource === 'wsdl' ? 'secondary' : 'ghost'}
                  size='sm'
                  className='h-7 px-3 text-xs'
                  onClick={() => switchImportSource('wsdl')}
                >
                  WSDL
                </Button>
```

7. Drop zone: the selected-source block currently branches on `source.kind` for the icon and label. Replace the label ternary with `{describeSource(source.kind)}`. In the empty state add a WSDL case: the headline `Choose a WSDL file`, the hint `SOAP 1.1 or 1.2, local XSD imports are followed`, and the `FileCode` icon (`<FileCode className='h-4 w-4 text-muted-foreground' />`) instead of `Upload` when `importSource === 'wsdl'`. The footer link for WSDL must be a shadcn `Button` (the existing raw `<button>` links in the file are legacy and are not extended):

```tsx
                  {importSource === 'wsdl' && (
                    <Button
                      variant='link'
                      size='sm'
                      className='h-auto p-0 text-xs'
                      onClick={() => void handleChooseWsdl()}
                    >
                      {source ? 'change WSDL file' : 'choose WSDL file'}
                    </Button>
                  )}
```

and restructure the existing `importSource === 'bruno' ? (...) : (...)` footer so the Postman link renders only for `importSource === 'postman'`. The accessible name `choose WSDL file` is what the test clicks.

8. The Postman-only "Additional Environment JSON" block already checks `importSource === 'postman'`, so WSDL needs nothing there.

Run: `yarn test src/components/import --run`
Expected: PASS (5 tests).

- [ ] **Step 7: Update the crate documentation**

In `crates/rocket-import/CLAUDE.md`:
- Change the first paragraph's scope to mention WSDL: after "Bruno API client collections" add ", Postman collections and WSDL 1.1 files".
- Add to the Module Map table: rows for `wsdl/{ast,schema,sampler,parser}.rs` ("WSDL 1.1 reader, owned XSD model, sample XML generation") and `converter/wsdl.rs` ("`convert_operation`, `build_envelope`, `soap_headers`").
- Add a section "### WSDL import" stating: SOAP is an HTTP POST with `BodyMode::Xml`; headers are set per binding version (1.1: `text/xml` + quoted `SOAPAction`; 1.2: `application/soap+xml; action="..."`, no `SOAPAction`); layout is `<service>/<port>/<operation>`; only local imports are followed, remote locations produce `UnsupportedRequestType` report items; DTDs are rejected; XSD attributes, wildcards and `group` are ignored; `choice` uses its first alternative; depth limit is `sampler::MAX_DEPTH`; rpc/encoded is generated as literal; WSDL 2.0 is rejected.
- Add to Tauri Commands: `import_wsdl(path: String, target_workspace_id: String) -> Result<ImportReport, String>`.
- Add `roxmltree` to Dependencies.

- [ ] **Step 8: Full verification**

Run each and expect success:
- `cargo check -j4`
- `cargo test -j4 -p rocket-import wsdl`
- `cargo test -j4 -p rocket-import --test wsdl_integration_test`
- `yarn tsc --noEmit`
- `yarn check`
- `yarn test src/components/import --run`

Do not run `cargo test --workspace`. Optionally smoke test in the real app with `yarn tauri dev`: File, Import Collection, WSDL, choose `crates/rocket-import/tests/fixtures/wsdl/calc.wsdl`, send an `Add` request at a local SOAP mock and confirm the `Content-Type` and `SOAPAction` headers in the Console panel.

- [ ] **Step 9: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill, staging by path. Two commits are cleaner:
1. `git add src-tauri/src/commands/import.rs src-tauri/src/lib.rs crates/rocket-import/tests/wsdl_integration_test.rs crates/rocket-import/CLAUDE.md`, subject `feat(import): add import_wsdl command`.
2. `git add src/lib/tauri-api.ts src/components/import`, subject `feat(import): add WSDL source to the import dialog`.

---

## Known limits (state them in the PR description)

- WSDL 1.1 only. WSDL 2.0 is rejected with a clear error.
- Local files only. A WSDL fetched by URL, or one whose imports are URLs, imports what is local and reports the rest.
- XSD attributes, `xsd:any`, `xsd:group` and substitution groups are not sampled. `choice` samples its first alternative. `maxOccurs` is ignored (each element appears once).
- SOAP headers (`soap:header`), faults and WS-Security are not generated. The user adds auth through Rocket's existing auth tab or scripts.
- rpc/encoded bindings get a literal sample.
- One-way and solicit-response operations without an input message are skipped with a report item.

## Next Plan

Next: [2026-10-05-protocol-parity-plan-05-graphql-model-and-import.md](2026-10-05-protocol-parity-plan-05-graphql-model-and-import.md). Chain to it automatically when this plan finishes, one plan at a time.
