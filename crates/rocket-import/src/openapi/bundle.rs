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
use serde::de::{
    DeserializeSeed, Deserializer, EnumAccess, IgnoredAny, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use serde_yaml::Value as Yaml;
use std::cell::Cell;
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

/// Most YAML nodes one text may yield, counting an alias every time it is replayed.
const MAX_YAML_NODES: usize = 1_000_000;

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

/// Most skip notes kept for one document; the rest are counted in one closing line.
const MAX_NOTES: usize = 20;

/// The URLs and notes found in one document, each once and in first-encounter order. The
/// scan stops once it has more URLs than the file limit allows, so a huge spec stays cheap.
struct Scan {
    urls: Vec<String>,
    url_set: HashSet<String>,
    notes: Vec<String>,
    note_set: HashSet<String>,
    extra_notes: usize,
    /// The address of the main spec. A reference back to it needs no fetch.
    exclude: Option<String>,
}

impl Scan {
    fn new(exclude: Option<String>) -> Self {
        Self {
            urls: Vec::new(),
            url_set: HashSet::new(),
            notes: Vec::new(),
            note_set: HashSet::new(),
            extra_notes: 0,
            exclude,
        }
    }

    fn full(&self) -> bool {
        self.urls.len() > MAX_FILES
    }

    fn add(&mut self, target: Target) {
        match target {
            Target::Remote { url, .. } => {
                if self.exclude.as_deref() == Some(url.as_str()) {
                    return;
                }
                if self.url_set.insert(url.clone()) {
                    self.urls.push(url);
                }
            }
            Target::Skip(note) => {
                if self.note_set.contains(&note) {
                    return;
                }
                if self.notes.len() < MAX_NOTES {
                    self.note_set.insert(note.clone());
                    self.notes.push(note);
                } else {
                    self.extra_notes += 1;
                }
            }
            Target::Local { .. } => {}
        }
    }

    /// Adds the closing line for the notes that were not kept.
    fn finish(&mut self) {
        if self.extra_notes > 0 {
            self.notes.push(format!(
                "and {} more references were not followed.",
                self.extra_notes
            ));
            self.extra_notes = 0;
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
                if scan.full() {
                    return;
                }
                match (key.as_str(), child.as_str()) {
                    (Some("$ref"), Some(reference)) => scan.add(classify(reference, base)),
                    _ => collect(child, base, scan),
                }
            }
        }
        Yaml::Sequence(items) => {
            for item in items {
                if scan.full() {
                    return;
                }
                collect(item, base, scan);
            }
        }
        _ => {}
    }
}

/// Why a text did not become a mapping.
enum ParseFail {
    /// It is not YAML or JSON, or not a mapping.
    Invalid,
    /// It has more nodes than the budget, aliases counted every time they are replayed.
    TooComplex,
}

/// Counts every node a YAML text yields, so an alias bomb is refused before any value is built.
struct NodeBudget<'a> {
    count: &'a Cell<usize>,
    max: usize,
}

impl NodeBudget<'_> {
    fn tick<E: serde::de::Error>(&self) -> Result<(), E> {
        let next = self.count.get() + 1;
        self.count.set(next);
        if next > self.max {
            Err(E::custom("too many nodes"))
        } else {
            Ok(())
        }
    }

    fn again(&self) -> NodeBudget<'_> {
        NodeBudget {
            count: self.count,
            max: self.max,
        }
    }
}

impl<'de> DeserializeSeed<'de> for NodeBudget<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for NodeBudget<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("any YAML value")
    }

    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        self.tick()
    }

    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        self.tick()
    }

    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        self.tick()
    }

    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        self.tick()
    }

    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<(), E> {
        self.tick()
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        self.tick()
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<(), E> {
        self.tick()
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        self.tick()?;
        deserializer.deserialize_any(self)
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        self.tick()?;
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.tick()?;
        while seq.next_element_seed(self.again())?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.tick()?;
        while map.next_key_seed(self.again())?.is_some() {
            map.next_value_seed(self.again())?;
        }
        Ok(())
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<(), A::Error> {
        self.tick()?;
        let (_, variant) = data.variant::<IgnoredAny>()?;
        variant.newtype_variant_seed(self.again())
    }
}

/// Reads a YAML text into a mapping after checking it stays within the node budget.
fn parse_yaml_mapping(text: &str, max_nodes: usize) -> Result<Yaml, ParseFail> {
    let text = text.trim_start_matches('\u{feff}');
    let count = Cell::new(0);
    let budget = NodeBudget {
        count: &count,
        max: max_nodes,
    };
    if budget
        .deserialize(serde_yaml::Deserializer::from_str(text))
        .is_err()
    {
        return Err(if count.get() > max_nodes {
            ParseFail::TooComplex
        } else {
            ParseFail::Invalid
        });
    }
    serde_yaml::from_str::<Yaml>(text)
        .ok()
        .filter(|v| v.is_mapping())
        .ok_or(ParseFail::Invalid)
}

/// Parses a referenced file. A body that starts with `{` (after a BOM and spaces) is read as
/// JSON, which has no alias syntax to blow up; anything else is read as YAML.
fn parse_file_mapping(text: &str, max_nodes: usize) -> Result<Yaml, ParseFail> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{') {
        let json: serde_json::Value =
            serde_json::from_str(trimmed).map_err(|_| ParseFail::Invalid)?;
        return serde_yaml::to_value(&json)
            .ok()
            .filter(|v| v.is_mapping())
            .ok_or(ParseFail::Invalid);
    }
    parse_yaml_mapping(trimmed, max_nodes)
}

