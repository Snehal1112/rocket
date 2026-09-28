/// A single MCP server the ACP agent should be told about for one session.
/// Rocket-owned type — deliberately NOT `agent_client_protocol::McpServer`,
/// since `rocket-acp` must not depend on that crate (existing DDD boundary).
/// `rocket-infra`'s `AcpAgentClient` maps this to the real
/// `agent_client_protocol::McpServer` type when it builds `NewSessionRequest`
/// (Plan 02).
#[derive(Debug, Clone)]
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
}
