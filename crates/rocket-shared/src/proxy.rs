use serde::{Deserialize, Serialize};

/// Proxy authentication credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyAuth {
    pub username: String,
    pub password: String,
}

/// Proxy connection configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConnectionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Proxy auth: None serializes as `false`, Some(auth) serializes as the auth object.
    #[serde(
        default,
        deserialize_with = "deserialize_proxy_auth",
        serialize_with = "serialize_proxy_auth"
    )]
    pub auth: Option<ProxyAuth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bypass_proxy: Option<String>,
}

/// Proxy settings for a request or collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proxy {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub inherit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ProxyConnectionConfig>,
}

// Custom serializer: None -> false, Some(auth) -> auth object.
fn serialize_proxy_auth<S: serde::Serializer>(
    auth: &Option<ProxyAuth>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match auth {
        None => serializer.serialize_bool(false),
        Some(a) => a.serialize(serializer),
    }
}

// Custom deserializer: false -> None, object -> Some(ProxyAuth).
fn deserialize_proxy_auth<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ProxyAuth>, D::Error> {
    use serde::de;

    struct ProxyAuthVisitor;

    impl<'de> de::Visitor<'de> for ProxyAuthVisitor {
        type Value = Option<ProxyAuth>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "false or a ProxyAuth object")
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<Option<ProxyAuth>, E> {
            if v {
                Err(de::Error::custom(
                    "proxy auth cannot be true, must be false or an object",
                ))
            } else {
                Ok(None)
            }
        }

        fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Option<ProxyAuth>, A::Error> {
            let auth = ProxyAuth::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(Some(auth))
        }
    }

    deserializer.deserialize_any(ProxyAuthVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_auth_serde() {
        let auth = ProxyAuth {
            username: "user".into(),
            password: "pass".into(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        let back: ProxyAuth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, back);
    }

    #[test]
    fn proxy_config_auth_false() {
        let config = ProxyConnectionConfig {
            protocol: Some("http".into()),
            hostname: Some("proxy.example.com".into()),
            port: Some(8080),
            auth: None,
            bypass_proxy: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"auth\":false"));
        let back: ProxyConnectionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn proxy_config_auth_object() {
        let config = ProxyConnectionConfig {
            protocol: Some("https".into()),
            hostname: Some("proxy.example.com".into()),
            port: Some(3128),
            auth: Some(ProxyAuth {
                username: "admin".into(),
                password: "secret".into(),
            }),
            bypass_proxy: Some("localhost,127.0.0.1".into()),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"username\":\"admin\""));
        let back: ProxyConnectionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn proxy_full_roundtrip() {
        let proxy = Proxy {
            enabled: true,
            inherit: false,
            config: Some(ProxyConnectionConfig {
                protocol: Some("http".into()),
                hostname: Some("proxy.local".into()),
                port: Some(8080),
                auth: None,
                bypass_proxy: None,
            }),
        };
        let json = serde_json::to_string(&proxy).unwrap();
        let back: Proxy = serde_json::from_str(&json).unwrap();
        assert_eq!(proxy, back);
    }

    #[test]
    fn proxy_minimal() {
        let proxy = Proxy {
            enabled: false,
            inherit: true,
            config: None,
        };
        let json = serde_json::to_string(&proxy).unwrap();
        let back: Proxy = serde_json::from_str(&json).unwrap();
        assert_eq!(proxy, back);
    }
}
