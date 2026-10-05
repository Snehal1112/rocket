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
        let soap_el = node.children().find(|n| {
            n.is_element() && n.tag_name().name() == "binding" && soap_version_of(n).is_some()
        });
        let soap = soap_el.as_ref().and_then(soap_version_of);
        let style = style_of(soap_el.and_then(|e| e.attribute("style")))
            .unwrap_or(BindingStyle::Document);
        let mut ops = Vec::new();
        for op in node.children().filter(|n| in_ns(n, WSDL_NS, "operation")) {
            let Some(oname) = op.attribute("name") else { continue };
            let soap_op = op.children().find(|n| {
                n.is_element() && n.tag_name().name() == "operation" && soap_version_of(n).is_some()
            });
            let input = op.children().find(|n| in_ns(n, WSDL_NS, "input"));
            let body = input.and_then(|i| {
                i.children().find(|n| {
                    n.is_element() && n.tag_name().name() == "body" && soap_version_of(n).is_some()
                })
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
                .find(|n| {
                    n.is_element() && n.tag_name().name() == "address" && soap_version_of(n).is_some()
                })
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
