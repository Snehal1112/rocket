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
