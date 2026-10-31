use super::*;
use async_trait::async_trait;
use rocket_collection::openapi_source::SpecLocation;
use rocket_http::spec_fetch::{SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest, SpecFetcher};
use std::collections::HashMap;
use std::sync::Mutex;

const MAIN_URL: &str = "https://github.com/acme/orders/blob/main/api/openapi.yaml";
const RAW_DIR: &str = "https://raw.githubusercontent.com/acme/orders/main/api";

enum Reply {
    Body(Vec<u8>, Option<String>),
    Fail(SpecFetchFailure),
    Missing,
}

#[derive(Default)]
struct FakeFetcher {
    files: Mutex<HashMap<String, Reply>>,
    requests: Mutex<Vec<SpecFetchRequest>>,
}

impl FakeFetcher {
    fn serve(&self, url: &str, body: &str) -> &Self {
        self.serve_with_etag(url, body, None)
    }

    fn serve_with_etag(&self, url: &str, body: &str, etag: Option<&str>) -> &Self {
        self.files.lock().expect("lock").insert(
            url.to_string(),
            Reply::Body(body.as_bytes().to_vec(), etag.map(str::to_string)),
        );
        self
    }

    fn fail(&self, url: &str, failure: SpecFetchFailure) -> &Self {
        self.files
            .lock()
            .expect("lock")
            .insert(url.to_string(), Reply::Fail(failure));
        self
    }

    fn requests(&self) -> Vec<SpecFetchRequest> {
        self.requests.lock().expect("lock").clone()
    }
}

