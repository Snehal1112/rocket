use crate::oc::*;
use rocket_shared::types::Header;

impl From<OcHttpRequestHeader> for Header {
    fn from(oc: OcHttpRequestHeader) -> Self {
        Header {
            key: oc.name,
            value: oc.value,
            enabled: !oc.disabled.unwrap_or(false),
            description: oc.description,
        }
    }
}

impl From<Header> for OcHttpRequestHeader {
    fn from(h: Header) -> Self {
        OcHttpRequestHeader {
            name: h.key,
            value: h.value,
            description: h.description,
            // Omit disabled entirely when enabled (cleaner YAML output).
            disabled: if h.enabled { None } else { Some(true) },
        }
    }
}
