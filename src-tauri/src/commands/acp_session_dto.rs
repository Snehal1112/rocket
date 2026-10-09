//! IPC DTOs for ACP agent sessions.
//!
//! The `rocket-acp` domain types carry no camelCase serde. These DTOs own the
//! camelCase wire shape that `src/lib/tauri-api.ts` reads and sends.

use rocket_acp::{ConfigChoice, ConfigOption, PromptPart, SessionInfo};
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

/// Most resources one prompt may carry (the spec's chip limit).
pub const MAX_PROMPT_RESOURCES: usize = 8;
/// Largest resource text in bytes (the spec's 8 KB per chip).
pub const MAX_PROMPT_RESOURCE_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigChoiceDto {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigOptionDto {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub current_value: String,
    pub choices: Vec<ConfigChoiceDto>,
}

/// What `start_workspace_assistant` returns. Prompt capabilities stay in the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionStartedDto {
    pub session_id: String,
    pub config_options: Vec<ConfigOptionDto>,
}

/// One text resource sent with a prompt, such as a request definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptResourceDto {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: String,
}

impl From<ConfigChoice> for ConfigChoiceDto {
    fn from(choice: ConfigChoice) -> Self {
        // Destructure fully, so a new domain field fails to compile here.
        let ConfigChoice {
            value,
            name,
            description,
        } = choice;
        Self {
            value,
            name,
            description,
        }
    }
}

impl From<ConfigOption> for ConfigOptionDto {
    fn from(option: ConfigOption) -> Self {
        let ConfigOption {
            id,
            name,
            category,
            current_value,
            choices,
        } = option;
        Self {
            id,
            name,
            category,
            current_value,
            choices: choices.into_iter().map(ConfigChoiceDto::from).collect(),
        }
    }
}

impl From<SessionInfo> for AgentSessionStartedDto {
    fn from(info: SessionInfo) -> Self {
        Self {
            session_id: info.session_id,
            config_options: info
                .config_options
                .into_iter()
                .map(ConfigOptionDto::from)
                .collect(),
        }
    }
}

/// Builds the prompt parts: the resources first, then the prompt text. The
/// limits are checked here, at the IPC boundary, so nothing oversized reaches
/// the agent. The errors name the limit, never the resource content.
pub fn prompt_parts(
    prompt: String,
    resources: Option<Vec<PromptResourceDto>>,
) -> DomainResult<Vec<PromptPart>> {
    let resources = resources.unwrap_or_default();
    if resources.len() > MAX_PROMPT_RESOURCES {
        return Err(DomainError::InvalidInput(format!(
            "a prompt may carry at most {MAX_PROMPT_RESOURCES} resources"
        )));
    }
    let mut parts = Vec::with_capacity(resources.len() + 1);
    for resource in resources {
        if resource.text.len() > MAX_PROMPT_RESOURCE_BYTES {
            return Err(DomainError::InvalidInput(format!(
                "a prompt resource may hold at most {MAX_PROMPT_RESOURCE_BYTES} bytes"
            )));
        }
        parts.push(PromptPart::Resource {
            uri: resource.uri,
            mime_type: resource.mime_type,
            text: resource.text,
        });
    }
    if !prompt.trim().is_empty() {
        parts.push(PromptPart::Text(prompt));
    }
    if parts.is_empty() {
        return Err(DomainError::InvalidInput("the prompt is empty".to_string()));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::PromptCapabilities;

    fn resource(uri: &str, text: &str) -> PromptResourceDto {
        PromptResourceDto {
            uri: uri.to_string(),
            mime_type: Some("text/plain".to_string()),
            text: text.to_string(),
        }
    }

    #[test]
    fn session_started_dto_serializes_camel_case() {
        let info = SessionInfo {
            session_id: "s-1".to_string(),
            config_options: vec![ConfigOption {
                id: "model".to_string(),
                name: "Model".to_string(),
                category: Some("model".to_string()),
                current_value: "opus".to_string(),
                choices: vec![ConfigChoice {
                    value: "opus".to_string(),
                    name: "Opus".to_string(),
                    description: None,
                }],
            }],
            prompt_capabilities: PromptCapabilities::default(),
        };
        let json = serde_json::to_string(&AgentSessionStartedDto::from(info)).expect("serialize");
        assert_eq!(
            json,
            r#"{"sessionId":"s-1","configOptions":[{"id":"model","name":"Model","category":"model","currentValue":"opus","choices":[{"value":"opus","name":"Opus","description":null}]}]}"#
        );
    }

    #[test]
    fn prompt_resource_dto_reads_camel_case_and_a_missing_mime_type() {
        let dto: PromptResourceDto =
            serde_json::from_str(r#"{"uri":"rocket://a","mimeType":"text/plain","text":"x"}"#)
                .expect("deserialize");
        assert_eq!(dto.mime_type.as_deref(), Some("text/plain"));
        let dto: PromptResourceDto =
            serde_json::from_str(r#"{"uri":"rocket://a","text":"x"}"#).expect("deserialize");
        assert_eq!(dto.mime_type, None);
    }

    #[test]
    fn prompt_parts_puts_resources_before_the_prompt_text() {
        let parts = prompt_parts(
            "explain".to_string(),
            Some(vec![resource("rocket://request/a", "GET /a")]),
        )
        .expect("parts");
        assert_eq!(
            parts,
            vec![
                PromptPart::Resource {
                    uri: "rocket://request/a".to_string(),
                    mime_type: Some("text/plain".to_string()),
                    text: "GET /a".to_string(),
                },
                PromptPart::Text("explain".to_string()),
            ]
        );
    }

    #[test]
    fn prompt_parts_without_resources_is_one_text_part() {
        let parts = prompt_parts("hi".to_string(), None).expect("parts");
        assert_eq!(parts, vec![PromptPart::Text("hi".to_string())]);
    }

    #[test]
    fn prompt_parts_rejects_too_many_resources() {
        let resources = (0..=MAX_PROMPT_RESOURCES)
            .map(|i| resource(&format!("rocket://r/{i}"), "x"))
            .collect();
        let err = prompt_parts("hi".to_string(), Some(resources))
            .expect_err("more than the limit must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn prompt_parts_rejects_an_oversized_resource() {
        let big = "a".repeat(MAX_PROMPT_RESOURCE_BYTES + 1);
        let err = prompt_parts("hi".to_string(), Some(vec![resource("rocket://big", &big)]))
            .expect_err("an oversized resource must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        // A resource of exactly the limit is accepted.
        let exact = "a".repeat(MAX_PROMPT_RESOURCE_BYTES);
        prompt_parts(
            "hi".to_string(),
            Some(vec![resource("rocket://ok", &exact)]),
        )
        .expect("a resource at the limit is accepted");
    }

    #[test]
    fn prompt_parts_rejects_an_empty_prompt() {
        let err = prompt_parts("   ".to_string(), None).expect_err("empty must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        // Whitespace text with a resource sends only the resource.
        let parts =
            prompt_parts("  ".to_string(), Some(vec![resource("rocket://a", "x")])).expect("parts");
        assert_eq!(parts.len(), 1);
    }
}