#[async_trait]
impl SpecFetcher for FakeFetcher {
    async fn fetch(&self, request: SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure> {
        self.requests.lock().expect("lock").push(request.clone());
        let files = self.files.lock().expect("lock");
        match files.get(&request.url).unwrap_or(&Reply::Missing) {
            Reply::Body(body, etag) => {
                if request.if_none_match.is_some() && request.if_none_match == *etag {
                    return Ok(SpecFetchOutcome::NotModified);
                }
                Ok(SpecFetchOutcome::Fetched {
                    body: body.clone(),
                    etag: etag.clone(),
                    last_modified: None,
                    final_url: request.url.clone(),
                })
            }
            Reply::Fail(failure) => Err(failure.clone()),
            Reply::Missing => Err(SpecFetchFailure::NotFound),
        }
    }
}

fn location() -> SpecLocation {
    parse_spec_url(MAIN_URL).expect("location")
}

fn spec_with(refs: &[&str]) -> String {
    let params: String = refs
        .iter()
        .map(|r| format!("        - $ref: '{r}'\n"))
        .collect();
    format!(
        "openapi: 3.0.3\ninfo: {{ title: T, version: '1' }}\npaths:\n  /a:\n    get:\n      operationId: getA\n      parameters:\n{params}      responses: {{ '200': {{ description: ok }} }}\n"
    )
}

fn yaml(text: &str) -> serde_yaml::Value {
    serde_yaml::from_str(text).expect("yaml")
}

#[test]
fn finds_absolute_and_relative_refs_in_order() {
    let text = spec_with(&[
        "https://example.com/b.yaml#/x",
        "defs.yaml#/y",
        "../shared/c.yaml#/z",
        "/root/d.yaml",
    ]);
    let found = find_remote_refs(&text, Some(&format!("{RAW_DIR}/openapi.yaml")));
    assert_eq!(
        found,
        vec![
            "https://example.com/b.yaml",
            "https://raw.githubusercontent.com/acme/orders/main/api/defs.yaml",
            "https://raw.githubusercontent.com/acme/orders/main/shared/c.yaml",
            "https://raw.githubusercontent.com/root/d.yaml",
        ]
    );
}

#[test]
fn a_ref_cannot_climb_above_the_root() {
    let found = find_remote_refs(
        &spec_with(&["../../../../../../x.yaml"]),
        Some("https://example.com/a/openapi.yaml"),
    );
    assert_eq!(found, vec!["https://example.com/x.yaml"]);
}

#[test]
fn ignores_local_refs() {
    let text = spec_with(&["#/components/parameters/p", "#"]);
    assert!(find_remote_refs(&text, Some("https://example.com/a.yaml")).is_empty());
}

#[test]
fn relative_refs_without_a_base_are_not_listed() {
    assert!(find_remote_refs(&spec_with(&["defs.yaml#/x"]), None).is_empty());
}

#[tokio::test]
async fn skips_query_userinfo_http_and_private_hosts_with_a_note() {
    let fetcher = FakeFetcher::default();
    let text = spec_with(&[
        "https://example.com/q.yaml?token=SECRET#/x",
        "https://user:pw@example.com/u.yaml",
        "http://example.com/h.yaml",
        "file:///etc/passwd",
        "https://127.0.0.1/p.yaml",
        "https://localhost/l.yaml",
        "https://10.0.0.5/p2.yaml",
        "https://2130706433/",
        "https://good.com@evil.com/",
        "https://[::1]/",
        "https://0x7f.1/",
        "https://localhost./x",
    ]);
    let found = find_remote_refs(&text, Some(&format!("{RAW_DIR}/openapi.yaml")));
    assert!(found.is_empty(), "{found:?}");
    let bundle = bundle_spec(&fetcher, &text, &location(), None, &[])
        .await
        .expect("bundle");
    assert!(fetcher.requests().is_empty());
    assert_eq!(bundle.text, text);
    // The two loopback spellings (2130706433 and 0x7f.1) share one note.
    assert_eq!(bundle.notes.len(), 11, "{:?}", bundle.notes);
    for note in &bundle.notes {
        assert!(
            !note.contains("SECRET") && !note.contains("token"),
            "{note}"
        );
        assert!(!note.contains("pw"), "{note}");
    }
}

#[test]
fn deduplicates_urls_and_drops_fragments() {
    let text = spec_with(&[
        "https://example.com/b.yaml#/a",
        "https://example.com/b.yaml#/b",
        "https://example.com/b.yaml",
        "https://example.com/b.yaml#",
    ]);
    assert_eq!(
        find_remote_refs(&text, None),
        vec!["https://example.com/b.yaml"]
    );
}

#[test]
fn inlines_a_file_and_rewrites_pointers_inside_it() {
    let mut main = yaml(&spec_with(&[
        "https://example.com/d.yaml#/parameters/limit",
    ]));
    let file = yaml(
        "parameters:\n  limit:\n    name: limit\n    in: query\n    schema:\n      $ref: '#/schemas/Num'\nschemas:\n  Num: { type: integer }\n",
    );
    inline_refs(
        &mut main,
        &[("https://example.com/d.yaml".to_string(), file)],
    );
    let text = serde_yaml::to_string(&main).expect("yaml");
    assert!(
        text.contains("#/x-rocket-remote/0/parameters/limit"),
        "{text}"
    );
    assert!(text.contains("#/x-rocket-remote/0/schemas/Num"), "{text}");
    assert!(!text.contains("example.com"), "{text}");
}

#[test]
fn a_whole_file_ref_points_at_the_file() {
    let mut main = yaml(&spec_with(&[
        "https://example.com/d.yaml",
        "https://example.com/d.yaml#",
    ]));
    inline_refs(
        &mut main,
        &[("https://example.com/d.yaml".to_string(), yaml("a: 1\n"))],
    );
    let text = serde_yaml::to_string(&main).expect("yaml");
    assert_eq!(text.matches("'#/x-rocket-remote/0'").count(), 2, "{text}");
}

#[test]
fn file_to_file_refs_resolve_against_each_files_own_address() {
    let mut main = yaml(&spec_with(&["https://example.com/a/one.yaml#/p"]));
    let one = yaml("p:\n  $ref: 'sub/two.yaml#/q'\n");
    let two = yaml("q: { name: q }\n");
    inline_refs(
        &mut main,
        &[
            ("https://example.com/a/one.yaml".to_string(), one),
            ("https://example.com/a/sub/two.yaml".to_string(), two),
        ],
    );
    let text = serde_yaml::to_string(&main).expect("yaml");
    assert!(text.contains("#/x-rocket-remote/1/q"), "{text}");
    assert!(text.contains("#/x-rocket-remote/0/p"), "{text}");
}

#[tokio::test]
async fn a_cycle_between_two_files_terminates() {
    let fetcher = FakeFetcher::default();
    fetcher
        .serve(
            "https://example.com/a.yaml",
            "x:\n  $ref: 'https://example.com/b.yaml#/y'\n",
        )
        .serve(
            "https://example.com/b.yaml",
            "y:\n  $ref: 'https://example.com/a.yaml#/x'\n",
        );
    let text = spec_with(&["https://example.com/a.yaml#/x"]);
    let bundle = bundle_spec(&fetcher, &text, &location(), None, &[])
        .await
        .expect("bundle");
    assert_eq!(fetcher.requests().len(), 2);
    assert!(!bundle.incomplete);
    assert!(
        bundle.text.contains("#/x-rocket-remote/1/y"),
        "{}",
        bundle.text
    );
    assert!(
        bundle.text.contains("#/x-rocket-remote/0/x"),
        "{}",
        bundle.text
    );
}

#[tokio::test]
async fn same_inputs_give_the_same_bytes() {
    let build = || async {
        let fetcher = FakeFetcher::default();
        fetcher
            .serve(
                "https://example.com/a.yaml",
                "x: { $ref: 'https://example.com/b.yaml#/y' }\n",
            )
            .serve("https://example.com/b.yaml", "y: { name: y }\n")
            .serve("https://example.com/c.yaml", "z: 1\n");
        let text = spec_with(&[
            "https://example.com/a.yaml#/x",
            "https://example.com/c.yaml#/z",
        ]);
        bundle_spec(&fetcher, &text, &location(), None, &[])
            .await
            .expect("bundle")
            .text
    };
    assert_eq!(build().await, build().await);
}

#[tokio::test]
async fn a_spec_without_remote_refs_is_returned_unchanged() {
    let fetcher = FakeFetcher::default();
    // Odd spacing and comments would not survive a re-serialization.
    let text = "# my spec\nopenapi:   3.0.3\ninfo: { title: T, version: '1' }\npaths: {}\n";
    let bundle = bundle_spec(&fetcher, text, &location(), None, &[])
        .await
        .expect("bundle");
    assert_eq!(bundle.text, text);
    assert!(bundle.refs.is_empty() && bundle.notes.is_empty() && !bundle.incomplete);
    assert!(fetcher.requests.lock().expect("lock").is_empty());
}

#[tokio::test]
async fn the_users_case_parameters_limit_offset_sort_resolve() {
    let fetcher = FakeFetcher::default();
    let defs = "https://raw.githubusercontent.com/teamdigitale/openapi/0.0.3/docs/definitions.yaml";
    fetcher.serve_with_etag(
        defs,
        "parameters:\n  limit: { name: limit, in: query, schema: { type: integer } }\n  offset: { name: offset, in: query, schema: { type: integer } }\n  sort: { name: sort, in: query, schema: { type: string } }\n",
        Some("\"d1\""),
    );
    let text = spec_with(&[
        &format!("{defs}#/parameters/limit"),
        &format!("{defs}#/parameters/offset"),
        &format!("{defs}#/parameters/sort"),
    ]);
    let bundle = bundle_spec(&fetcher, &text, &location(), None, &[])
        .await
        .expect("bundle");
    assert_eq!(fetcher.requests().len(), 1);
    assert!(!bundle.incomplete && bundle.notes.is_empty());
    assert_eq!(
        bundle.refs,
        vec![RemoteRefState {
            url: defs.to_string(),
            etag: Some("\"d1\"".to_string()),
            last_modified: None
        }]
    );
    let doc = crate::openapi::parse_openapi(&bundle.text).expect("parse");
    assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
    let names: Vec<_> = doc.operations[0]
        .parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, vec!["limit", "offset", "sort"]);
}

