use crate::oc::*;
use rocket_shared::oauth2::{
    OAuth2AdditionalParameters, OAuth2ClientCredentials, OAuth2Flow, OAuth2PKCE,
    OAuth2ResourceOwner, OAuth2Settings, OAuth2TokenConfig,
};
use rocket_shared::types::Auth;

impl From<OcAuth> for Auth {
    fn from(oc: OcAuth) -> Self {
        match oc {
            OcAuth::Inherit(ref s) if s == "inherit" => Auth::Inherit,
            OcAuth::Inherit(_) => Auth::None,
            OcAuth::Typed(typed) => typed.into(),
        }
    }
}

impl From<OcAuthTyped> for Auth {
    fn from(oc: OcAuthTyped) -> Self {
        match oc {
            OcAuthTyped::None => Auth::None,
            OcAuthTyped::Basic { username, password } => Auth::Basic { username, password },
            OcAuthTyped::Bearer { token } => Auth::Bearer { token },
            OcAuthTyped::ApiKey { key, value, placement } => Auth::ApiKey {
                key,
                value,
                placement: placement.unwrap_or_else(|| "header".into()),
            },
            OcAuthTyped::Digest { username, password } => Auth::Digest { username, password },
            OcAuthTyped::Ntlm { username, password, domain } => {
                Auth::Ntlm { username, password, domain }
            }
            OcAuthTyped::Wsse { username, password } => Auth::Wsse { username, password },
            OcAuthTyped::AwsV4 {
                access_key_id,
                secret_access_key,
                region,
                service,
                session_token,
                profile_name,
            } => Auth::AwsSigV4 {
                access_key: access_key_id,
                secret_key: secret_access_key,
                region: region.unwrap_or_default(),
                service: service.unwrap_or_default(),
                session_token,
                profile_name,
            },
            OcAuthTyped::OAuth2 {
                flow,
                access_token_url,
                refresh_token_url,
                authorization_url,
                callback_url,
                credentials,
                resource_owner,
                scope,
                state,
                pkce,
                additional_parameters,
                token_config,
                settings,
            } => {
                let creds = credentials.map(oc_creds_to_domain).unwrap_or_else(|| {
                    OAuth2ClientCredentials {
                        client_id: String::new(),
                        client_secret: String::new(),
                        placement: None,
                    }
                });
                let oauth_flow = match flow.as_str() {
                    "client_credentials" => OAuth2Flow::ClientCredentials {
                        access_token_url: access_token_url.unwrap_or_default(),
                        refresh_token_url,
                        credentials: creds,
                        scope,
                        additional_parameters,
                        token_config,
                        settings,
                    },
                    "resource_owner_password_credentials" => {
                        OAuth2Flow::ResourceOwnerPassword {
                            access_token_url: access_token_url.unwrap_or_default(),
                            refresh_token_url,
                            credentials: creds,
                            resource_owner: resource_owner.map(oc_ro_to_domain),
                            scope,
                            additional_parameters,
                            token_config,
                            settings,
                        }
                    }
                    "authorization_code" => OAuth2Flow::AuthorizationCode {
                        authorization_url: authorization_url.unwrap_or_default(),
                        access_token_url: access_token_url.unwrap_or_default(),
                        refresh_token_url,
                        callback_url,
                        credentials: creds,
                        scope,
                        state,
                        pkce: pkce.map(oc_pkce_to_domain),
                        additional_parameters,
                        token_config,
                        settings,
                    },
                    "implicit" | _ => OAuth2Flow::Implicit {
                        authorization_url: authorization_url.unwrap_or_default(),
                        callback_url,
                        client_id: creds.client_id,
                        scope,
                        state,
                        additional_parameters,
                        token_config,
                        settings,
                    },
                };
                Auth::OAuth2(oauth_flow)
            }
        }
    }
}

fn oc_creds_to_domain(c: OcOAuth2Credentials) -> OAuth2ClientCredentials {
    OAuth2ClientCredentials {
        client_id: c.client_id,
        client_secret: c.client_secret,
        placement: c.placement,
    }
}

fn oc_ro_to_domain(r: OcOAuth2ResourceOwner) -> OAuth2ResourceOwner {
    OAuth2ResourceOwner {
        username: r.username,
        password: r.password,
    }
}

fn oc_pkce_to_domain(p: OcOAuth2PKCE) -> OAuth2PKCE {
    OAuth2PKCE {
        enabled: p.enabled,
        method: p.method,
    }
}

