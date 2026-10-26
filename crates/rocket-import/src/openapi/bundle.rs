//! Inlines the files a spec refers to by URL into one self-contained text.
//!
//! The rest of the import pipeline reads one text, so each referenced file is copied under the
//! root key `x-rocket-remote` and every `$ref` is rewritten to a local pointer. Nothing here
//! reads a local path, and only `https` files without a query or user name are fetched.

use crate::error::{ImportError, ImportResult};
use crate::openapi::entry::fetch_failure_message;
use crate::openapi::{fetch_target, parse_spec_url};
pub use rocket_collection::openapi_source::RemoteRefState;
use rocket_collection::openapi_source::SpecLocation;
use rocket_http::spec_fetch::{
    redirect_target_allowed, SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest, SpecFetcher,
    DEFAULT_MAX_SPEC_BYTES,
};
use serde_yaml::Value as Yaml;
use std::collections::{HashMap, HashSet};
use url::Url;

/// Root key that holds the inlined files.
const ROOT_KEY: &str = "x-rocket-remote";
/// Most referenced files fetched for one spec.
const MAX_FILES: usize = 20;
/// Deepest nesting of files that refer to files.
const MAX_DEPTH: usize = 3;
/// Most bytes for the main spec plus all files.
const MAX_TOTAL_BYTES: usize = DEFAULT_MAX_SPEC_BYTES;

const SIZE_NOTE: &str = "The referenced files together are larger than the 25 MB limit.";

/// The result of bundling a spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// The main text unchanged when nothing was inlined, else the combined document.
    pub text: String,
    /// The validators of every file that was fetched, in the order they were inlined.
    pub refs: Vec<RemoteRefState>,
    /// Plain sentences, one per skipped reference, failed file or limit. No query strings.
    pub notes: Vec<String>,
    /// True when a file could not be fetched or parsed, or a limit was hit.
    pub incomplete: bool,
}

/// What one `$ref` string points at.
enum Target {
    /// A pointer into the document that holds it. `fragment` is the part after `#`.
    Local { fragment: String },
    /// A file at an absolute address without fragment.
    Remote { url: String, fragment: String },
    /// A reference that is not followed. Holds the note for it.
    Skip(String),
}

fn label_of(url: &Url) -> String {
    url.path_segments()
        .and_then(|segments| segments.filter(|s| !s.is_empty()).next_back())
        .map(str::to_string)
        .or_else(|| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "a file".to_string())
}

fn skip(url: &Url, reason: &str) -> Target {
    Target::Skip(format!(
        "A reference to {} was not followed: {reason}.",
        label_of(url)
    ))
}

fn invalid() -> Target {
    Target::Skip("A reference was not followed: it is not a valid address.".to_string())
}

fn classify(reference: &str, base: Option<&Url>) -> Target {
    let (head, fragment) = reference.split_once('#').unwrap_or((reference, ""));
    let fragment = fragment.to_string();
    if head.is_empty() {
        return Target::Local { fragment };
    }
    let mut url = match Url::parse(head) {
        Ok(url) => url,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let Some(base) = base else {
                return Target::Skip(
                    "A relative reference was not followed: the spec has no address.".to_string(),
                );
            };
            match base.join(head) {
                Ok(url) => url,
                Err(_) => return invalid(),
            }
        }
        Err(_) => return invalid(),
    };
    if url.scheme() != "https" {
        return skip(&url, "only https links are followed");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return skip(&url, "links with a user name or password are not followed");
    }
    if url.query().is_some() {
        return skip(&url, "links with a query string are not followed");
    }
    if !redirect_target_allowed(url.as_str()) {
        return skip(&url, "the address is not allowed");
    }
    if !fragment.is_empty() && !fragment.starts_with('/') {
        return skip(&url, "only pointers that start with / are followed");
    }
    url.set_fragment(None);
    Target::Remote {
        url: url.to_string(),
        fragment,
    }
}

/// The URLs and notes found in one document, each once and in first-encounter order.
#[derive(Default)]
struct Scan {
    urls: Vec<String>,
    notes: Vec<String>,
}