#[tokio::test]
async fn relative_refs_resolve_against_the_main_address() {
    let fetcher = FakeFetcher::default();
    fetcher.serve(
        &format!("{RAW_DIR}/defs.yaml"),
        "parameters:\n  p: { name: p, in: query }\n",
    );
    let text = spec_with(&["defs.yaml#/parameters/p"]);
    let bundle = bundle_spec(&fetcher, &text, &location(), None, &[])
        .await
        .expect("bundle");
    let doc = crate::openapi::parse_openapi(&bundle.text).expect("parse");
    assert_eq!(doc.operations[0].parameters[0].name, "p");
}

#[tokio::test]
async fn token_goes_only_to_the_same_repository() {
    let fetcher = FakeFetcher::default();
    let same = "https://raw.githubusercontent.com/acme/orders/main/api/defs.yaml";
    let same_api = "https://api.github.com/repos/acme/orders/contents/api/defs.yaml?ref=main";
    let other = "https://raw.githubusercontent.com/acme/other/main/defs.yaml";
    fetcher.serve(same_api, "a: 1\n").serve(other, "b: 2\n");
    let text = spec_with(&[same, other]);
    bundle_spec(&fetcher, &text, &location(), Some("tok"), &[])
        .await
        .expect("bundle");
    let requests = fetcher.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url, same_api);
    assert_eq!(requests[0].bearer_token.as_deref(), Some("tok"));
    assert!(requests[0].accept.is_some());
    assert_eq!(requests[1].url, other);
    assert_eq!(requests[1].bearer_token, None);
}

#[tokio::test]
async fn other_hosts_get_no_token() {
    let fetcher = FakeFetcher::default();
    fetcher.serve("https://example.com/d.yaml", "a: 1\n");
    let text = spec_with(&["https://example.com/d.yaml#/a"]);
    bundle_spec(&fetcher, &text, &location(), Some("tok"), &[])
        .await
        .expect("bundle");
    assert_eq!(fetcher.requests()[0].bearer_token, None);

    // A main spec on another host never sends the token, even to GitHub files.
    let fetcher = FakeFetcher::default();
    let gh = "https://raw.githubusercontent.com/acme/orders/main/d.yaml";
    fetcher.serve(gh, "a: 1\n");
    let https = SpecLocation::Https {
        url: "https://example.com/openapi.yaml".to_string(),
    };
    bundle_spec(&fetcher, &spec_with(&[gh]), &https, Some("tok"), &[])
        .await
        .expect("bundle");
    assert_eq!(fetcher.requests()[0].url, gh);
    assert_eq!(fetcher.requests()[0].bearer_token, None);
}