/// The absolute, fragment-free, deduplicated URLs the main text refers to, in first-encounter
/// order. Skipped references are not listed. A relative reference needs `base`. At most one
/// more than the file limit is returned.
pub fn find_remote_refs(main_text: &str, base: Option<&str>) -> Vec<String> {
    let Ok(root) = parse_yaml_mapping(main_text, MAX_YAML_NODES) else {
        return Vec::new();
    };
    let base = base.and_then(|b| Url::parse(b).ok());
    let mut scan = Scan::new(None);
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
    main_url: Option<&str>,
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
                            // A reference back to the main spec points at the main document.
                            Target::Remote { url, fragment } if main_url == Some(url.as_str()) => {
                                Some(format!("#{fragment}"))
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
                rewrite(child, base, owner, index, main_url);
            }
        }
        Yaml::Sequence(items) => {
            for item in items {
                rewrite(item, base, owner, index, main_url);
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

fn inline_with_base(
    main: &mut Yaml,
    main_base: Option<&Url>,
    main_url: Option<&str>,
    files: &[(String, Yaml)],
) {
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
        rewrite(&mut copy, base.as_ref(), Some(i), &index, main_url);
        root.insert(Yaml::String(i.to_string()), copy);
    }
    // The main document is rewritten before the files are attached to it.
    rewrite(main, main_base, None, &index, main_url);
    if let Yaml::Mapping(map) = main {
        map.insert(Yaml::String(ROOT_KEY.to_string()), Yaml::Mapping(root));
    }
}

/// Copies each file under `x-rocket-remote.<i>` (by position in `files`) and rewrites the
/// `$ref` values that point at them. The URLs in `files` are absolute. Relative references in
/// the main document need an address, so use `bundle_spec` for those.
pub fn inline_refs(main: &mut Yaml, files: &[(String, Yaml)]) {
    inline_with_base(main, None, None, files);
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
    // A GitHub file of another repository is fetched in its raw form, never with the token.
    let (url, accept, bearer_token) = match (location, parse_spec_url(file_url)) {
        (SpecLocation::Github { owner, repo, .. }, Ok(file_location)) => {
            let same = matches!(
                &file_location,
                SpecLocation::Github { owner: o, repo: r, .. }
                    if o.eq_ignore_ascii_case(owner) && r.eq_ignore_ascii_case(repo)
            );
            if same {
                let target = fetch_target(&file_location, token.is_some());
                (target.url, target.accept, token.map(str::to_string))
            } else {
                (fetch_target(&file_location, false).url, None, None)
            }
        }
        (_, Ok(file_location @ SpecLocation::Github { .. })) => {
            (fetch_target(&file_location, false).url, None, None)
        }
        _ => (file_url.to_string(), None, None),
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
    bundle_with_limit(
        fetcher,
        main_text,
        location,
        token,
        MAX_TOTAL_BYTES,
        MAX_YAML_NODES,
    )
    .await
}

async fn bundle_with_limit(
    fetcher: &dyn SpecFetcher,
    main_text: &str,
    location: &SpecLocation,
    token: Option<&str>,
    max_total: usize,
    max_nodes: usize,
) -> ImportResult<Bundle> {
    let mut bundle = Bundle {
        text: main_text.to_string(),
        refs: Vec::new(),
        notes: Vec::new(),
        incomplete: false,
    };
    let mut main = match parse_yaml_mapping(main_text, max_nodes) {
        Ok(main) => main,
        Err(ParseFail::Invalid) => return Ok(bundle),
        Err(ParseFail::TooComplex) => {
            bundle.incomplete = true;
            bundle
                .notes
                .push("The spec is too large or too complex to follow its references.".to_string());
            return Ok(bundle);
        }
    };
    if main.get(ROOT_KEY).is_some() {
        bundle.notes.push(format!(
            "The spec already has a {ROOT_KEY} key, so its references were not followed."
        ));
        return Ok(bundle);
    }
    let main_base = Url::parse(&raw_address(location)).ok();
    let main_url = main_base.as_ref().map(|u| canonical(u.as_str()));
    let mut scan = Scan::new(main_url.clone());
    collect(&main, main_base.as_ref(), &mut scan);
    scan.finish();
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
        if total >= max_total {
            bundle.incomplete = true;
            if !noted.size {
                noted.size = true;
                bundle.notes.push(SIZE_NOTE.to_string());
            }
            continue;
        }
        let request = request_for(&url, location, token, max_total - total);
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
                total = max_total;
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
        if total + body.len() > max_total {
            bundle.incomplete = true;
            total = max_total;
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
        let value = match parse_file_mapping(&text, max_nodes) {
            Ok(value) => value,
            Err(failure) => {
                bundle.incomplete = true;
                bundle.notes.push(match failure {
                    ParseFail::Invalid => format!(
                        "The file {label} could not be loaded: it is not a YAML or JSON mapping."
                    ),
                    ParseFail::TooComplex => {
                        format!("The file {label} is too large or too complex to load.")
                    }
                });
                continue;
            }
        };
        let file_base = Url::parse(&url).ok();
        let mut child = Scan::new(main_url.clone());
        collect(&value, file_base.as_ref(), &mut child);
        child.finish();
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
    inline_with_base(&mut main, main_base.as_ref(), main_url.as_deref(), &files);
    let text = serde_yaml::to_string(&main).map_err(|_| {
        ImportError::SpecUnavailable(
            "The spec could not be combined with the files it refers to.".to_string(),
        )
    })?;
    // Aliases in a small file can expand to a much larger document.
    if text.len() > max_total {
        bundle.refs.clear();
        bundle.incomplete = true;
        bundle.notes.push(
            "The referenced files grow too large when combined, so none of them were used."
                .to_string(),
        );
        return Ok(bundle);
    }
    bundle.text = text;
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
