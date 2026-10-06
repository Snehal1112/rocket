use prost_reflect::{DynamicMessage, MessageDescriptor, SerializeOptions};
use rocket_shared::error::{DomainError, DomainResult};

/// Parses `json` as the protobuf JSON mapping of `desc`. Blank text counts as
/// `{}`. Unknown fields, bad enum names and conflicting oneof members fail.
pub fn json_to_message(desc: &MessageDescriptor, json: &str) -> DomainResult<DynamicMessage> {
    let text = if json.trim().is_empty() { "{}" } else { json };
    let invalid =
        |e: String| DomainError::InvalidInput(format!("invalid {} message: {e}", desc.full_name()));
    let mut de = serde_json::Deserializer::from_str(text);
    let message =
        DynamicMessage::deserialize(desc.clone(), &mut de).map_err(|e| invalid(e.to_string()))?;
    de.end().map_err(|e| invalid(e.to_string()))?;
    Ok(message)
}

/// Pretty JSON for `message`. Fields that hold their default value are written,
/// so the result doubles as an editable template. 64-bit integers are strings.
pub fn message_to_json(message: &DynamicMessage) -> DomainResult<String> {
    let options = SerializeOptions::new().skip_default_fields(false);
    let mut out = Vec::new();
    let mut serializer = serde_json::Serializer::pretty(&mut out);
    message
        .serialize_with_options(&mut serializer, &options)
        .map_err(|e| DomainError::Serialization(format!("could not write message JSON: {e}")))?;
    String::from_utf8(out).map_err(|e| DomainError::Serialization(e.to_string()))
}

/// A JSON template for `desc` with every field at its default value.
pub fn empty_message_json(desc: &MessageDescriptor) -> DomainResult<String> {
    message_to_json(&DynamicMessage::new(desc.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::greeter_registry;
    use prost::Message;

    fn hello_request() -> MessageDescriptor {
        greeter_registry()
            .pool()
            .get_message_by_name("demo.greeter.v1.HelloRequest")
            .expect("HelloRequest")
    }

    const FULL: &str = r#"{
        "name": "ada",
        "tags": ["a", "b"],
        "mood": "HAPPY",
        "counters": {"x": 5},
        "email": "ada@example.com",
        "address": {"street": "1 Main", "city": "Paris"},
        "sentAt": "2024-01-02T03:04:05Z",
        "blob": "aGk=",
        "big": "9007199254740993",
        "stops": [{"city": "Rome"}, {"city": "Oslo"}],
        "byId": {"7": {"city": "Kyiv"}}
    }"#;

    #[test]
    fn a_full_message_survives_json_to_bytes_to_json() {
        let desc = hello_request();
        let message = json_to_message(&desc, FULL).expect("parse");
        let bytes = message.encode_to_vec();
        let decoded = DynamicMessage::decode(desc, bytes.as_slice()).expect("decode");
        let json: serde_json::Value =
            serde_json::from_str(&message_to_json(&decoded).expect("write")).expect("json");
        assert_eq!(json["name"], "ada");
        assert_eq!(json["tags"], serde_json::json!(["a", "b"]));
        assert_eq!(json["mood"], "HAPPY");
        assert_eq!(json["counters"]["x"], "5");
        assert_eq!(json["email"], "ada@example.com");
        assert_eq!(json["address"]["city"], "Paris");
        assert_eq!(json["sentAt"], "2024-01-02T03:04:05Z");
        assert_eq!(json["blob"], "aGk=");
        assert_eq!(json["stops"][1]["city"], "Oslo");
        assert_eq!(json["byId"]["7"]["city"], "Kyiv");
    }

    #[test]
    fn sixty_four_bit_integers_keep_every_digit() {
        let desc = hello_request();
        let message = json_to_message(&desc, r#"{"big": "9007199254740993"}"#).expect("parse");
        let json: serde_json::Value =
            serde_json::from_str(&message_to_json(&message).expect("write")).expect("json");
        assert_eq!(json["big"], "9007199254740993");
    }

    #[test]
    fn an_enum_is_accepted_by_name_or_number_and_written_by_name() {
        let desc = hello_request();
        for text in [r#"{"mood":"GRUMPY"}"#, r#"{"mood":2}"#] {
            let message = json_to_message(&desc, text).expect("parse");
            assert!(message_to_json(&message)
                .expect("write")
                .contains("\"GRUMPY\""));
        }
    }

    #[test]
    fn two_members_of_one_oneof_are_rejected() {
        let desc = hello_request();
        let err =
            json_to_message(&desc, r#"{"email":"a@b.c","phone":"1"}"#).expect_err("oneof conflict");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m.contains("oneof 'contact'")),
            "got: {err:?}"
        );
    }

    #[test]
    fn bad_input_is_an_invalid_input_that_names_the_message_type() {
        let desc = hello_request();
        for text in [
            r#"{"nope": 1}"#,
            r#"{"mood": "SAD"}"#,
            r#"{"tags": "x"}"#,
            r#"{} trailing"#,
            r#"{"name": "#,
        ] {
            let err = json_to_message(&desc, text).expect_err(text);
            assert!(
                matches!(&err, DomainError::InvalidInput(m) if m.contains("demo.greeter.v1.HelloRequest")),
                "{text}: {err:?}"
            );
        }
    }

    #[test]
    fn blank_text_is_an_empty_message() {
        let desc = hello_request();
        for text in ["", "   \n"] {
            let message = json_to_message(&desc, text).expect("blank");
            assert!(message.encode_to_vec().is_empty());
        }
    }

    #[test]
    fn the_template_lists_every_plain_field_at_its_default() {
        let json: serde_json::Value =
            serde_json::from_str(&empty_message_json(&hello_request()).expect("template"))
                .expect("json");
        assert_eq!(json["name"], "");
        assert_eq!(json["tags"], serde_json::json!([]));
        assert_eq!(json["mood"], "MOOD_UNSPECIFIED");
        assert_eq!(json["counters"], serde_json::json!({}));
        assert_eq!(json["big"], "0");
        assert!(
            json.get("email").is_none(),
            "unset oneof members are omitted"
        );
    }

    #[test]
    fn snake_case_field_names_are_accepted_on_input() {
        let desc = hello_request();
        let message =
            json_to_message(&desc, r#"{"by_id": {"1": {"city": "Rome"}}}"#).expect("snake case");
        assert!(message_to_json(&message)
            .expect("write")
            .contains("\"byId\""));
    }
}