#[tokio::test]
async fn a_failed_file_is_unresolved_with_one_note_and_incomplete_true() {
    let fetcher = FakeFetcher::default();
    fetcher.fail("https://example.com/gone.yaml", SpecFetchFailure::NotFound);
    fetcher.serve("https://example.com/text.yaml", "- not\n- a map\n");
    let text = spec_with(&[
        "https://example.com/gone.yaml#/a",
        "https://example.com/gone.yaml#/b",
        "https://example.com/text.yaml#/c",
    ]);
    let bundle = bundle_spec(&fetcher, &text, &location(), None, &[])
        .await
        .expect("bundle");
    assert!(bundle.incomplete);
    assert_eq!(bundle.notes.len(), 2, "{:?}", bundle.notes);
    assert!(bundle.notes[0].contains("gone.yaml"));
    assert!(bundle.notes[1].contains("text.yaml"));
    assert!(bundle.refs.is_empty());
    assert_eq!(bundle.text, text);
}

#[tokio::test]
async fn limits_stop_the_fetching() {
    // 21 files.
    let fetcher = FakeFetcher::default();
    let urls: Vec<String> = (0..21)
        .map(|i| format!("https://example.com/f{i}.yaml"))
        .collect();
    for u in &urls {
        fetcher.serve(u, "a: 1\n");
    }
    let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
    let bundle = bundle_spec(&fetcher, &spec_with(&refs), &location(), None, &[])
        .await
        .expect("bundle");
    assert_eq!(fetcher.requests().len(), 20);
    assert!(bundle.incomplete);
    assert_eq!(bundle.notes.len(), 1, "{:?}", bundle.notes);

    // Depth 4: main, d1, d2, d3, d4.
    let fetcher = FakeFetcher::default();
    for i in 1..=4 {
        fetcher.serve(
            &format!("https://example.com/d{i}.yaml"),
            &format!("x: {{ $ref: 'https://example.com/d{}.yaml#/x' }}\n", i + 1),
        );
    }
    let bundle = bundle_spec(
        &fetcher,
        &spec_with(&["https://example.com/d1.yaml#/x"]),
        &location(),
        None,
        &[],
    )
    .await
    .expect("bundle");
    assert_eq!(fetcher.requests().len(), 3);
    assert!(bundle.incomplete);
    assert_eq!(bundle.notes.len(), 1, "{:?}", bundle.notes);

    // 26 MB in total.
    let fetcher = FakeFetcher::default();
    let big = format!("a: '{}'\n", "x".repeat(9 * 1024 * 1024));
    let urls: Vec<String> = (0..4)
        .map(|i| format!("https://example.com/big{i}.yaml"))
        .collect();
    for u in &urls {
        fetcher.serve(u, &big);
    }
    let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
    let main_text = spec_with(&refs);
    let bundle = bundle_spec(&fetcher, &main_text, &location(), None, &[])
        .await
        .expect("bundle");
    let requests = fetcher.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[2].max_bytes,
        MAX_TOTAL_BYTES - (main_text.len() + 2 * big.len())
    );
    assert!(bundle.incomplete);
    assert_eq!(bundle.notes.len(), 1, "{:?}", bundle.notes);
    assert_eq!(bundle.refs.len(), 2);
}

#[tokio::test]
async fn a_thousand_references_to_one_file_fetch_it_once() {
    let fetcher = FakeFetcher::default();
    fetcher.serve("https://example.com/d.yaml", "a: 1\n");
    let owned: Vec<String> = (0..1000)
        .map(|i| format!("https://example.com/d.yaml#/a{i}"))
        .collect();
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let bundle = bundle_spec(&fetcher, &spec_with(&refs), &location(), None, &[])
        .await
        .expect("bundle");
    assert_eq!(fetcher.requests().len(), 1);
    assert!(!bundle.incomplete);
}

fn state(url: &str, etag: &str) -> RemoteRefState {
    RemoteRefState {
        url: url.to_string(),
        etag: Some(etag.to_string()),
        last_modified: None,
    }
}

#[tokio::test]
async fn all_304_is_unchanged() {
    let fetcher = FakeFetcher::default();
    fetcher
        .serve_with_etag("https://example.com/a.yaml", "a: 1\n", Some("\"1\""))
        .serve_with_etag("https://example.com/b.yaml", "b: 1\n", Some("\"2\""));
    let previous = [
        state("https://example.com/a.yaml", "\"1\""),
        state("https://example.com/b.yaml", "\"2\""),
    ];
    assert_eq!(
        refs_unchanged(&fetcher, &location(), None, &previous).await,
        Ok(true)
    );
    assert_eq!(
        fetcher.requests()[0].if_none_match.as_deref(),
        Some("\"1\"")
    );
}

#[tokio::test]
async fn one_200_is_changed() {
    let fetcher = FakeFetcher::default();
    fetcher
        .serve_with_etag("https://example.com/a.yaml", "a: 1\n", Some("\"1\""))
        .serve_with_etag("https://example.com/b.yaml", "b: 2\n", Some("\"new\""));
    let previous = [
        state("https://example.com/a.yaml", "\"1\""),
        state("https://example.com/b.yaml", "\"2\""),
    ];
    assert_eq!(
        refs_unchanged(&fetcher, &location(), None, &previous).await,
        Ok(false)
    );
}

