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