impl Scan {
    fn add(&mut self, target: Target) {
        match target {
            Target::Remote { url, .. } => {
                if !self.urls.contains(&url) {
                    self.urls.push(url);
                }
            }
            Target::Skip(note) => push_unique(&mut self.notes, note),
            Target::Local { .. } => {}
        }
    }
}

fn push_unique(list: &mut Vec<String>, note: String) {
    if !list.contains(&note) {
        list.push(note);
    }
}

fn collect(value: &Yaml, base: Option<&Url>, scan: &mut Scan) {
    match value {
        Yaml::Mapping(map) => {
            for (key, child) in map {
                match (key.as_str(), child.as_str()) {
                    (Some("$ref"), Some(reference)) => scan.add(classify(reference, base)),
                    _ => collect(child, base, scan),
                }
            }
        }
        Yaml::Sequence(items) => {
            for item in items {
                collect(item, base, scan);
            }
        }
        _ => {}
    }
}

fn parse_yaml_mapping(text: &str) -> Option<Yaml> {
    serde_yaml::from_str::<Yaml>(text)
        .ok()
        .filter(|v| v.is_mapping())
}

/// The absolute, fragment-free, deduplicated URLs the main text refers to, in first-encounter
/// order. Skipped references are not listed. A relative reference needs `base`.
pub fn find_remote_refs(main_text: &str, base: Option<&str>) -> Vec<String> {
    let Some(root) = parse_yaml_mapping(main_text) else {
        return Vec::new();
    };
    let base = base.and_then(|b| Url::parse(b).ok());
    let mut scan = Scan::default();
    collect(&root, base.as_ref(), &mut scan);
    scan.urls
}

fn pointer_into(index: usize, fragment: &str) -> String {
    format!("#/{ROOT_KEY}/{index}{fragment}")
}

fn rewrite(
    value: &mut Yaml,
    base: Option<&Url>,
    owner: Option<usize>,
    index: &HashMap<String, usize>,
) {
    match value {
        Yaml::Mapping(map) => {
            for (key, child) in map.iter_mut() {
                if key.as_str() == Some("$ref") {
                    if let Some(reference) = child.as_str() {
                        let replacement = match classify(reference, base) {
                            Target::Local { fragment }
                                if fragment.is_empty() || fragment.starts_with('/') =>
                            {
                                owner.map(|i| pointer_into(i, &fragment))
                            }
                            Target::Remote { url, fragment } => {
                                index.get(&url).map(|&i| pointer_into(i, &fragment))
                            }
                            _ => None,
                        };
                        if let Some(replacement) = replacement {
                            *child = Yaml::String(replacement);
                        }
                        continue;
                    }
                }
                rewrite(child, base, owner, index);
            }
        }
        Yaml::Sequence(items) => {
            for item in items {
                rewrite(item, base, owner, index);
            }
        }
        _ => {}
    }
}

fn canonical(url: &str) -> String {
    match Url::parse(url) {
        Ok(mut parsed) => {
            parsed.set_fragment(None);
            parsed.to_string()
        }
        Err(_) => url.to_string(),
    }
}

fn inline_with_base(main: &mut Yaml, main_base: Option<&Url>, files: &[(String, Yaml)]) {
    if files.is_empty() || !main.is_mapping() {
        return;
    }
    let mut index = HashMap::new();
    for (i, (url, _)) in files.iter().enumerate() {
        index.entry(canonical(url)).or_insert(i);
    }
    let mut root = serde_yaml::Mapping::new();
    for (i, (url, value)) in files.iter().enumerate() {
        let mut copy = value.clone();
        let base = Url::parse(url).ok();
        rewrite(&mut copy, base.as_ref(), Some(i), &index);
        root.insert(Yaml::String(i.to_string()), copy);
    }
    // The main document is rewritten before the files are attached to it.
    rewrite(main, main_base, None, &index);
    if let Yaml::Mapping(map) = main {
        map.insert(Yaml::String(ROOT_KEY.to_string()), Yaml::Mapping(root));
    }
}

/// Copies each file under `x-rocket-remote.<i>` (by position in `files`) and rewrites the
/// `$ref` values that point at them. The URLs in `files` are absolute. Relative references in
/// the main document need an address, so use `bundle_spec` for those.
pub fn inline_refs(main: &mut Yaml, files: &[(String, Yaml)]) {
    inline_with_base(main, None, files);
}