#[tokio::test]
async fn a_failure_is_returned() {
    let fetcher = FakeFetcher::default();
    fetcher.fail("https://example.com/a.yaml", SpecFetchFailure::Status(500));
    let previous = [state("https://example.com/a.yaml", "\"1\"")];
    assert_eq!(
        refs_unchanged(&fetcher, &location(), None, &previous).await,
        Err(SpecFetchFailure::Status(500))
    );
}

#[tokio::test]
async fn lookalike_repositories_get_no_token_and_other_refs_of_the_repo_do() {
    let fetcher = FakeFetcher::default();
    let evil_a = "https://raw.githubusercontent.com/acme/orders-evil/main/a.yaml";
    let evil_b = "https://raw.githubusercontent.com/evil/orders/main/b.yaml";
    let dev = "https://raw.githubusercontent.com/acme/orders/dev/api/c.yaml";
    let dev_api = "https://api.github.com/repos/acme/orders/contents/api/c.yaml?ref=dev";
    fetcher
        .serve(evil_a, "a: 1\n")
        .serve(evil_b, "b: 1\n")
        .serve(dev_api, "c: 1\n");
    bundle_spec(
        &fetcher,
        &spec_with(&[evil_a, evil_b, dev]),
        &location(),
        Some("tok"),
        &[],
    )
    .await
    .expect("bundle");
    let requests = fetcher.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].url, evil_a);
    assert_eq!(requests[0].bearer_token, None);
    assert_eq!(requests[1].url, evil_b);
    assert_eq!(requests[1].bearer_token, None);
    assert_eq!(requests[2].url, dev_api);
    assert_eq!(requests[2].bearer_token.as_deref(), Some("tok"));
}

#[tokio::test]
async fn a_github_blob_link_to_another_repo_is_fetched_raw_without_token() {
    let fetcher = FakeFetcher::default();
    let raw = "https://raw.githubusercontent.com/other/repo/main/defs.yaml";
    fetcher.serve(raw, "a: 1\n");
    let blob = "https://github.com/other/repo/blob/main/defs.yaml";
    bundle_spec(&fetcher, &spec_with(&[blob]), &location(), Some("tok"), &[])
        .await
        .expect("bundle");
    let requests = fetcher.requests();
    assert_eq!(requests[0].url, raw);
    assert_eq!(requests[0].bearer_token, None);
}

// The test lock is held across awaits on purpose; the test runtime has one thread.
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn a_huge_spec_scans_fast_and_keeps_few_notes() {
    let fetcher = FakeFetcher::default();
    let _timed = crate::openapi::yaml_budget::timed_test();
    let started = std::time::Instant::now();
    let owned: Vec<String> = (0..50_000)
        .map(|i| format!("http://example.com/f{i}.yaml"))
        .collect();
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let bundle = bundle_spec(&fetcher, &spec_with(&refs), &location(), None, &[])
        .await
        .expect("bundle");
    assert!(bundle.notes.len() <= 21, "{}", bundle.notes.len());
    assert!(bundle.notes.last().is_some_and(|n| n.starts_with("and ")));

    let owned: Vec<String> = (0..50_000)
        .map(|i| format!("https://example.com/f{i}.yaml"))
        .collect();
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let bundle = bundle_spec(&fetcher, &spec_with(&refs), &location(), None, &[])
        .await
        .expect("bundle");
    assert_eq!(fetcher.requests().len(), 20);
    assert!(bundle.incomplete);
    // A loose bound: it catches unbounded work, and a slow shared CI runner still passes.
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}

#[tokio::test]
async fn alias_growth_gives_back_the_original_text_with_a_note() {
    let fetcher = FakeFetcher::default();
    let items: Vec<String> = (0..1000).map(|i| i.to_string()).collect();
    let aliases = vec!["*a"; 1000].join(", ");
    let body = format!("a: &a [{}]\nb: [{aliases}]\n", items.join(", "));
    fetcher.serve("https://example.com/bomb.yaml", &body);
    let text = spec_with(&["https://example.com/bomb.yaml#/a"]);
    let bundle = bundle_with_limit(
        &fetcher,
        &text,
        &location(),
        None,
        200_000,
        node_limit_for,
        MAX_YAML_BYTES,
    )
    .await
    .expect("bundle");
    assert_eq!(bundle.text, text);
    assert!(bundle.incomplete);
    assert!(bundle.refs.is_empty());
    assert_eq!(bundle.notes.len(), 1, "{:?}", bundle.notes);
}