impl From<Auth> for OcAuth {
    fn from(auth: Auth) -> Self {
        match auth {
            Auth::Inherit => OcAuth::Inherit("inherit".into()),
            Auth::None => OcAuth::Typed(OcAuthTyped::None),
            Auth::Basic { username, password } => {
                OcAuth::Typed(OcAuthTyped::Basic { username, password })
            }
            Auth::Bearer { token } => OcAuth::Typed(OcAuthTyped::Bearer { token }),
            Auth::ApiKey { key, value, placement } => OcAuth::Typed(OcAuthTyped::ApiKey {
                key,
                value,
                placement: Some(placement),
            }),
            Auth::Digest { username, password } => {
                OcAuth::Typed(OcAuthTyped::Digest { username, password })
            }
            Auth::Ntlm { username, password, domain } => {
                OcAuth::Typed(OcAuthTyped::Ntlm { username, password, domain })
            }
            Auth::Wsse { username, password } => {
                OcAuth::Typed(OcAuthTyped::Wsse { username, password })
            }
            Auth::AwsSigV4 {
                access_key,
                secret_key,
                region,
                service,
                session_token,
                profile_name,
            } => OcAuth::Typed(OcAuthTyped::AwsV4 {
                access_key_id: access_key,
                secret_access_key: secret_key,
                region: if region.is_empty() { None } else { Some(region) },
                service: if service.is_empty() { None } else { Some(service) },
                session_token,
                profile_name,
            }),
            Auth::OAuth2(flow) => {
                let (
                    flow_str,
                    access_token_url,
                    refresh_token_url,
                    authorization_url,
                    callback_url,
                    credentials,
                    resource_owner,
                    scope,
                    state,
                    pkce,
                    additional_parameters,
                    token_config,
                    settings,
                ) = domain_oauth2_to_oc_fields(flow);
                OcAuth::Typed(OcAuthTyped::OAuth2 {
                    flow: flow_str,
                    access_token_url,
                    refresh_token_url,
                    authorization_url,
                    callback_url,
                    credentials,
                    resource_owner,
                    scope,
                    state,
                    pkce,
                    additional_parameters,
                    token_config,
                    settings,
                })
            }
        }
    }
}

/// Extract all OC OAuth2 fields from a domain OAuth2Flow variant.
#[allow(clippy::type_complexity)]
fn domain_oauth2_to_oc_fields(
    flow: OAuth2Flow,
) -> (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<OcOAuth2Credentials>,
    Option<OcOAuth2ResourceOwner>,
    Option<String>,
    Option<String>,
    Option<OcOAuth2PKCE>,
    Option<OAuth2AdditionalParameters>,
    Option<OAuth2TokenConfig>,
    Option<OAuth2Settings>,
) {
    match flow {
        OAuth2Flow::ClientCredentials {
            access_token_url,
            refresh_token_url,
            credentials,
            scope,
            additional_parameters,
            token_config,
            settings,
        } => (
            "client_credentials".into(),
            Some(access_token_url),
            refresh_token_url,
            None,
            None,
            Some(domain_creds_to_oc(credentials)),
            None,
            scope,
            None,
            None,
            additional_parameters,
            token_config,
            settings,
        ),
        OAuth2Flow::ResourceOwnerPassword {
            access_token_url,
            refresh_token_url,
            credentials,
            resource_owner,
            scope,
            additional_parameters,
            token_config,
            settings,
        } => (
            "resource_owner_password_credentials".into(),
            Some(access_token_url),
            refresh_token_url,
            None,
            None,
            Some(domain_creds_to_oc(credentials)),
            resource_owner.map(domain_ro_to_oc),
            scope,
            None,
            None,
            additional_parameters,
            token_config,
            settings,
        ),
        OAuth2Flow::AuthorizationCode {
            authorization_url,
            access_token_url,
            refresh_token_url,
            callback_url,
            credentials,
            scope,
            state,
            pkce,
            additional_parameters,
            token_config,
            settings,
        } => (
            "authorization_code".into(),
            Some(access_token_url),
            refresh_token_url,
            Some(authorization_url),
            callback_url,
            Some(domain_creds_to_oc(credentials)),
            None,
            scope,
            state,
            pkce.map(domain_pkce_to_oc),
            additional_parameters,
            token_config,
            settings,
        ),
        OAuth2Flow::Implicit {
            authorization_url,
            callback_url,
            client_id,
            scope,
            state,
            additional_parameters,
            token_config,
            settings,
        } => (
            "implicit".into(),
            None,
            None,
            Some(authorization_url),
            callback_url,
            Some(OcOAuth2Credentials {
                client_id,
                client_secret: String::new(),
                placement: None,
            }),
            None,
            scope,
            state,
            None,
            additional_parameters,
            token_config,
            settings,
        ),
    }
}

fn domain_creds_to_oc(c: OAuth2ClientCredentials) -> OcOAuth2Credentials {
    OcOAuth2Credentials {
        client_id: c.client_id,
        client_secret: c.client_secret,
        placement: c.placement,
    }
}

fn domain_ro_to_oc(r: OAuth2ResourceOwner) -> OcOAuth2ResourceOwner {
    OcOAuth2ResourceOwner {
        username: r.username,
        password: r.password,
    }
}

fn domain_pkce_to_oc(p: OAuth2PKCE) -> OcOAuth2PKCE {
    OcOAuth2PKCE {
        enabled: p.enabled,
        method: p.method,
    }
}
