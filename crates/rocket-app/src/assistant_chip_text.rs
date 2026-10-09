//! Plain-text layouts of the assistant's context chips.
//!
//! Every renderer takes the masked views from `mcp_read_views`, the same views the MCP read
//! tools return, so a credential is masked in one place only. The caller runs a last pass over
//! the whole text with the known secret values and then caps it with `cap_text`.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use rocket_shared::types::{Auth, Body, Header, QueryParam};
use serde_json::Value;

use crate::mcp_read_views::{
    mask_named_value, mask_url, truncate_utf8, MaskedEnvironment, MaskedFolderSettings, MaskedPair,
    MaskedRequest, MaskedSettings, MaskedVariable,
};

/// Largest text one chip adds to a prompt, in UTF-8 bytes, marker included.
pub const CHIP_TEXT_LIMIT_BYTES: usize = 8 * 1024;

/// What a context chip points at. A response chip is built from the frontend's data instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    Request,
    Folder,
    Collection,
    Environment,
}

impl ChipKind {
    /// The name used in the chip's resource URI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Folder => "folder",
            Self::Collection => "collection",
            Self::Environment => "environment",
        }
    }
}

/// A chip as an embedded text resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipResource {
    pub uri: String,
    pub text: String,
}

/// One response header, as the frontend shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseChipHeader {
    pub key: String,
    pub value: String,
}

/// One test result of a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseChipTest {
    pub name: String,
    pub passed: bool,
    pub error: Option<String>,
}

/// The request of the tab that holds the response, as it is on screen. It can differ from the
/// saved request, so its literal credentials are masked too.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseChipRequest {
    pub headers: Vec<Header>,
    pub query_params: Vec<QueryParam>,
    pub body: Option<Body>,
    pub auth: Auth,
}

/// The last response of a request tab. The data lives in the frontend, so it is passed in and
/// masked by `McpToolService::mask_response_chip`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseChipInput {
    pub method: String,
    pub url: String,
    pub status: u16,
    pub status_text: String,
    pub duration_ms: u64,
    pub size_bytes: u64,
    pub headers: Vec<ResponseChipHeader>,
    pub body: String,
    pub is_binary: bool,
    pub tests: Vec<ResponseChipTest>,
    /// The tab's request, when it is known.
    pub request: Option<ResponseChipRequest>,
}

// The characters `encodeURIComponent` leaves alone.
const URI_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

/// The resource URI of a chip, such as `rocket://request/shop/orders/list.yml`.
pub(crate) fn chip_uri(kind: &str, collection: &str, path: Option<&str>) -> String {
    let mut out = format!(
        "rocket://{kind}/{}",
        utf8_percent_encode(collection, URI_SEGMENT)
    );
    if let Some(path) = path.filter(|p| !p.is_empty()) {
        for segment in path.split('/') {
            out.push('/');
            out.push_str(&utf8_percent_encode(segment, URI_SEGMENT).to_string());
        }
    }
    out
}

/// The fence still open at the end of `text`, if a code block was cut short.
fn open_fence(text: &str) -> Option<String> {
    let mut open: Option<String> = None;
    for line in text.lines() {
        let ticks = line.chars().take_while(|c| *c == '`').count();
        match &open {
            None if ticks >= 3 => open = Some("`".repeat(ticks)),
            Some(fence) if ticks >= fence.len() && line.trim_end().len() == ticks => open = None,
            _ => {}
        }
    }
    open
}

/// Cuts `text` to `limit` UTF-8 bytes, marker included, on a character boundary. A code block
/// that the cut leaves open is closed before the marker.
pub(crate) fn cap_text(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let marker = format!("\n[truncated: {} bytes cut to {limit}]", text.len());
    // Room for the newline and a closing fence, which is never longer than a run in the text.
    let longest_run = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
        .max(3);
    let keep = limit.saturating_sub(marker.len() + longest_run + 1);
    let (head, _) = truncate_utf8(text, keep);
    match open_fence(&head) {
        Some(fence) => format!("{head}\n{fence}{marker}"),
        None => format!("{head}{marker}"),
    }
}

/// A code fence longer than any run of backticks in `content`, so the content cannot close it.
pub(crate) fn fence_for(content: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for ch in content.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

fn code_section(title: &str, code: Option<&str>, language: &str) -> Vec<String> {
    match code {
        Some(code) if !code.trim().is_empty() => {
            let fence = fence_for(code);
            vec![
                format!("{title}:"),
                format!("{fence}{language}"),
                code.to_string(),
                fence,
            ]
        }
        _ => Vec::new(),
    }
}

fn pairs_section(title: &str, pairs: &[MaskedPair]) -> Vec<String> {
    let shown: Vec<&MaskedPair> = pairs
        .iter()
        .filter(|pair| pair.enabled && !pair.key.trim().is_empty())
        .collect();
    if shown.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!("{title}:")];
    lines.extend(
        shown
            .iter()
            .map(|pair| format!("  {}: {}", pair.key, pair.value)),
    );
    lines
}

