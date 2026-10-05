use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use roxmltree::{Document, Node};

use super::ast::*;
use super::schema::SchemaSet;
use crate::error::{ImportError, ImportResult};

const MAX_SOURCES: usize = 32;
const MAX_SOURCE_BYTES: u64 = 5 * 1024 * 1024;
/// roxmltree recurses per nesting level and has no limit of its own.
const MAX_XML_DEPTH: usize = 256;
/// Ports kept per service. Every port repeats its binding's operations, so this bounds fan-out.
const MAX_PORTS_PER_SERVICE: usize = 256;
/// Operations kept across the whole model, counted over all ports.
const MAX_MODEL_OPERATIONS: usize = 10_000;

fn parse_err(path: &Path, message: impl Into<String>) -> ImportError {
    ImportError::ParseError {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn xml_err(path: &Path, e: roxmltree::Error) -> ImportError {
    match e {
        roxmltree::Error::DtdDetected => {
            parse_err(path, "DTDs are not allowed (found a DOCTYPE declaration)")
        }
        other => parse_err(path, other.to_string()),
    }
}

/// Deepest element nesting of an XML text, from a light scan that only follows tags.
fn xml_depth(text: &str) -> usize {
    let b = text.as_bytes();
    let (mut i, mut depth, mut max) = (0, 0usize, 0usize);
    let skip_to = |from: usize, end: &[u8]| -> usize {
        if from >= b.len() {
            return b.len();
        }
        b[from..]
            .windows(end.len())
            .position(|w| w == end)
            .map_or(b.len(), |p| from + p + end.len())
    };
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            i = skip_to(i + 4, b"-->");
        } else if rest.starts_with(b"<![CDATA[") {
            i = skip_to(i + 9, b"]]>");
        } else if rest.starts_with(b"<?") {
            i = skip_to(i + 2, b"?>");
        } else if rest.starts_with(b"<!") {
            i = skip_to(i + 2, b">");
        } else if rest.starts_with(b"</") {
            depth = depth.saturating_sub(1);
            i = skip_to(i + 2, b">");
        } else {
            // A start tag ends at the first `>` outside a quoted attribute value.
            let mut j = i + 1;
            let mut quote: Option<u8> = None;
            while j < b.len() {
                match (quote, b[j]) {
                    (None, b'"') | (None, b'\'') => quote = Some(b[j]),
                    (Some(q), c) if c == q => quote = None,
                    (None, b'>') => break,
                    _ => {}
                }
                j += 1;
            }
            if !(j > i + 1 && b.get(j - 1) == Some(&b'/')) {
                depth += 1;
                max = max.max(depth);
            }
            i = j + 1;
        }
    }
    max
}

/// Parse XML after bounding its nesting. The default options also reject DTDs.
fn parse_xml<'a>(text: &'a str, path: &Path) -> ImportResult<Document<'a>> {
    if xml_depth(text) > MAX_XML_DEPTH {
        return Err(parse_err(
            path,
            format!("XML nesting is deeper than {MAX_XML_DEPTH} levels"),
        ));
    }
    Document::parse(text).map_err(|e| xml_err(path, e))
}

/// Read a regular file of at most `MAX_SOURCE_BYTES`. Devices, pipes and directories are refused.
fn read_limited(path: &Path) -> ImportResult<String> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(parse_err(path, "not a regular file"));
    }
    let mut buf = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_SOURCE_BYTES {
        return Err(parse_err(path, "file is larger than 5 MiB"));
    }
    String::from_utf8(buf).map_err(|_| parse_err(path, "file is not valid UTF-8"))
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn in_ns(n: &Node, ns: &str, name: &str) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(ns) && n.tag_name().name() == name
}

/// URLs and UNC paths are never opened, since a UNC path would reach the network on Windows.
fn is_remote(loc: &str) -> bool {
    let mut chars = loc.chars();
    let sep = |c: Option<char>| matches!(c, Some('/' | '\\'));
    loc.contains("://") || (sep(chars.next()) && sep(chars.next()))
}