/// The address relative references in the main spec resolve against.
fn raw_address(location: &SpecLocation) -> String {
    fetch_target(location, false).url
}

/// How a referenced file is requested. Only a file in the main spec's own GitHub repository
/// gets the token; every other file is a plain request.
fn request_for(
    file_url: &str,
    location: &SpecLocation,
    token: Option<&str>,
    max_bytes: usize,
) -> SpecFetchRequest {
    let same_repo = match (location, parse_spec_url(file_url)) {
        (SpecLocation::Github { owner, repo, .. }, Ok(file_location)) => {
            let same = matches!(
                &file_location,
                SpecLocation::Github { owner: o, repo: r, .. }
                    if o.eq_ignore_ascii_case(owner) && r.eq_ignore_ascii_case(repo)
            );
            same.then_some(file_location)
        }
        _ => None,
    };
    let (url, accept, bearer_token) = match same_repo {
        Some(file_location) => {
            let target = fetch_target(&file_location, token.is_some());
            (target.url, target.accept, token.map(str::to_string))
        }
        None => (file_url.to_string(), None, None),
    };
    SpecFetchRequest {
        url,
        accept,
        bearer_token,
        if_none_match: None,
        if_modified_since: None,
        max_bytes,
    }
}

fn file_label(url: &str) -> String {
    Url::parse(url)
        .map(|u| label_of(&u))
        .unwrap_or_else(|_| "a file".to_string())
}

fn failure_text(failure: &SpecFetchFailure, token_sent: bool) -> String {
    match failure {
        // The message of a network error may hold a redirect address.
        SpecFetchFailure::Network(_) => "The file could not be fetched.".to_string(),
        other => fetch_failure_message(other, token_sent),
    }
}

struct Queued {
    url: String,
    depth: usize,
}

/// Which limit notes were already given.
#[derive(Default)]
struct LimitNotes {
    depth: bool,
    count: bool,
    size: bool,
}

fn enqueue(
    urls: Vec<String>,
    depth: usize,
    seen: &mut HashSet<String>,
    queue: &mut Vec<Queued>,
    noted: &mut LimitNotes,
    bundle: &mut Bundle,
) {
    for url in urls {
        if seen.contains(&url) {
            continue;
        }
        if depth > MAX_DEPTH {
            if !noted.depth {
                noted.depth = true;
                bundle.notes.push(format!(
                    "Files that refer to files were followed {MAX_DEPTH} levels deep, no further."
                ));
            }
            bundle.incomplete = true;
        } else if seen.len() >= MAX_FILES {
            if !noted.count {
                noted.count = true;
                bundle.notes.push(format!(
                    "Only the first {MAX_FILES} referenced files were loaded."
                ));
            }
            bundle.incomplete = true;
        } else {
            seen.insert(url.clone());
            queue.push(Queued { url, depth });
        }
    }
}