fn variables_section(title: &str, variables: &[MaskedVariable]) -> Vec<String> {
    let shown: Vec<&MaskedVariable> = variables
        .iter()
        .filter(|variable| variable.enabled && !variable.key.trim().is_empty())
        .collect();
    if shown.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!("{title}:")];
    lines.extend(shown.iter().map(|variable| match &variable.value {
        Some(value) if !variable.secret => format!("  {}: {value}", variable.key),
        _ => format!("  {}: (secret, value not shared)", variable.key),
    }));
    lines
}

fn auth_lines(auth_type: &str, auth: &Value) -> Vec<String> {
    match auth_type {
        "none" | "" => Vec::new(),
        "inherit" => vec!["Auth: inherited from the folder or collection".to_string()],
        other => {
            let mut lines = vec![format!("Auth: {other}")];
            if !auth.is_null() {
                lines.push(format!("Auth fields (credentials masked): {auth}"));
            }
            lines
        }
    }
}

/// A request chip. `docs` is the request's documentation text.
pub(crate) fn render_request(collection: &str, view: &MaskedRequest, docs: Option<&str>) -> String {
    let auth_type = view
        .auth
        .get("authType")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let mut lines = vec![
        format!("Request: {}", view.name),
        format!("Collection: {collection}"),
        format!("Path: {}", view.path),
        "Type: http".to_string(),
        format!("{} {}", view.method, view.url),
    ];
    lines.extend(pairs_section("Query parameters", &view.query_params));
    lines.extend(pairs_section("Headers", &view.headers));
    lines.extend(auth_lines(auth_type, &view.auth));
    if let Some(body) = &view.body {
        match body.mode.as_str() {
            "none" | "" => {}
            "binary" => lines.push("Body (binary): (binary file, not shared)".to_string()),
            mode => {
                let form: Vec<String> = body
                    .form
                    .iter()
                    .filter(|pair| pair.enabled && !pair.key.trim().is_empty())
                    .map(|pair| format!("{}={}", pair.key, pair.value))
                    .collect();
                let text = if form.is_empty() {
                    body.content.clone().unwrap_or_default()
                } else {
                    form.join("\n")
                };
                lines.extend(code_section(&format!("Body ({mode})"), Some(&text), ""));
            }
        }
    }
    lines.extend(variables_section("Request variables", &view.variables));
    lines.extend(code_section(
        "Pre-request script",
        view.pre_request_script.as_deref(),
        "javascript",
    ));
    lines.extend(code_section(
        "Post-response script",
        view.post_response_script.as_deref(),
        "javascript",
    ));
    lines.extend(code_section("Tests", view.tests.as_deref(), "javascript"));
    lines.extend(code_section("Docs", docs, "markdown"));
    lines.join("\n")
}

pub(crate) fn render_folder(collection: &str, path: &str, view: &MaskedFolderSettings) -> String {
    let mut lines = vec![
        format!("Folder: {path}"),
        format!("Collection: {collection}"),
    ];
    lines.extend(auth_lines(&view.auth_type, &view.auth));
    lines.extend(pairs_section("Headers", &view.headers));
    lines.extend(variables_section("Variables", &view.variables));
    lines.extend(code_section(
        "Pre-request script",
        view.pre_request_script.as_deref(),
        "javascript",
    ));
    lines.extend(code_section(
        "Post-response script",
        view.post_response_script.as_deref(),
        "javascript",
    ));
    lines.extend(code_section(
        "Tests",
        view.tests_script.as_deref(),
        "javascript",
    ));
    lines.extend(code_section("Docs", view.docs.as_deref(), "markdown"));
    lines.join("\n")
}

pub(crate) fn render_collection(name: &str, view: &MaskedSettings, docs: Option<&str>) -> String {
    let mut lines = vec![format!("Collection: {name}")];
    lines.extend(auth_lines(&view.auth_type, &view.auth));
    lines.extend(pairs_section("Headers", &view.headers));
    lines.extend(variables_section("Variables", &view.variables));
    lines.push(format!(
        "The agent may run requests here: {}",
        if view.run_allowed { "yes" } else { "no" }
    ));
    lines.extend(code_section("Docs", docs, "markdown"));
    lines.join("\n")
}

pub(crate) fn render_environment(collection: &str, view: &MaskedEnvironment) -> String {
    let mut lines = vec![
        format!("Environment: {}", view.name),
        format!("Collection: {collection}"),
    ];
    lines.extend(variables_section("Variables", &view.variables));
    if !view.vault_references.is_empty() {
        lines.push("Vault references (values not shared):".to_string());
        lines.extend(view.vault_references.iter().map(|name| format!("  {name}")));
    }
    lines.join("\n")
}

