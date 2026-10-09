/// One part of a prompt. Images and audio are not used in v1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptPart {
    Text(String),
    /// A text resource Rocket generated from its own data, such as a request
    /// definition. It is sent as an embedded resource when the agent
    /// supports that, and as plain text otherwise.
    Resource {
        uri: String,
        mime_type: Option<String>,
        text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_parts_compare_by_value() {
        let part = PromptPart::Resource {
            uri: "rocket://request/a".to_string(),
            mime_type: Some("text/plain".to_string()),
            text: "GET /a".to_string(),
        };
        assert_eq!(part.clone(), part);
        assert_ne!(PromptPart::Text("a".to_string()), part);
    }
}
