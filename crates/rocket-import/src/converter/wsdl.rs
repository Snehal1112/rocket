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
    // Quotes, backslashes and control characters cannot appear in a valid action URI.
    // Percent-encode them so they cannot end the value or break the header.
    let mut encoded = String::with_capacity(action.len());
    for c in action.chars() {
        if c == '"' || c == '\\' || c.is_control() {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                encoded.push_str(&format!("%{b:02X}"));
            }
        } else {
            encoded.push(c);
        }
    }
    let action = encoded;
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
        assert_eq!(
            h[0].value,
            "application/soap+xml; charset=utf-8; action=\"urn:do\""
        );
        let none = soap_headers(SoapVersion::V12, "");
        assert_eq!(none[0].value, "application/soap+xml; charset=utf-8");
    }

    #[test]
    fn action_with_quote_cannot_break_out_of_the_header() {
        let h = soap_headers(SoapVersion::V12, "a\"b");
        assert_eq!(
            h[0].value,
            "application/soap+xml; charset=utf-8; action=\"a%22b\""
        );
    }

    #[test]
    fn action_with_backslash_is_encoded() {
        let h = soap_headers(SoapVersion::V11, "urn:x\\");
        assert_eq!(h[1].value, "\"urn:x%5C\"");
        let h = soap_headers(SoapVersion::V12, "urn:x\\");
        assert_eq!(
            h[0].value,
            "application/soap+xml; charset=utf-8; action=\"urn:x%5C\""
        );
    }

    #[test]
    fn action_with_control_characters_is_encoded() {
        let h = soap_headers(SoapVersion::V11, "a\nb\r\tc\u{7f}");
        assert_eq!(h[1].value, "\"a%0Ab%0D%09c%7F\"");
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
        assert!(body
            .content
            .as_deref()
            .unwrap_or("")
            .contains("<soapenv:Envelope"));
        assert_eq!(
            header(&req, "Content-Type"),
            Some("text/xml; charset=utf-8")
        );
        assert_eq!(
            header(&req, "SOAPAction"),
            Some("\"http://example.com/calc/Add\"")
        );
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
