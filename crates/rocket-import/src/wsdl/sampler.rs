use std::collections::BTreeMap;

use super::ast::{QName, XSD_NS};
use super::schema::{ElementDecl, ElementType, SchemaSet, SimpleType, TypeDef};

/// Element nesting beyond this depth is replaced by a marker comment.
pub(crate) const MAX_DEPTH: usize = 8;
/// Total elements one sample may contain. Breadth is bounded here, depth by `MAX_DEPTH`.
const MAX_NODES: usize = 5_000;
/// Longest element name written into a sample.
const MAX_NAME_LEN: usize = 128;
/// Longest text value written into a sample.
const MAX_VALUE_LEN: usize = 1_024;
/// Longest chain of nested type definitions followed while rendering.
const MAX_TYPE_CHAIN: usize = 64;

/// Escape text content.
pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Escape an attribute value.
pub(crate) fn esc_attr(s: &str) -> String {
    esc(s).replace('"', "&quot;")
}

/// Type names may legally contain `--` or end in `-`, both illegal inside an XML comment.
fn comment_safe(s: &str) -> String {
    let mut out = s.to_string();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_end_matches('-').to_string()
}

/// Turn any string into a valid XML name, so a sample can never carry injected markup.
fn safe_name(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().take(MAX_NAME_LEN).enumerate() {
        let start = c.is_alphabetic() || c == '_';
        let rest = start || c.is_ascii_digit() || c == '-' || c == '.';
        if i == 0 && !start {
            out.push('_');
        }
        out.push(if rest { c } else { '_' });
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

/// Escape sample text and cap its length.
fn text(s: &str) -> String {
    esc(&s.chars().take(MAX_VALUE_LEN).collect::<String>())
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
/// The node budget covers the whole sampler, so use one sampler per message body.
pub(crate) struct Sampler<'a> {
    schemas: &'a SchemaSet,
    prefixes: BTreeMap<String, String>,
    stack: Vec<QName>,
    nodes_left: usize,
}

impl<'a> Sampler<'a> {
    pub(crate) fn new(schemas: &'a SchemaSet) -> Self {
        Self {
            schemas,
            prefixes: BTreeMap::new(),
            stack: Vec::new(),
            nodes_left: MAX_NODES,
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
        let local = safe_name(local);
        match self.prefix_for(ns) {
            Some(p) => format!("{p}:{local}"),
            None => local,
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
        let local = safe_name(local);
        let content = self.render_named(ty, 0);
        format!("<{local}>{content}</{local}>")
    }

    /// Wrap already rendered content in a (possibly qualified) element.
    pub(crate) fn wrap(&mut self, ns: &str, local: &str, inner: &str) -> String {
        let tag = self.tag(ns, local);
        format!("<{tag}>{inner}</{tag}>")
    }

    fn render_decl(&mut self, d: &'a ElementDecl, depth: usize) -> String {
        if self.nodes_left == 0 {
            return "<!-- size limit -->".to_string();
        }
        self.nodes_left -= 1;
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
            return format!("<{tag}>{}</{tag}>", text(v));
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
        if self.stack.len() >= MAX_TYPE_CHAIN {
            return "<!-- type nesting limit -->".to_string();
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
            return text(e);
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

    fn assert_wellformed(s: &Sampler, xml: &str) {
        let decls: String = s
            .namespaces()
            .iter()
            .map(|(uri, p)| format!(r#" xmlns:{p}="{}""#, esc_attr(uri)))
            .collect();
        let doc = format!("<r{decls}>{xml}</r>");
        if let Err(e) = roxmltree::Document::parse(&doc) {
            panic!("not well formed: {e}: {doc}");
        }
    }

    fn has_element(s: &Sampler, xml: &str, name: &str) -> bool {
        let decls: String = s
            .namespaces()
            .iter()
            .map(|(uri, p)| format!(r#" xmlns:{p}="{}""#, esc_attr(uri)))
            .collect();
        let doc = format!("<r{decls}>{xml}</r>");
        let parsed = roxmltree::Document::parse(&doc).expect("sample is well formed");
        parsed
            .descendants()
            .any(|n| n.is_element() && n.tag_name().name() == name)
    }

    #[test]
    fn comment_safe_never_leaves_a_dash_run_or_trailing_dash() {
        for input in ["---", "a--->", "a-", "a----b", "x---><inj/><!--", "-"] {
            let out = comment_safe(input);
            assert!(!out.contains("--"), "{input:?} -> {out:?}");
            assert!(!out.ends_with('-'), "{input:?} -> {out:?}");
        }
        assert_eq!(comment_safe("a--->"), "a->");
    }

    #[test]
    fn recursive_type_name_cannot_close_the_comment() {
        let model = model_with_schema(
            r#"<xsd:complexType name="x---&gt;&lt;inj/&gt;&lt;!--"><xsd:sequence>
                 <xsd:element name="child" type="t:x---&gt;&lt;inj/&gt;&lt;!--"/>
               </xsd:sequence></xsd:complexType>
               <xsd:element name="Root" type="t:x---&gt;&lt;inj/&gt;&lt;!--"/>"#,
        );
        let mut s = Sampler::new(&model.schemas);
        let xml = s.element(&QName::new("urn:t", "Root"));
        assert!(xml.contains("<!-- recursive type:"), "got: {xml}");
        assert!(!has_element(&s, &xml, "inj"), "injected element in: {xml}");
    }

    #[test]
    fn hostile_element_names_cannot_inject_markup() {
        let model = model_with_schema(
            r#"<xsd:element name="a&gt;&lt;evil/&gt;" type="xsd:string"/>
               <xsd:element name="Pick"><xsd:complexType><xsd:sequence>
                 <xsd:element ref="t:b&gt;&lt;evil/&gt;"/>
                 <xsd:element name="c&gt;&lt;evil/&gt;" type="xsd:string"/>
               </xsd:sequence></xsd:complexType></xsd:element>"#,
        );
        let mut s = Sampler::new(&model.schemas);
        for name in ["a><evil/>", "Pick", "missing><evil/>"] {
            let xml = s.element(&QName::new("urn:t", name));
            assert!(!has_element(&s, &xml, "evil"), "injected element in: {xml}");
        }
    }

    #[test]
    fn hostile_part_and_wrapper_names_cannot_inject_markup() {
        let model = model_with_schema("");
        let mut s = Sampler::new(&model.schemas);
        let typed = s.typed_element(
            "p><evil/>",
            &QName::new("http://www.w3.org/2001/XMLSchema", "string"),
        );
        let wrapped = s.wrap("urn:t", "w><evil/>", &typed);
        for xml in [&typed, &wrapped] {
            assert!(!has_element(&s, xml, "evil"), "injected element in: {xml}");
        }
    }

    #[test]
    fn safe_name_always_yields_a_valid_name() {
        assert_eq!(safe_name("ok.name-1"), "ok.name-1");
        assert_eq!(safe_name("1abc"), "_1abc");
        assert_eq!(safe_name(""), "_");
        assert_eq!(safe_name("a:b"), "a_b");
        assert_eq!(safe_name(&"x".repeat(1000)).len(), MAX_NAME_LEN);
    }

    #[test]
    fn wide_fan_out_is_bounded_by_the_node_budget() {
        // Eight distinct types of ten children each would expand to about 10^8 elements.
        let mut xsd = String::new();
        for i in 0..8 {
            let child = if i == 7 {
                "xsd:string".to_string()
            } else {
                format!("t:T{}", i + 1)
            };
            xsd.push_str(&format!(r#"<xsd:complexType name="T{i}"><xsd:sequence>"#));
            for j in 0..10 {
                xsd.push_str(&format!(r#"<xsd:element name="p{j}" type="{child}"/>"#));
            }
            xsd.push_str("</xsd:sequence></xsd:complexType>");
        }
        xsd.push_str(r#"<xsd:element name="Root" type="t:T0"/>"#);
        let model = model_with_schema(&xsd);
        let mut s = Sampler::new(&model.schemas);
        let xml = s.element(&QName::new("urn:t", "Root"));
        assert!(xml.contains("<!-- size limit -->"), "marker missing");
        assert!(xml.len() < 500_000, "output was {} bytes", xml.len());
        assert_wellformed(&s, &xml);
    }

    #[test]
    fn long_base_type_chain_is_bounded() {
        let mut xsd = String::new();
        for i in 0..500 {
            xsd.push_str(&format!(
                r#"<xsd:complexType name="C{i}"><xsd:complexContent><xsd:extension base="t:C{}"><xsd:sequence/></xsd:extension></xsd:complexContent></xsd:complexType>"#,
                i + 1
            ));
        }
        xsd.push_str(r#"<xsd:element name="Root" type="t:C0"/>"#);
        let model = model_with_schema(&xsd);
        let mut s = Sampler::new(&model.schemas);
        let xml = s.element(&QName::new("urn:t", "Root"));
        assert!(xml.contains("<!-- type nesting limit -->"), "got: {xml}");
    }

    fn fan_out_xsd() -> String {
        let mut xsd = String::new();
        for i in 0..8 {
            let child = if i == 7 {
                "xsd:string".to_string()
            } else {
                format!("t:T{}", i + 1)
            };
            xsd.push_str(&format!(r#"<xsd:complexType name="T{i}"><xsd:sequence>"#));
            for j in 0..10 {
                xsd.push_str(&format!(r#"<xsd:element name="p{j}" type="{child}"/>"#));
            }
            xsd.push_str("</xsd:sequence></xsd:complexType>");
        }
        xsd.push_str(r#"<xsd:element name="Root" type="t:T0"/>"#);
        xsd
    }

    #[test]
    fn budget_is_shared_across_calls_on_one_sampler() {
        // A message with many parts reuses one sampler, so the budget covers the whole body.
        let model = model_with_schema(&fan_out_xsd());
        let mut s = Sampler::new(&model.schemas);
        let t0 = QName::new("urn:t", "T0");
        let mut body = String::new();
        for i in 0..200 {
            if i % 2 == 0 {
                body.push_str(&s.typed_element("part", &t0));
            } else {
                body.push_str(&s.element(&QName::new("urn:t", "Root")));
            }
        }
        assert!(body.contains("<!-- size limit -->"), "marker missing");
        assert!(body.len() < 1_000_000, "total output was {} bytes", body.len());
        assert_wellformed(&s, &body);
    }

    #[test]
    fn a_fresh_sampler_gets_a_fresh_budget() {
        let model = model_with_schema(&fan_out_xsd());
        let mut spent = Sampler::new(&model.schemas);
        for _ in 0..4 {
            spent.element(&QName::new("urn:t", "Root"));
        }
        let exhausted = spent.element(&QName::new("urn:t", "Root"));
        assert_eq!(exhausted, "<!-- size limit -->");
        let mut fresh = Sampler::new(&model.schemas);
        let first = fresh.element(&QName::new("urn:t", "Root"));
        assert!(first.starts_with("<ns1:Root>"), "got: {}", &first[..40]);
    }
}
