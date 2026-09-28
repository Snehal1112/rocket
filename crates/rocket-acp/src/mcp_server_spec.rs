/// A single MCP server the ACP agent should be told about for one session.
/// Rocket-owned type — deliberately NOT `agent_client_protocol::McpServer`,
/// since `rocket-acp` must not depend on that crate (existing DDD boundary).
/// `rocket-infra`'s `AcpAgentClient` maps this to the real
/// `agent_client_protocol::McpServer` type when it builds `NewSessionRequest`
/// (Plan 02).
#[derive(Clone)]
pub enum McpServerSpec {
    Http {
        name: String,
        url: String,
        token: String,
    },
    Stdio {
        name: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
}

/// Manual `Debug` impl. Redacts the `Http` bearer token and every `Stdio`
/// env var value, since either may carry a secret. Env var names are kept
/// as-is because they are useful in logs and are not themselves secret.
impl std::fmt::Debug for McpServerSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpServerSpec::Http { name, url, .. } => f
                .debug_struct("Http")
                .field("name", name)
                .field("url", url)
                .field("token", &"[REDACTED]")
                .finish(),
            McpServerSpec::Stdio {
                name,
                command,
                args,
                env,
            } => {
                let redacted_env: Vec<(&str, &str)> = env
                    .iter()
                    .map(|(k, _)| (k.as_str(), "[REDACTED]"))
                    .collect();
                f.debug_struct("Stdio")
                    .field("name", name)
                    .field("command", command)
                    .field("args", args)
                    .field("env", &redacted_env)
                    .finish()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_variant_holds_its_fields() {
        let spec = McpServerSpec::Http {
            name: "rocket-mcp".to_string(),
            url: "http://127.0.0.1:4000/mcp".to_string(),
            token: "tok-abc".to_string(),
        };
        match spec {
            McpServerSpec::Http { name, url, token } => {
                assert_eq!(name, "rocket-mcp");
                assert_eq!(url, "http://127.0.0.1:4000/mcp");
                assert_eq!(token, "tok-abc");
            }
            McpServerSpec::Stdio { .. } => panic!("expected Http variant"),
        }
    }

    #[test]
    fn stdio_variant_holds_its_fields() {
        let spec = McpServerSpec::Stdio {
            name: "rocket-mcp-stdio".to_string(),
            command: "/usr/bin/rocket".to_string(),
            args: vec!["--acp-mcp-stdio-bridge".to_string()],
            env: vec![("ROCKET_MCP_PORT".to_string(), "4000".to_string())],
        };
        match spec {
            McpServerSpec::Stdio {
                name,
                command,
                args,
                env,
            } => {
                assert_eq!(name, "rocket-mcp-stdio");
                assert_eq!(command, "/usr/bin/rocket");
                assert_eq!(args, vec!["--acp-mcp-stdio-bridge".to_string()]);
                assert_eq!(
                    env,
                    vec![("ROCKET_MCP_PORT".to_string(), "4000".to_string())]
                );
            }
            McpServerSpec::Http { .. } => panic!("expected Stdio variant"),
        }
    }

    #[test]
    fn is_clonable_and_debug_formattable() {
        let spec = McpServerSpec::Http {
            name: "a".into(),
            url: "b".into(),
            token: "c".into(),
        };
        let cloned = spec.clone();
        let _ = format!("{cloned:?}");
    }

    #[test]
    fn http_debug_redacts_token() {
        let spec = McpServerSpec::Http {
            name: "rocket-mcp".to_string(),
            url: "http://127.0.0.1:4000/mcp".to_string(),
            token: "tok-super-secret".to_string(),
        };
        let debug_output = format!("{spec:?}");
        assert!(
            !debug_output.contains("tok-super-secret"),
            "debug output must not contain the raw token: {debug_output}"
        );
        assert!(debug_output.contains("rocket-mcp"));
        assert!(debug_output.contains("http://127.0.0.1:4000/mcp"));
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn stdio_debug_redacts_env_values_but_keeps_keys() {
        let spec = McpServerSpec::Stdio {
            name: "rocket-mcp-stdio".to_string(),
            command: "/usr/bin/rocket".to_string(),
            args: vec!["--acp-mcp-stdio-bridge".to_string()],
            env: vec![
                (
                    "ROCKET_MCP_TOKEN".to_string(),
                    "env-super-secret".to_string(),
                ),
                ("ROCKET_MCP_PORT".to_string(), "4000".to_string()),
            ],
        };
        let debug_output = format!("{spec:?}");
        assert!(
            !debug_output.contains("env-super-secret"),
            "debug output must not contain the raw env secret value: {debug_output}"
        );
        assert!(debug_output.contains("rocket-mcp-stdio"));
        assert!(debug_output.contains("/usr/bin/rocket"));
        assert!(debug_output.contains("ROCKET_MCP_TOKEN"));
        assert!(debug_output.contains("[REDACTED]"));
    }
}