/// Fetches the files the main spec refers to (breadth first, at most 20 files, 3 levels and
/// 25 MB in total) and inlines them. A spec with no followable reference comes back as the
/// same text. A file that cannot be used stays unresolved with one note. `_previous` is not
/// used: bodies are always fetched fresh, and `refs_unchanged` does the cheap check.
pub async fn bundle_spec(
    fetcher: &dyn SpecFetcher,
    main_text: &str,
    location: &SpecLocation,
    token: Option<&str>,
    _previous: &[RemoteRefState],
) -> ImportResult<Bundle> {
    let mut bundle = Bundle {
        text: main_text.to_string(),
        refs: Vec::new(),
        notes: Vec::new(),
        incomplete: false,
    };
    let Some(mut main) = parse_yaml_mapping(main_text) else {
        return Ok(bundle);
    };
    let main_base = Url::parse(&raw_address(location)).ok();
    let mut scan = Scan::default();
    collect(&main, main_base.as_ref(), &mut scan);
    bundle.notes = scan.notes;

    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: Vec<Queued> = Vec::new();
    let mut noted = LimitNotes::default();
    enqueue(
        scan.urls,
        1,
        &mut seen,
        &mut queue,
        &mut noted,
        &mut bundle,
    );

    let mut total = main_text.len();
    let mut files: Vec<(String, Yaml)> = Vec::new();
    let mut next = 0;
    while next < queue.len() {
        let (url, depth) = (queue[next].url.clone(), queue[next].depth);
        next += 1;
        let label = file_label(&url);
        if total >= MAX_TOTAL_BYTES {
            bundle.incomplete = true;
            if !noted.size {
                noted.size = true;
                bundle.notes.push(SIZE_NOTE.to_string());
            }
            continue;
        }
        let request = request_for(&url, location, token, MAX_TOTAL_BYTES - total);
        let token_sent = request.bearer_token.is_some();
        let (body, etag, last_modified) = match fetcher.fetch(request).await {
            Ok(SpecFetchOutcome::Fetched {
                body,
                etag,
                last_modified,
                ..
            }) => (body, etag, last_modified),
            Ok(SpecFetchOutcome::NotModified) => {
                bundle.incomplete = true;
                bundle.notes.push(format!(
                    "The file {label} could not be loaded: the server sent no content."
                ));
                continue;
            }
            Err(SpecFetchFailure::TooLarge) => {
                bundle.incomplete = true;
                total = MAX_TOTAL_BYTES;
                if !noted.size {
                    noted.size = true;
                    bundle.notes.push(SIZE_NOTE.to_string());
                }
                continue;
            }
            Err(failure) => {
                bundle.incomplete = true;
                bundle.notes.push(format!(
                    "The file {label} could not be loaded. {}",
                    failure_text(&failure, token_sent)
                ));
                continue;
            }
        };
        if total + body.len() > MAX_TOTAL_BYTES {
            bundle.incomplete = true;
            total = MAX_TOTAL_BYTES;
            if !noted.size {
                noted.size = true;
                bundle.notes.push(SIZE_NOTE.to_string());
            }
            continue;
        }
        total += body.len();
        let Ok(text) = String::from_utf8(body) else {
            bundle.incomplete = true;
            bundle.notes.push(format!(
                "The file {label} could not be loaded: it is not text."
            ));
            continue;
        };
        let Some(value) = parse_yaml_mapping(&text) else {
            bundle.incomplete = true;
            bundle.notes.push(format!(
                "The file {label} could not be loaded: it is not a YAML or JSON mapping."
            ));
            continue;
        };
        let file_base = Url::parse(&url).ok();
        let mut child = Scan::default();
        collect(&value, file_base.as_ref(), &mut child);
        for note in child.notes {
            push_unique(&mut bundle.notes, note);
        }
        enqueue(
            child.urls,
            depth + 1,
            &mut seen,
            &mut queue,
            &mut noted,
            &mut bundle,
        );
        bundle.refs.push(RemoteRefState {
            url: url.clone(),
            etag,
            last_modified,
        });
        files.push((url, value));
    }

    if files.is_empty() {
        return Ok(bundle);
    }
    inline_with_base(&mut main, main_base.as_ref(), &files);
    bundle.text = serde_yaml::to_string(&main).map_err(|_| {
        ImportError::SpecUnavailable(
            "The spec could not be combined with the files it refers to.".to_string(),
        )
    })?;
    Ok(bundle)
}

/// True when every file in `previous` answers 304 to its stored validators. A changed file
/// gives false at once and the first failure is returned. A stored address that is no longer
/// allowed counts as changed, so the full bundle step decides what to do with it.
pub async fn refs_unchanged(
    fetcher: &dyn SpecFetcher,
    location: &SpecLocation,
    token: Option<&str>,
    previous: &[RemoteRefState],
) -> Result<bool, SpecFetchFailure> {
    for state in previous {
        if !matches!(classify(&state.url, None), Target::Remote { .. }) {
            return Ok(false);
        }
        let mut request = request_for(&state.url, location, token, DEFAULT_MAX_SPEC_BYTES);
        request.if_none_match = state.etag.clone();
        request.if_modified_since = state.last_modified.clone();
        match fetcher.fetch(request).await? {
            SpecFetchOutcome::NotModified => {}
            SpecFetchOutcome::Fetched { .. } => return Ok(false),
        }
    }
    Ok(true)
}

#[cfg(test)]
#[path = "bundle_tests.rs"]
mod tests;