fn alias_bomb(items: usize) -> String {
    let list: Vec<String> = (0..items).map(|i| i.to_string()).collect();
    let aliases = vec!["*a"; items].join(", ");
    format!("a: &a [{}]\nb: [{aliases}]\n", list.join(", "))
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn a_flat_alias_bomb_in_a_file_is_refused_quickly() {
    let fetcher = FakeFetcher::default();
    fetcher.serve("https://example.com/bomb.yaml", &alias_bomb(5000));
    let text = spec_with(&["https://example.com/bomb.yaml#/a"]);
    let _timed = crate::openapi::yaml_budget::timed_test();
    let started = std::time::Instant::now();
    let bundle = bundle_with_limit(
        &fetcher,
        &text,
        &location(),
        None,
        MAX_TOTAL_BYTES,
        |_| 100_000,
        MAX_YAML_BYTES,
    )
    .await
    .expect("bundle");
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(bundle.text, text);
    assert!(bundle.incomplete);
    assert_eq!(bundle.notes.len(), 1, "{:?}", bundle.notes);
    assert!(bundle.notes[0].contains("bomb.yaml") && bundle.notes[0].contains("too complex"));
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn a_big_string_aliased_many_times_is_refused_by_bytes() {
    let fetcher = FakeFetcher::default();
    let big = "x".repeat(100 * 1024);
    let aliases = vec!["*a"; 2000].join(", ");
    fetcher.serve(
        "https://example.com/s.yaml",
        &format!("a: &a '{big}'\nb: [{aliases}]\n"),
    );
    let text = spec_with(&["https://example.com/s.yaml#/a"]);
    let _timed = crate::openapi::yaml_budget::timed_test();
    let started = std::time::Instant::now();
    // Well under the node limit; only the byte limit (10 MB here) stops it.
    let bundle = bundle_with_limit(
        &fetcher,
        &text,
        &location(),
        None,
        MAX_TOTAL_BYTES,
        node_limit_for,
        10 * 1024 * 1024,
    )
    .await
    .expect("bundle");
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(bundle.incomplete);
    assert_eq!(bundle.notes.len(), 1, "{:?}", bundle.notes);
    assert!(bundle.notes[0].contains("too complex"));
}

#[tokio::test]
async fn a_big_aliased_mapping_key_is_refused_by_bytes() {
    let fetcher = FakeFetcher::default();
    let big = "k".repeat(100 * 1024);
    let entries: Vec<String> = (0..2000).map(|_| "*a : 1".to_string()).collect();
    fetcher.serve(
        "https://example.com/k.yaml",
        &format!("a: &a '{big}'\nb: {{ {} }}\n", entries.join(", ")),
    );
    let text = spec_with(&["https://example.com/k.yaml#/a"]);
    let bundle = bundle_with_limit(
        &fetcher,
        &text,
        &location(),
        None,
        MAX_TOTAL_BYTES,
        node_limit_for,
        10 * 1024 * 1024,
    )
    .await
    .expect("bundle");
    assert!(bundle.incomplete);
    assert_eq!(bundle.notes.len(), 1, "{:?}", bundle.notes);
}

#[tokio::test]
async fn the_budget_is_shared_by_all_files() {
    let fetcher = FakeFetcher::default();
    let body = |n: usize| format!("a: [{}]\n", vec!["1"; n].join(", "));
    for i in 0..3 {
        fetcher.serve(&format!("https://example.com/f{i}.yaml"), &body(400));
    }
    let text = spec_with(&[
        "https://example.com/f0.yaml#/a",
        "https://example.com/f1.yaml#/a",
        "https://example.com/f2.yaml#/a",
    ]);
    // Each file is about 402 nodes; 900 in total admits the first two at most.
    let bundle = bundle_with_limit(
        &fetcher,
        &text,
        &location(),
        None,
        MAX_TOTAL_BYTES,
        |_| 900,
        MAX_YAML_BYTES,
    )
    .await
    .expect("bundle");
    assert_eq!(bundle.refs.len(), 2, "{:?}", bundle.notes);
    assert!(bundle.incomplete);
    assert_eq!(bundle.notes.len(), 1, "{:?}", bundle.notes);
}

#[tokio::test]
async fn a_file_over_the_shared_budget_and_the_files_after_it_get_notes() {
    let fetcher = FakeFetcher::default();
    let body = |n: usize| format!("a: [{}]\n", vec!["1"; n].join(", "));
    for i in 0..3 {
        fetcher.serve(&format!("https://example.com/f{i}.yaml"), &body(400));
    }
    let text = spec_with(&[
        "https://example.com/f0.yaml#/a",
        "https://example.com/f1.yaml#/a",
        "https://example.com/f2.yaml#/a",
    ]);
    let bundle = bundle_with_limit(
        &fetcher,
        &text,
        &location(),
        None,
        MAX_TOTAL_BYTES,
        |_| 500,
        MAX_YAML_BYTES,
    )
    .await
    .expect("bundle");
    // The first loads, the second pushes the budget over, the third is not even fetched.
    assert_eq!(bundle.refs.len(), 1);
    assert_eq!(fetcher.requests().len(), 2);
    assert_eq!(bundle.notes.len(), 2, "{:?}", bundle.notes);
    assert!(bundle.notes[1].contains("remaining files"));
}

#[tokio::test]
async fn a_file_with_a_few_aliases_still_loads() {
    let fetcher = FakeFetcher::default();
    fetcher.serve(
        "https://example.com/d.yaml",
        "base: &b { name: p, in: query }\np: *b\nq: *b\n",
    );
    let text = spec_with(&["https://example.com/d.yaml#/p"]);
    let bundle = bundle_with_limit(
        &fetcher,
        &text,
        &location(),
        None,
        MAX_TOTAL_BYTES,
        |_| 100_000,
        MAX_YAML_BYTES,
    )
    .await
    .expect("bundle");
    assert!(
        !bundle.incomplete && bundle.notes.is_empty(),
        "{:?}",
        bundle.notes
    );
    let doc = crate::openapi::parse_openapi(&bundle.text).expect("parse");
    assert_eq!(doc.operations[0].parameters[0].name, "p");
}

#[tokio::test]
async fn a_main_text_alias_bomb_returns_the_original_text() {
    let fetcher = FakeFetcher::default();
    let text = format!(
        "{}x-bomb:\n  {}\n",
        spec_with(&["https://example.com/d.yaml#/a"]),
        alias_bomb(5000).replace('\n', "\n  ")
    );
    let bundle = bundle_with_limit(
        &fetcher,
        &text,
        &location(),
        None,
        MAX_TOTAL_BYTES,
        |_| 100_000,
        MAX_YAML_BYTES,
    )
    .await
    .expect("bundle");
    assert_eq!(bundle.text, text);
    assert!(bundle.incomplete);
    assert_eq!(bundle.notes.len(), 1);
    assert!(fetcher.requests.lock().expect("lock").is_empty());
}

#[tokio::test]
async fn a_bom_prefixed_json_body_loads_as_json() {
    let fetcher = FakeFetcher::default();
    fetcher.serve(
        "https://example.com/d.json",
        "\u{feff}  {\"p\": {\"name\": \"p\", \"in\": \"query\"}}",
    );
    let text = spec_with(&["https://example.com/d.json#/p"]);
    let bundle = bundle_spec(&fetcher, &text, &location(), None, &[])
        .await
        .expect("bundle");
    assert!(bundle.notes.is_empty(), "{:?}", bundle.notes);
    let doc = crate::openapi::parse_openapi(&bundle.text).expect("parse");
    assert_eq!(doc.operations[0].parameters[0].name, "p");
}

#[tokio::test]
async fn a_json_file_is_read_without_aliases() {
    let fetcher = FakeFetcher::default();
    fetcher.serve(
        "https://example.com/d.json",
        "{\"p\": {\"name\": \"p\", \"in\": \"query\"}}",
    );
    let text = spec_with(&["https://example.com/d.json#/p"]);
    let bundle = bundle_spec(&fetcher, &text, &location(), None, &[])
        .await
        .expect("bundle");
    let doc = crate::openapi::parse_openapi(&bundle.text).expect("parse");
    assert_eq!(doc.operations[0].parameters[0].name, "p");
}

#[tokio::test]
async fn a_main_spec_with_the_root_key_is_left_alone() {
    let fetcher = FakeFetcher::default();
    let text = format!(
        "{}x-rocket-remote: {{}}\n",
        spec_with(&["https://example.com/d.yaml#/a"])
    );
    let bundle = bundle_spec(&fetcher, &text, &location(), None, &[])
        .await
        .expect("bundle");
    assert_eq!(bundle.text, text);
    assert_eq!(bundle.notes.len(), 1);
    assert!(!bundle.incomplete);
    assert!(fetcher.requests.lock().expect("lock").is_empty());
}

#[tokio::test]
async fn a_ref_back_to_the_main_spec_points_at_the_main_document() {
    let fetcher = FakeFetcher::default();
    let main_raw = format!("{RAW_DIR}/openapi.yaml");
    fetcher.serve(
        &format!("{RAW_DIR}/defs.yaml"),
        &format!("p:\n  $ref: '{main_raw}#/components/parameters/q'\n"),
    );
    let text = format!(
        "{}components:\n  parameters:\n    q: {{ name: q, in: query }}\n",
        spec_with(&["defs.yaml#/p"])
    );
    let bundle = bundle_spec(&fetcher, &text, &location(), None, &[])
        .await
        .expect("bundle");
    assert_eq!(fetcher.requests().len(), 1);
    assert!(
        bundle.text.contains("'#/components/parameters/q'"),
        "{}",
        bundle.text
    );
    let doc = crate::openapi::parse_openapi(&bundle.text).expect("parse");
    assert_eq!(doc.operations[0].parameters[0].name, "q");
}

#[tokio::test]
async fn a_stored_ref_to_a_blocked_address_is_never_fetched() {
    let fetcher = FakeFetcher::default();
    let previous = [state("https://127.0.0.1/a.yaml", "\"1\"")];
    assert_eq!(
        refs_unchanged(&fetcher, &location(), None, &previous).await,
        Ok(false)
    );
    assert!(fetcher.requests.lock().expect("lock").is_empty());
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn a_large_bundle_without_aliases_loads_and_the_reader_accepts_it() {
    use crate::openapi::yaml_budget::large_alias_free_spec;
    let _heavy = crate::openapi::yaml_budget::heavy_test();
    let fetcher = FakeFetcher::default();
    let mut refs = String::new();
    let mut texts = Vec::new();
    for i in 0..3 {
        let url = format!("https://example.com/f{i}.yaml");
        let body = format!(
            "p: {{ name: p{i}, in: query }}\n{}",
            large_alias_free_spec(1000)
        );
        fetcher.serve(&url, &body);
        refs.push_str(&format!("        - $ref: '{url}#/p'\n"));
        texts.push(body);
    }
    let main = large_alias_free_spec(1500).replacen(
        "paths:\n",
        &format!(
            "paths:\n  /a:\n    get:\n      operationId: getA\n      parameters:\n{refs}      \
             responses: {{ '200': {{ description: ok }} }}\n"
        ),
        1,
    );
    let total: usize = main.len() + texts.iter().map(String::len).sum::<usize>();
    assert!(total >= 8 * 1024 * 1024, "{total} bytes");

    let bundle = bundle_spec(&fetcher, &main, &location(), None, &[])
        .await
        .expect("bundle");
    assert!(!bundle.incomplete, "{:?}", bundle.notes);
    assert_eq!(bundle.refs.len(), 3);
    // The bundle has more than 1,000,000 nodes, so the old fixed reader limit refused it.
    let old_reader = YamlBudget::new(1_000_000, MAX_YAML_BYTES);
    assert_eq!(old_reader.check(&bundle.text), Err(BudgetFail::Exceeded));
    // A bundle that passed bundling is not refused by the reader for its node count.
    let doc = crate::openapi::parse_openapi(&bundle.text).expect("parse");
    assert_eq!(doc.operations.len(), 1501);
    let get_a = doc
        .operations
        .iter()
        .find(|op| op.key == "getA")
        .expect("getA");
    let names: Vec<&str> = get_a.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["p0", "p1", "p2"]);
}

#[tokio::test]
async fn a_missing_file_in_another_repository_does_not_ask_for_a_token() {
    let fetcher = FakeFetcher::default();
    let other = "https://raw.githubusercontent.com/other/repo/main/d.yaml";
    let same = format!("{RAW_DIR}/e.yaml");
    let text = spec_with(&[
        &format!("{other}#/parameters/p"),
        &format!("{same}#/parameters/p"),
    ]);
    let bundle = bundle_spec(&fetcher, &text, &location(), None, &[])
        .await
        .expect("bundle");
    assert_eq!(bundle.notes.len(), 2, "{:?}", bundle.notes);
    let foreign = bundle
        .notes
        .iter()
        .find(|n| n.contains("d.yaml"))
        .expect("note");
    assert!(
        foreign.contains("sends the token only to files in the spec's own repository"),
        "{foreign}"
    );
    assert!(!foreign.contains("add an access token"), "{foreign}");
    let own = bundle
        .notes
        .iter()
        .find(|n| n.contains("e.yaml"))
        .expect("note");
    assert!(own.contains("add an access token"), "{own}");
}

#[tokio::test]
async fn unloaded_lists_a_failing_file_and_a_limit_skipped_file() {
    let fetcher = FakeFetcher::default();
    let urls: Vec<String> = (0..22)
        .map(|i| format!("https://example.com/f{i}.yaml"))
        .collect();
    for u in urls.iter().skip(1) {
        fetcher.serve(u, "parameters:\n  p: { name: p, in: query }\n");
    }
    let refs: Vec<String> = urls.iter().map(|u| format!("{u}#/parameters/p")).collect();
    let refs: Vec<&str> = refs.iter().map(String::as_str).collect();
    let bundle = bundle_spec(&fetcher, &spec_with(&refs), &location(), None, &[])
        .await
        .expect("bundle");
    assert!(bundle.incomplete);
    // f0 answers 404 (missing), f20 is past the 20 file limit.
    assert!(bundle.unloaded.contains(&urls[0]), "{:?}", bundle.unloaded);
    assert!(bundle.unloaded.contains(&urls[20]), "{:?}", bundle.unloaded);
    assert!(!bundle.unloaded.contains(&urls[1]));
    assert!(bundle.scan_truncated);
    let mut dedup = bundle.unloaded.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(dedup.len(), bundle.unloaded.len());
}

#[tokio::test]
async fn a_file_location_is_not_bundled_and_never_fetches() {
    let fetcher = FakeFetcher::default();
    let text = "openapi: 3.0.0\ninfo: {title: t, version: '1'}\npaths:\n  /a:\n    get:\n      responses:\n        '200':\n          description: ok\n          content:\n            application/json:\n              schema:\n                $ref: 'https://example.com/schemas/a.yaml'\n";
    let location = SpecLocation::File {
        path: "spec.yaml".to_string(),
    };
    let bundle = bundle_spec(&fetcher, text, &location, None, &[])
        .await
        .expect("bundle");
    assert_eq!(bundle.text, text);
    assert!(bundle.refs.is_empty());
    assert_eq!(bundle.notes.len(), 1, "{:?}", bundle.notes);
    assert!(fetcher.requests.lock().expect("lock").is_empty());
}