/// Locations referenced by `wsdl:import`, `xsd:import`, `xsd:include` and `xsd:redefine`.
fn import_locations(path: &Path, text: &str) -> ImportResult<Vec<String>> {
    let doc = parse_xml(text, path)?;
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

struct Source {
    path: PathBuf,
    text: String,
    locations: Vec<String>,
}

/// Read the root document and every local file it imports. Remote locations are only reported.
/// The root must be well formed, while a broken import only becomes a warning.
fn load_sources(
    origin: &Path,
    root_text: String,
    warnings: &mut Vec<String>,
) -> ImportResult<Vec<Source>> {
    let locations = import_locations(origin, &root_text)?;
    let mut out = vec![Source {
        path: origin.to_path_buf(),
        text: root_text,
        locations,
    }];
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(canonical(origin));
    let root_parent = origin
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let root_dir = canonical(root_parent);
    let mut i = 0;
    while i < out.len() {
        let path = out[i].path.clone();
        for loc in out[i].locations.clone() {
            if is_remote(&loc) {
                warnings.push(format!("remote import not fetched: {loc}"));
                continue;
            }
            let target = path.parent().unwrap_or_else(|| Path::new("")).join(&loc);
            let resolved = canonical(&target);
            if !seen.insert(resolved.clone()) {
                continue;
            }
            if out.len() >= MAX_SOURCES {
                warnings.push(format!("import limit reached, skipped: {loc}"));
                continue;
            }
            if !resolved.starts_with(&root_dir) {
                warnings.push(format!(
                    "import outside the WSDL directory was followed: {loc}"
                ));
            }
            let loaded = read_limited(&target).and_then(|text| {
                let locations = import_locations(&target, &text)?;
                Ok((text, locations))
            });
            match loaded {
                Ok((text, locations)) => out.push(Source {
                    path: target,
                    text,
                    locations,
                }),
                Err(e) => warnings.push(format!("import skipped, {loc}: {e}")),
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
    fn absorb(
        &mut self,
        root: Node,
        schemas: &mut SchemaSet,
        path: &Path,
        warnings: &mut Vec<String>,
    ) -> ImportResult<()> {
        if root.tag_name().namespace() == Some(WSDL2_NS) {
            return Err(parse_err(path, "WSDL 2.0 is not supported, use a WSDL 1.1 document"));
        }
        if in_ns(&root, XSD_NS, "schema") {
            schemas.add_schema(root, warnings);
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
                        schemas.add_schema(s, warnings);
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

/// Resolve the operations of one binding against its port type.
fn resolve_binding_ops(
    raw: &Raw,
    binding: &RawBinding,
    port_type: &[RawPortOp],
    warnings: &mut Vec<String>,
) -> Vec<WsdlOperation> {
    // Index port type operations by name to avoid a scan per binding operation.
    let mut by_name: HashMap<&str, Vec<&RawPortOp>> = HashMap::new();
    for o in port_type {
        by_name.entry(o.name.as_str()).or_default().push(o);
    }
    let mut operations = Vec::new();
    for bop in &binding.ops {
        let pt_op = by_name.get(bop.name.as_str()).and_then(|c| {
            c.iter()
                .find(|o| bop.input_name.is_none() || o.input_name == bop.input_name)
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
    operations
}

fn build_model(raw: Raw, schemas: SchemaSet, mut warnings: Vec<String>) -> WsdlModel {
    let mut services = Vec::new();
    let mut total_ops = 0usize;
    let mut op_limit_hit = false;
    // Operations per binding, resolved once however many ports share the binding.
    let mut resolved: HashMap<QName, Vec<WsdlOperation>> = HashMap::new();
    for svc in &raw.services {
        let mut ports = Vec::new();
        if svc.ports.len() > MAX_PORTS_PER_SERVICE {
            warnings.push(format!(
                "service {} has {} ports, only the first {MAX_PORTS_PER_SERVICE} are read",
                svc.name,
                svc.ports.len()
            ));
        }
        for port in svc.ports.iter().take(MAX_PORTS_PER_SERVICE) {
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
            if !resolved.contains_key(&port.binding) {
                let ops = resolve_binding_ops(&raw, binding, port_type, &mut warnings);
                resolved.insert(port.binding.clone(), ops);
            }
            let mut operations = resolved.get(&port.binding).cloned().unwrap_or_default();
            let room = MAX_MODEL_OPERATIONS.saturating_sub(total_ops);
            if operations.len() > room {
                operations.truncate(room);
                op_limit_hit = true;
            }
            total_ops += operations.len();
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
    if op_limit_hit {
        warnings.push(format!(
            "more than {MAX_MODEL_OPERATIONS} operations across ports, the rest are not read"
        ));
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
    for (idx, src) in sources.iter().enumerate() {
        // The default options reject DTDs, which closes XXE and entity expansion.
        let result = parse_xml(&src.text, &src.path)
            .and_then(|doc| raw.absorb(doc.root_element(), &mut schemas, &src.path, &mut warnings));
        match result {
            Ok(()) => {}
            // Only the root document is fatal.
            Err(e) if idx == 0 => return Err(e),
            Err(e) => warnings.push(format!("import skipped, {}: {e}", src.path.display())),
        }
    }
    Ok(build_model(raw, schemas, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wsdl::{BindingStyle, PartKind, QName, SoapVersion};
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
        let err = parse_wsdl_str(wsdl, Path::new("dtd.wsdl")).expect_err("must fail");
        assert!(err.to_string().contains("DTD"), "got: {err}");
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

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("test file is written");
        path
    }

    const GOOD_XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:g">
        <xs:simpleType name="Good"><xs:restriction base="xs:string"/></xs:simpleType></xs:schema>"#;

    fn wsdl_importing(locations: &[&str]) -> String {
        let imports: String = locations
            .iter()
            .map(|l| format!(r#"<import namespace="urn:o" location="{l}"/>"#))
            .collect();
        format!(
            r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/" targetNamespace="urn:r">{imports}</definitions>"#
        )
    }

    #[test]
    fn read_limited_refuses_directories() {
        let dir = tempfile::tempdir().expect("temp dir");
        let err = read_limited(dir.path()).expect_err("directory must fail");
        assert!(err.to_string().contains("not a regular file"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn read_limited_refuses_devices() {
        let err = read_limited(Path::new("/dev/zero")).expect_err("device must fail");
        assert!(err.to_string().contains("not a regular file"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn read_limited_refuses_fifos_without_blocking() {
        let dir = tempfile::tempdir().expect("temp dir");
        let fifo = dir.path().join("pipe.xsd");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo runs");
        assert!(status.success());
        let err = read_limited(&fifo).expect_err("fifo must fail");
        assert!(err.to_string().contains("not a regular file"), "got: {err}");
    }

    #[test]
    fn read_limited_rejects_over_limit_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let big = write(dir.path(), "big.xsd", &"a".repeat(MAX_SOURCE_BYTES as usize + 1));
        let err = read_limited(&big).expect_err("over limit must fail");
        assert!(err.to_string().contains("5 MiB"), "got: {err}");
        let ok = write(dir.path(), "ok.xsd", &"a".repeat(MAX_SOURCE_BYTES as usize));
        assert!(read_limited(&ok).is_ok());
    }

    #[test]
    fn unc_and_url_locations_are_remote() {
        assert!(is_remote("//host/share/x.xsd"));
        assert!(is_remote(r"\\host\share\x.xsd"));
        assert!(is_remote("http://h/x.xsd"));
        assert!(is_remote("file:///etc/passwd"));
        assert!(!is_remote("calc-types.xsd"));
        assert!(!is_remote("../x.xsd"));
    }

    #[test]
    fn unc_imports_are_warned_not_opened() {
        let wsdl = wsdl_importing(&["//host/share/x.xsd", r"\\host\share\y.xsd"]);
        let model = parse_wsdl_str(&wsdl, Path::new("unc.wsdl")).expect("parses");
        let n = model.warnings.iter().filter(|w| w.contains("not fetched")).count();
        assert_eq!(n, 2, "got: {:?}", model.warnings);
    }

    #[test]
    fn import_outside_the_wsdl_tree_is_followed_with_a_warning() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir(dir.path().join("inner")).expect("inner dir");
        write(dir.path(), "outside.xsd", GOOD_XSD);
        let root = write(&dir.path().join("inner"), "main.wsdl", &wsdl_importing(&["../outside.xsd"]));
        let model = parse_wsdl_file(&root).expect("parses");
        assert!(
            model.warnings.iter().any(|w| w.contains("outside the WSDL directory")),
            "got: {:?}",
            model.warnings
        );
        assert!(model.schemas.types.contains_key(&QName::new("urn:g", "Good")));
    }

    #[test]
    fn import_inside_the_tree_has_no_outside_warning() {
        let dir = tempfile::tempdir().expect("temp dir");
        write(dir.path(), "good.xsd", GOOD_XSD);
        let root = write(dir.path(), "main.wsdl", &wsdl_importing(&["good.xsd"]));
        let model = parse_wsdl_file(&root).expect("parses");
        assert!(model.warnings.is_empty(), "got: {:?}", model.warnings);
    }

    #[test]
    fn broken_imports_become_warnings_and_the_rest_imports() {
        let dir = tempfile::tempdir().expect("temp dir");
        write(dir.path(), "notxml.xsd", "this is not xml");
        write(
            dir.path(),
            "dtd.xsd",
            "<?xml version=\"1.0\"?>\n<!DOCTYPE d [<!ENTITY x \"boom\">]>\n<d>&x;</d>",
        );
        write(dir.path(), "wrongroot.xsd", "<foo/>");
        std::fs::create_dir(dir.path().join("adir")).expect("dir");
        write(dir.path(), "good.xsd", GOOD_XSD);
        let root = write(
            dir.path(),
            "main.wsdl",
            &wsdl_importing(&["notxml.xsd", "dtd.xsd", "wrongroot.xsd", "adir", "missing.xsd", "good.xsd"]),
        );
        let model = parse_wsdl_file(&root).expect("a bad import must not abort");
        let skipped = model.warnings.iter().filter(|w| w.contains("import skipped")).count();
        assert_eq!(skipped, 5, "got: {:?}", model.warnings);
        assert!(model.warnings.iter().any(|w| w.contains("DTD")), "got: {:?}", model.warnings);
        assert!(model.schemas.types.contains_key(&QName::new("urn:g", "Good")));
    }

    #[test]
    fn root_with_doctype_is_fatal() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = write(
            dir.path(),
            "main.wsdl",
            "<?xml version=\"1.0\"?>\n<!DOCTYPE d [<!ENTITY x \"boom\">]>\n<definitions xmlns=\"http://schemas.xmlsoap.org/wsdl/\"/>",
        );
        let err = parse_wsdl_file(&root).expect_err("root DTD must fail");
        assert!(err.to_string().contains("DTD"), "got: {err}");
    }

    #[test]
    fn deeply_nested_schema_is_cut_off_with_a_warning() {
        let mut xsd = String::from(r#"<xsd:complexType name="Deep">"#);
        for _ in 0..200 {
            xsd.push_str("<xsd:sequence>");
        }
        xsd.push_str(r#"<xsd:element name="leaf" type="xsd:string"/>"#);
        for _ in 0..200 {
            xsd.push_str("</xsd:sequence>");
        }
        xsd.push_str("</xsd:complexType>");
        let wsdl = format!(
            r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
               <types><xsd:schema targetNamespace="urn:t">{xsd}</xsd:schema></types></definitions>"#
        );
        let model = parse_wsdl_str(&wsdl, Path::new("deep.wsdl")).expect("parses");
        assert!(model.warnings.iter().any(|w| w.contains("nesting")), "got: {:?}", model.warnings);
    }

    #[test]
    fn deeply_nested_anonymous_elements_are_bounded() {
        let mut xsd = String::from(r#"<xsd:element name="Root">"#);
        for _ in 0..50 {
            xsd.push_str(r#"<xsd:complexType><xsd:sequence><xsd:element name="n">"#);
        }
        for _ in 0..50 {
            xsd.push_str("</xsd:element></xsd:sequence></xsd:complexType>");
        }
        xsd.push_str("</xsd:element>");
        let wsdl = format!(
            r#"<definitions xmlns="http://schemas.xmlsoap.org/wsdl/" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
               <types><xsd:schema targetNamespace="urn:t">{xsd}</xsd:schema></types></definitions>"#
        );
        let model = parse_wsdl_str(&wsdl, Path::new("deep2.wsdl")).expect("parses");
        assert!(model.warnings.iter().any(|w| w.contains("nesting")), "got: {:?}", model.warnings);
    }

    fn nested(depth: usize) -> String {
        format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth))
    }

    #[test]
    fn xml_depth_counts_nesting_and_ignores_non_elements() {
        assert_eq!(xml_depth(&nested(3)), 3);
        assert_eq!(xml_depth("<a><b/><c x=\">\"/></a>"), 1);
        assert_eq!(xml_depth("<a><!-- <b><c> --><![CDATA[<d><e>]]></a>"), 1);
        assert_eq!(xml_depth("<?pi <x> ?><a><b></b></a>"), 2);
    }

    #[test]
    fn ten_thousand_levels_in_the_root_fail_cleanly() {
        let err = parse_wsdl_str(&nested(10_000), Path::new("deep.wsdl")).expect_err("must fail");
        assert!(err.to_string().contains("nesting"), "got: {err}");
    }

    #[test]
    fn ten_thousand_levels_in_an_import_only_warn() {
        let dir = tempfile::tempdir().expect("temp dir");
        write(dir.path(), "deep.xsd", &nested(10_000));
        write(dir.path(), "good.xsd", GOOD_XSD);
        let root = write(dir.path(), "main.wsdl", &wsdl_importing(&["deep.xsd", "good.xsd"]));
        let model = parse_wsdl_file(&root).expect("a deep import must not abort");
        assert!(
            model.warnings.iter().any(|w| w.contains("nesting")),
            "got: {:?}",
            model.warnings
        );
        assert!(model.schemas.types.contains_key(&QName::new("urn:g", "Good")));
    }

    #[test]
    fn mixed_separator_unc_forms_are_remote() {
        assert!(is_remote(r"/\host\share\x.xsd"));
        assert!(is_remote(r"\/host/share/x.xsd"));
        assert!(!is_remote("/abs/path.xsd"));
        assert!(!is_remote(r"\single.xsd"));
    }

    #[test]
    fn mixed_separator_unc_imports_are_warned_not_opened() {
        let wsdl = wsdl_importing(&[r"/\host\share\x.xsd", r"\/host/share/y.xsd"]);
        let model = parse_wsdl_str(&wsdl, Path::new("mixed.wsdl")).expect("parses");
        let n = model.warnings.iter().filter(|w| w.contains("not fetched")).count();
        assert_eq!(n, 2, "got: {:?}", model.warnings);
    }
}
