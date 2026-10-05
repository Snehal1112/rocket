//! AWS credentials for SigV4: either typed into the request, or read from a named profile of
//! the shared credentials file (`~/.aws/credentials`, or `AWS_SHARED_CREDENTIALS_FILE`).
//! Keys and tokens never appear in an error message.

use std::path::{Path, PathBuf};

use rocket_http::aws_sig::AwsCredentials;
use rocket_shared::error::{DomainError, DomainResult};

/// The three values a profile can supply.
#[derive(Clone, PartialEq, Eq)]
pub struct AwsProfileCredentials {
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
}

/// Prints the access key id only. The secret key and the token are redacted.
impl std::fmt::Debug for AwsProfileCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsProfileCredentials")
            .field("access_key", &self.access_key)
            .field("secret_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Reads `profile` from the text of a credentials file. Both `[name]` and the config-file style
/// `[profile name]` headers are accepted. A profile without both an access key and a secret key
/// is treated as absent.
pub fn parse_credentials(text: &str, profile: &str) -> Option<AwsProfileCredentials> {
    let mut in_profile = false;
    let mut access = None;
    let mut secret = None;
    let mut token = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = header.trim();
            let name = name.strip_prefix("profile ").map_or(name, str::trim);
            in_profile = name == profile;
            continue;
        }
        if !in_profile {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "aws_access_key_id" => access = Some(value.to_string()),
            "aws_secret_access_key" => secret = Some(value.to_string()),
            "aws_session_token" if !value.is_empty() => token = Some(value.to_string()),
            _ => {}
        }
    }
    match (access, secret) {
        (Some(a), Some(s)) if !a.is_empty() && !s.is_empty() => Some(AwsProfileCredentials {
            access_key: a,
            secret_key: s,
            session_token: token,
        }),
        _ => None,
    }
}

/// Path of the shared credentials file, or `None` when no home directory is known.
pub fn credentials_file_path() -> Option<PathBuf> {
    std::env::var_os("AWS_SHARED_CREDENTIALS_FILE")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".aws").join("credentials")))
}

/// Reads `profile` from the credentials file at `path`. Errors name the file and the profile,
/// never a key.
pub fn load_profile_from(path: &Path, profile: &str) -> DomainResult<AwsProfileCredentials> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        DomainError::InvalidInput(format!(
            "Cannot read the AWS credentials file {}: {e}",
            path.display()
        ))
    })?;
    parse_credentials(&text, profile).ok_or_else(|| {
        DomainError::InvalidInput(format!(
            "AWS profile {profile} was not found in {} or has no access key and secret key",
            path.display()
        ))
    })
}

/// Reads `profile` from the shared credentials file.
pub fn load_profile(profile: &str) -> DomainResult<AwsProfileCredentials> {
    let path = credentials_file_path().ok_or_else(|| {
        DomainError::InvalidInput(
            "Cannot find the AWS credentials file: no home directory is known".into(),
        )
    })?;
    load_profile_from(&path, profile)
}

/// The credentials to sign with. Keys typed into the request win. With no keys typed and a
/// profile name set, the profile is read from the credentials file. Anything else is an error:
/// signing with empty keys can never succeed, so it must not be attempted.
pub fn resolve_credentials(
    access_key: &str,
    secret_key: &str,
    region: &str,
    service: &str,
    session_token: Option<&str>,
    profile_name: Option<&str>,
) -> DomainResult<AwsCredentials> {
    let profile = profile_name.map(str::trim).filter(|p| !p.is_empty());
    let inline = !access_key.trim().is_empty() || !secret_key.trim().is_empty();
    let (access_key, secret_key, session_token) = match profile {
        Some(name) if !inline => {
            let c = load_profile(name)?;
            (c.access_key, c.secret_key, c.session_token)
        }
        _ => (
            access_key.trim().to_string(),
            secret_key.trim().to_string(),
            session_token
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string),
        ),
    };
    if access_key.is_empty() {
        return Err(DomainError::InvalidInput(
            "AWS Signature V4 needs an access key, or a profile name".into(),
        ));
    }
    if secret_key.is_empty() {
        return Err(DomainError::InvalidInput(
            "AWS Signature V4 needs a secret key, or a profile name".into(),
        ));
    }
    Ok(AwsCredentials {
        access_key,
        secret_key,
        region: region.trim().to_string(),
        service: service.trim().to_string(),
        session_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
[default]
aws_access_key_id = AKIADEFAULT
aws_secret_access_key = defsecret

# a comment
; another comment
[prod]
aws_access_key_id=AKIAPROD
aws_secret_access_key = prodsecret
aws_session_token = tok

[nokeys]
region = eu-west-1
";

    #[test]
    fn reads_a_named_profile() {
        let c = parse_credentials(FILE, "prod").expect("prod");
        assert_eq!(c.access_key, "AKIAPROD");
        assert_eq!(c.secret_key, "prodsecret");
        assert_eq!(c.session_token.as_deref(), Some("tok"));
        let d = parse_credentials(FILE, "default").expect("default");
        assert_eq!(d.session_token, None);
    }

    #[test]
    fn a_missing_profile_or_one_without_keys_is_none() {
        assert!(parse_credentials(FILE, "staging").is_none());
        assert!(parse_credentials(FILE, "nokeys").is_none());
    }

    #[test]
    fn accepts_the_config_file_style_profile_header() {
        let text = "[profile dev]\naws_access_key_id = A\naws_secret_access_key = S\n";
        assert!(parse_credentials(text, "dev").is_some());
    }

    #[test]
    fn load_profile_from_names_the_profile_but_never_the_keys_in_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("credentials");
        std::fs::write(&path, FILE).expect("write");
        let ok = load_profile_from(&path, "prod").expect("prod");
        assert_eq!(ok.access_key, "AKIAPROD");
        let err = load_profile_from(&path, "staging").expect_err("missing profile");
        let text = err.to_string();
        assert!(text.contains("staging"), "{text}");
        assert!(
            !text.contains("prodsecret") && !text.contains("defsecret"),
            "{text}"
        );
        let gone = load_profile_from(&dir.path().join("nope"), "prod").expect_err("no file");
        assert!(gone.to_string().contains("credentials file"), "{gone}");
    }

    #[test]
    fn inline_keys_win_over_a_profile_name() {
        let c = resolve_credentials("AK", "SK", "us-east-1", "s3", Some(""), Some("prod"))
            .expect("inline");
        assert_eq!(c.access_key, "AK");
        assert_eq!(c.session_token, None, "an empty token is no token");
    }

    #[test]
    fn missing_credentials_are_an_error() {
        let err = resolve_credentials("", "", "us-east-1", "s3", None, None).expect_err("no creds");
        assert!(err.to_string().contains("access key"), "{err}");
        let half = resolve_credentials("AK", "", "us-east-1", "s3", None, None).expect_err("half");
        assert!(half.to_string().contains("secret key"), "{half}");
    }

    #[test]
    fn debug_output_never_prints_keys_or_tokens() {
        let c = parse_credentials(FILE, "prod").expect("prod");
        let text = format!("{c:?}");
        assert!(
            !text.contains("prodsecret") && !text.contains("tok\""),
            "{text}"
        );
        let r = resolve_credentials("AK", "SK", "us-east-1", "s3", Some("TOKEN"), None)
            .expect("inline");
        let text = format!("{r:?}");
        assert!(!text.contains("SK\"") && !text.contains("TOKEN"), "{text}");
    }
}
