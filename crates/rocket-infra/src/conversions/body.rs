use crate::oc::*;
use rocket_shared::types::{Body, BodyMode, FormDataEntry, FormDataType};

impl From<OcHttpRequestBody> for Body {
    fn from(oc: OcHttpRequestBody) -> Self {
        match oc {
            OcHttpRequestBody::Json { data } => Body {
                mode: BodyMode::Json,
                content: Some(data),
                form_data: None,
                file_path: None,
            },
            OcHttpRequestBody::Text { data } => Body {
                mode: BodyMode::Text,
                content: Some(data),
                form_data: None,
                file_path: None,
            },
            OcHttpRequestBody::Xml { data } => Body {
                mode: BodyMode::Xml,
                content: Some(data),
                form_data: None,
                file_path: None,
            },
            OcHttpRequestBody::Sparql { data } => Body {
                mode: BodyMode::Sparql,
                content: Some(data),
                form_data: None,
                file_path: None,
            },
            OcHttpRequestBody::FormUrlEncoded { data } => Body {
                mode: BodyMode::FormUrlEncoded,
                content: None,
                form_data: Some(data.into_iter().map(form_field_to_entry).collect()),
                file_path: None,
            },
            OcHttpRequestBody::MultipartForm { data } => Body {
                mode: BodyMode::FormData,
                content: None,
                form_data: Some(data.into_iter().map(multipart_to_entry).collect()),
                file_path: None,
            },
            OcHttpRequestBody::File { data } => Body {
                mode: BodyMode::Binary,
                content: None,
                form_data: None,
                file_path: data.first().map(|f| f.file_path.clone()),
            },
        }
    }
}

/// Convert an OC form field to a domain form-data entry.
fn form_field_to_entry(f: OcFormField) -> FormDataEntry {
    FormDataEntry {
        key: f.name,
        value: f.value,
        entry_type: FormDataType::Text,
        enabled: !f.disabled.unwrap_or(false),
        content_type: None,
        description: f.description,
    }
}

/// Convert an OC multipart form part to a domain form-data entry.
fn multipart_to_entry(p: OcMultipartFormPart) -> FormDataEntry {
    let entry_type = if p.part_type == "file" {
        FormDataType::File
    } else {
        FormDataType::Text
    };
    let value = match p.value {
        OcMultipartValue::Single(s) => s,
        OcMultipartValue::Multiple(v) => v.join(","),
    };
    FormDataEntry {
        key: p.name,
        value,
        entry_type,
        enabled: !p.disabled.unwrap_or(false),
        content_type: p.content_type,
        description: p.description,
    }
}

impl From<Body> for OcHttpRequestBody {
    fn from(b: Body) -> Self {
        match b.mode {
            BodyMode::Json => OcHttpRequestBody::Json {
                data: b.content.unwrap_or_default(),
            },
            BodyMode::Text => OcHttpRequestBody::Text {
                data: b.content.unwrap_or_default(),
            },
            BodyMode::Xml => OcHttpRequestBody::Xml {
                data: b.content.unwrap_or_default(),
            },
            BodyMode::Sparql => OcHttpRequestBody::Sparql {
                data: b.content.unwrap_or_default(),
            },
            BodyMode::FormUrlEncoded => {
                let entries = b.form_data.unwrap_or_default();
                OcHttpRequestBody::FormUrlEncoded {
                    data: entries.into_iter().map(entry_to_form_field).collect(),
                }
            }
            BodyMode::FormData => {
                let entries = b.form_data.unwrap_or_default();
                OcHttpRequestBody::MultipartForm {
                    data: entries.into_iter().map(entry_to_multipart).collect(),
                }
            }
            BodyMode::Binary => OcHttpRequestBody::File {
                data: b
                    .file_path
                    .map(|fp| vec![OcFileBodyVariant {
                        file_path: fp,
                        content_type: None,
                        selected: true,
                    }])
                    .unwrap_or_default(),
            },
            BodyMode::None => OcHttpRequestBody::Text {
                data: String::new(),
            },
        }
    }
}

/// Convert a domain form-data entry back to an OC form field.
fn entry_to_form_field(e: FormDataEntry) -> OcFormField {
    OcFormField {
        name: e.key,
        value: e.value,
        description: e.description,
        disabled: if e.enabled { None } else { Some(true) },
    }
}

/// Convert a domain form-data entry back to an OC multipart part.
fn entry_to_multipart(e: FormDataEntry) -> OcMultipartFormPart {
    OcMultipartFormPart {
        name: e.key,
        part_type: match e.entry_type {
            FormDataType::File => "file".into(),
            FormDataType::Text => "text".into(),
        },
        value: OcMultipartValue::Single(e.value),
        description: e.description,
        content_type: e.content_type,
        disabled: if e.enabled { None } else { Some(true) },
    }
}