/// Header names whose value is a URL that can carry a credential.
const URL_HEADERS: &[&str] = &["location", "content-location", "referer", "refresh"];

/// Masks each entry of a `Link` header (`<url>; rel="next", <url>; rel="last"`) on its own.
/// Commas inside `<...>` do not split entries.
fn mask_link_header(value: &str) -> String {
    let mut entries: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_url = false;
    for ch in value.chars() {
        match ch {
            '<' => in_url = true,
            '>' => in_url = false,
            ',' if !in_url => {
                entries.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(ch);
    }
    entries.push(current);
    entries
        .iter()
        .map(|entry| match (entry.find('<'), entry.find('>')) {
            (Some(start), Some(end)) if start < end => format!(
                "{}<{}>{}",
                &entry[..start],
                mask_url(&entry[start + 1..end]),
                &entry[end + 1..]
            ),
            _ => mask_url(entry),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn mask_response_header(header: &ResponseChipHeader) -> MaskedPair {
    let name = header.key.to_ascii_lowercase();
    let value = if name == "link" {
        mask_link_header(&header.value)
    } else if URL_HEADERS.contains(&name.as_str()) {
        mask_url(&header.value)
    } else {
        mask_named_value(&header.key, &header.value)
    };
    MaskedPair {
        key: header.key.clone(),
        value,
        enabled: true,
    }
}

/// A response chip. Headers and the URL are masked here by name and shape. The caller masks
/// the known secret values over the whole text afterwards.
pub(crate) fn render_response(title: &str, input: &ResponseChipInput) -> String {
    let failed: Vec<&ResponseChipTest> = input.tests.iter().filter(|test| !test.passed).collect();
    let headers: Vec<MaskedPair> = input.headers.iter().map(mask_response_header).collect();
    let mut lines = vec![
        format!("Last response of: {title}"),
        format!("{} {}", input.method, mask_url(&input.url)),
        format!("Status: {} {}", input.status, input.status_text),
        format!(
            "Time: {} ms, size: {} bytes",
            input.duration_ms, input.size_bytes
        ),
    ];
    lines.extend(pairs_section("Headers", &headers));
    if !input.tests.is_empty() {
        lines.push(format!(
            "Tests: {} passed, {} failed",
            input.tests.len() - failed.len(),
            failed.len()
        ));
        for test in failed {
            match &test.error {
                Some(error) => lines.push(format!("  failed: {} ({error})", test.name)),
                None => lines.push(format!("  failed: {}", test.name)),
            }
        }
    }
    if input.is_binary {
        lines.push("Body: (binary body, not shared)".to_string());
    } else {
        lines.extend(code_section("Body", Some(&input.body), ""));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_uri_encodes_each_segment() {
        assert_eq!(
            chip_uri("request", "my shop", Some("a b/list.yml")),
            "rocket://request/my%20shop/a%20b/list.yml"
        );
        assert_eq!(
            chip_uri("collection", "shop", None),
            "rocket://collection/shop"
        );
    }

    #[test]
    fn cap_text_keeps_short_text_and_cuts_on_a_char_boundary() {
        assert_eq!(cap_text("hello", 100), "hello");
        let capped = cap_text(&"é".repeat(5_000), CHIP_TEXT_LIMIT_BYTES);
        assert!(capped.len() <= CHIP_TEXT_LIMIT_BYTES);
        assert!(capped.contains("[truncated: 10000 bytes cut to 8192]"));
    }

    #[test]
    fn fence_is_longer_than_any_backtick_run() {
        assert_eq!(fence_for("plain"), "```");
        assert_eq!(fence_for("a ```` b"), "`````");
    }

    #[test]
    fn code_cannot_close_its_own_fence() {
        let lines = code_section("Tests", Some("x\n```\ninjected"), "javascript");
        assert_eq!(lines[1], "````javascript");
        assert_eq!(lines[2], "x\n```\ninjected");
        assert_eq!(lines[3], "````");
    }

    #[test]
    fn cap_text_closes_a_fence_it_leaves_open() {
        let text = format!(
            "head\n{}",
            code_section("Body", Some(&"x\n".repeat(6_000)), "").join("\n")
        );
        let capped = cap_text(&text, 1_000);
        assert!(capped.len() <= 1_000);
        assert!(capped.contains("\n```\n[truncated:"), "{capped}");
        assert_eq!(open_fence(&capped), None);
    }

    #[test]
    fn link_headers_are_masked_entry_by_entry() {
        let masked = mask_link_header(
            "<https://a.test/p?page=2&access_token=tok-1>; rel=\"next\", <https://b.test/x?a=1,2&sig=s-2>; rel=\"last\"",
        );
        assert!(!masked.contains("tok-1"));
        assert!(!masked.contains("s-2"));
        assert!(masked.contains("rel=\"next\""));
        assert!(masked.contains("rel=\"last\""));
        assert!(masked.contains("page=2"));
    }
}
