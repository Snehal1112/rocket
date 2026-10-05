//! WSDL 1.1 reader: services, ports, SOAP operations and sample XML from XSD.

pub(crate) mod ast;
pub(crate) mod parser;
pub(crate) mod sampler;
pub(crate) mod schema;

#[cfg(test)]
pub(crate) use ast::{QName, WsdlModel};
pub(crate) use ast::{BindingStyle, PartKind, SoapVersion, WsdlOperation, WsdlPort};
pub(crate) use parser::parse_wsdl_file;
#[cfg(test)]
pub(crate) use parser::parse_wsdl_str;
pub(crate) use sampler::{esc_attr, Sampler};
pub(crate) use schema::SchemaSet;
