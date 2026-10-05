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
                    self.elements
                        .insert(QName::new(tns.clone(), decl.name.clone()), decl);
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
                    self.types.insert(
                        QName::new(tns.clone(), name),
                        TypeDef::Simple(parse_simple(child)),
                    );
                }
            }
        }
    }
}

fn parse_element(
    node: Node,
    tns: &str,
    qualified_default: bool,
    top: bool,
) -> Option<ElementDecl> {
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
    let ns = if qualified {
        tns.to_string()
    } else {
        String::new()
    };
    let ty = if let Some(q) = qname_attr(node, "type") {
        ElementType::Named(q)
    } else if let Some(c) = node.children().find(|n| is_xsd(n, "complexType")) {
        ElementType::Inline(Box::new(TypeDef::Complex(parse_complex(
            c,
            tns,
            qualified_default,
        ))))
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
