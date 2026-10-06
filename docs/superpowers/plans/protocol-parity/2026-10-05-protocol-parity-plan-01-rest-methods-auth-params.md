# Protocol parity, Plan 01: REST methods, SPARQL and OAuth 1.0 in the UI, parameter resolution

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a request use any HTTP method (TRACE, CONNECT and custom tokens), make the SPARQL body and OAuth 1.0 auth usable from the UI, and move path and query parameter resolution into the backend so every execution path (single send, collection runner, flows, load test) behaves the same.

**Architecture:** `HttpMethod` gains `Trace`, `Connect` and `Custom(String)` and serializes as a plain string, so persistence, IPC and scripts keep exchanging method names as text. Path-parameter substitution becomes a pure function in `rocket-http`, called by `RequestExecutionService::resolve_request` after variable resolution, together with `{{var}}` resolution of query parameter keys and values. The frontend stops substituting path parameters on the wire and only keeps a display helper for the cURL copy and the console.

**Tech Stack:** Rust (`rocket-shared`, `rocket-http`, `rocket-collection`, `rocket-app`, `rocket-infra`), wiremock, React + TypeScript, Vitest, Biome, `cargo test -j4 -p <crate>`.

**Spec:** [../../specs/opencollection-spec-reference.md](../../specs/opencollection-spec-reference.md) (sections 2.3 `HttpRequestParam` with `type: "query" | "path"`, 2.9 `HttpRequestBody` incl. `sparql`, and the `AuthOAuth1` shape). Audit summary (verified against the code on 2026-10-05):

- `HttpMethod` has 7 variants and `FromStr` rejects everything else (`crates/rocket-shared/src/types.rs:14-58`). Worse, `crates/rocket-infra/src/conversions/request.rs:22-26` does `.parse::<HttpMethod>().unwrap_or(HttpMethod::Get)`, so a request file with any other method silently becomes GET today. The Postman importer (`crates/rocket-import/src/converter/postman.rs:189`) does the same.
- The SPARQL body is implemented and persisted in the backend (`reqwest_executor.rs` `BodyMode::Sparql`, `conversions/body.rs`) but absent from `BODY_MODES` (`src/components/request/RequestPanel.tsx:93`), `BodyState['mode']` (`src/types/pane-types.ts:264`) and `BodyMode` (`src/lib/tauri-api.ts:22`).
- OAuth 1.0 is signed in the backend (`apply_oauth1`) and round-trips through `persisted-auth.ts`, but it is only listed as a read-only option (`src/lib/auth-type-options.ts`) and has NO editor: `AuthEditor.tsx` shows an info card. Selecting it alone is not enough, this plan adds the editor. RSA-* signature methods fail the request, so the editor offers HMAC-* and PLAINTEXT only.
- `resolve_request` (`crates/rocket-app/src/execution_service.rs:664-735`) resolves `{{var}}` in auth, URL, headers and body but passes `input.query_params.clone()` through unresolved. The frontend resolves query values before sending (`resolveRequestFieldsForPath`), which hides the gap for the single send, but `build_step_input` (`crates/rocket-app/src/runner_sequence.rs:116-139`) feeds raw saved query values and an unsubstituted `:id` URL straight into `resolve_request`. So the backend collection runner and any flow node send `{{var}}` and `:id` literally today.
- Path parameters are substituted only in the frontend (`src/lib/execute-request.ts:243-246`): `String.replace` (first occurrence only, and `:id` also rewrites the start of `:idx`), and the value is not variable-resolved. The load-test dialog does not even send `pathParams`.
- Additional finding: saved path-parameter values are lost by the UI. `mapApiRequestToState` (`src/lib/pane-utils.ts:39-44`) rebuilds path params from the URL with `value: ''`, the TS `Request` type has no `pathParams`, and `buildRequestSavePayload` (`src/lib/request-save-mapper.ts`) never sends them. Task 3 fixes the round trip.

## Global Constraints

- Never apply `#[serde(rename_all = "camelCase")]` to persistence structs. `HttpMethod` is shared by persistence and IPC, so it serializes as a plain string with no rename attribute.
- Production code never panics on bad input: use `DomainResult` and explicit error mapping, no `unwrap()`. Tests may use `.expect("reason")`.
- Always pass `-j4` to `cargo test` and `cargo check`. Never run `cargo test --workspace`; a workspace-wide `cargo check -j4 --workspace --tests` is allowed.
- Commit with conventional commits using the `dev-workflow-skills:1-git-commit` skill, staging by explicit path only. Several sessions may share this repo, so run `git status` before staging and never stage files you did not edit. If `crates/rocket-app/src/execution_service.rs` shows unrelated local edits, stage only your hunks with `git add -p`.
- UI: shadcn/ui primitives and `lucide-react` only, no raw `<button>`, `<input>`, `<select>`. `SingleLineEditor` for single-line variable-aware fields. Zustand: narrow selectors only.
- Auth is being worked on in another place (flow Auth node, "inherit from parent"). Do NOT touch `src/lib/flow-auth*.ts`, `src/components/flow/**` or `AUTH_NODE_TYPE_OPTIONS`. Adding an `oauth1` default inside the shared `authStateForType` is allowed because it is a pure addition.

## Review Focus

- A request file with an unknown method must keep that method through load, save and send, never fall back to GET (Task 1 test `unknown_method_survives_a_request_roundtrip`).
- A method that is not an HTTP token (spaces, slashes, newline) must be rejected at parse time, because it would otherwise become a header-injection or request-line-injection vector (Task 1 test `http_method_rejects_non_token_text`).
- `:id` must not rewrite `:idx`, the `:8080` port must never be treated as a parameter, `{{id}}` must never be corrupted by a `{id}` parameter, and a value containing `/` or `{{` must be percent-encoded rather than re-interpreted (Task 3 tests in `path_params.rs`).
- Query keys and values must resolve `{{var}}` on the backend and `enabled` must be preserved, so the runner and flows match the single send (Task 3 test `resolve_request_resolves_placeholders_in_query_params`).
- Selecting OAuth 1.0 must produce an `oauth1` auth state whose unknown persisted fields survive an edit, and an emptied text field must be removed rather than saved as `""` (Task 2 `OAuth1AuthEditor` tests).

---

## Task 1: Custom, TRACE and CONNECT methods end to end

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-shared/src/types.rs` (enum at lines 14-58, tests at about line 311)
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`map_method` at about line 652, its call in `execute` at about line 235, test `maps_all_http_methods` at about line 969)
- Modify: `crates/rocket-collection/src/contract/snapshot.rs` (`http_method_name` at line 83)
- Modify: `crates/rocket-infra/src/conversions/tests.rs` (add a round-trip test)
- Modify (mechanical, compiler-guided): every file that copies an `HttpMethod` out of a borrow, since the type stops being `Copy`. Known: `crates/rocket-app/src/execution_service.rs` (`method: input.method`), `crates/rocket-app/src/runner_sequence.rs` (`method: request.method`).
- Create: `src/lib/method-options.ts`, `src/lib/__tests__/method-options.test.ts`
- Create: `src/components/request/MethodSelect.tsx`, `src/components/request/__tests__/MethodSelect.test.tsx`
- Modify: `src/lib/tauri-api.ts` (line 14), `src/types/pane-types.ts` (line 384), `src/components/request/RequestPanel.tsx` (line 91 `METHODS`, lines 925-945 the method `Select`)

**Interfaces:**
- Produces (Rust):
  - `rocket_shared::types::HttpMethod` with variants `Get, Post, Put, Patch, Delete, Options, Head, Trace, Connect, Custom(String)`. It is `Clone` but no longer `Copy`. It serializes as a plain string (`"PURGE"`), deserializes through `FromStr`.
  - `rocket_shared::types::is_valid_method_token(s: &str) -> bool`.
  - `reqwest_executor::map_method(method: &HttpMethod) -> DomainResult<reqwest::Method>` (private to `rocket-infra`, was infallible).
- Produces (TS):
  - `STANDARD_METHODS: readonly string[]`, `isValidMethodToken(s: string): boolean`, `normalizeMethod(input: string): string | null`, `withCurrentMethod(options: readonly string[], current: string): string[]` in `src/lib/method-options.ts`.
  - `MethodSelect({ value, onChange }: { value: string; onChange: (method: string) => void })`.
  - `HttpMethod` in `tauri-api.ts` and `pane-types.ts` becomes `string`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Note that `http.method` is a free `string` there, which is why custom methods are legal in a request file.

- [ ] **Step 2: Write the failing shared tests**

In `crates/rocket-shared/src/types.rs`, inside the existing `#[cfg(test)] mod tests`, change the existing assertion `assert!(HttpMethod::from_str("INVALID").is_err());` in `http_method_from_string` to `assert!(HttpMethod::from_str("NOT A METHOD").is_err());` (`INVALID` is now a legal custom token). Then append:

```rust
    #[test]
    fn http_method_trace_and_connect_are_standard() {
        assert_eq!(HttpMethod::from_str("trace"), Ok(HttpMethod::Trace));
        assert_eq!(HttpMethod::from_str("CONNECT"), Ok(HttpMethod::Connect));
        assert_eq!(HttpMethod::Trace.to_string(), "TRACE");
        assert_eq!(HttpMethod::Connect.to_string(), "CONNECT");
    }

    #[test]
    fn http_method_custom_token_is_kept_as_written() {
        let purge = HttpMethod::from_str("PURGE").expect("valid token");
        assert_eq!(purge, HttpMethod::Custom("PURGE".into()));
        assert_eq!(purge.to_string(), "PURGE");
        // Methods are case-sensitive on the wire, so a custom token is not upper-cased.
        assert_eq!(
            HttpMethod::from_str("m-search").expect("valid token").to_string(),
            "m-search"
        );
        // A standard name in any case maps to the standard variant, as before.
        assert_eq!(HttpMethod::from_str("Get"), Ok(HttpMethod::Get));
    }

    #[test]
    fn http_method_rejects_non_token_text() {
        for bad in ["", "GET ME", "PU RGE", "A/B", "BAD\n", "BAD\r\nHost: x", "caf\u{e9}"] {
            assert!(
                HttpMethod::from_str(bad).is_err(),
                "{bad:?} must be rejected"
            );
        }
        assert!(HttpMethod::from_str(&"A".repeat(65)).is_err());
    }

    #[test]
    fn http_method_serializes_as_a_plain_string() {
        assert_eq!(serde_json::to_string(&HttpMethod::Get).expect("ser"), "\"GET\"");
        assert_eq!(
            serde_json::to_string(&HttpMethod::Custom("PURGE".into())).expect("ser"),
            "\"PURGE\""
        );
        assert_eq!(
            serde_json::from_str::<HttpMethod>("\"TRACE\"").expect("de"),
            HttpMethod::Trace
        );
        assert_eq!(
            serde_json::from_str::<HttpMethod>("\"PURGE\"").expect("de"),
            HttpMethod::Custom("PURGE".into())
        );
        assert!(serde_json::from_str::<HttpMethod>("\"BAD METHOD\"").is_err());
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-shared http_method`
Expected: FAIL to compile (`no variant named Trace`).

- [ ] **Step 4: Implement the new `HttpMethod`**

In `crates/rocket-shared/src/types.rs` replace the enum, `Display` and `FromStr` (lines 12-58) with:

```rust
/// An HTTP request method. The nine standard methods have their own variant, and any other
/// valid method token is kept as `Custom`, exactly as written, because methods are
/// case-sensitive on the wire. It serializes as a plain string so request files, IPC payloads
/// and scripts all keep exchanging method names as text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
    Trace,
    Connect,
    Custom(String),
}

/// True when `s` is a legal HTTP method token (RFC 9110 `token`, at most 64 characters).
/// Anything else, such as text with spaces or line breaks, must never reach the request line.
pub fn is_valid_method_token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Patch => write!(f, "PATCH"),
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Options => write!(f, "OPTIONS"),
            HttpMethod::Head => write!(f, "HEAD"),
            HttpMethod::Trace => write!(f, "TRACE"),
            HttpMethod::Connect => write!(f, "CONNECT"),
            HttpMethod::Custom(name) => write!(f, "{name}"),
        }
    }
}

impl FromStr for HttpMethod {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "GET" => Ok(HttpMethod::Get),
            "POST" => Ok(HttpMethod::Post),
            "PUT" => Ok(HttpMethod::Put),
            "PATCH" => Ok(HttpMethod::Patch),
            "DELETE" => Ok(HttpMethod::Delete),
            "OPTIONS" => Ok(HttpMethod::Options),
            "HEAD" => Ok(HttpMethod::Head),
            "TRACE" => Ok(HttpMethod::Trace),
            "CONNECT" => Ok(HttpMethod::Connect),
            _ if is_valid_method_token(s) => Ok(HttpMethod::Custom(s.to_string())),
            _ => Err(DomainError::InvalidInput(format!(
                "Invalid HTTP method: {s}"
            ))),
        }
    }
}

impl TryFrom<String> for HttpMethod {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<HttpMethod> for String {
    fn from(method: HttpMethod) -> Self {
        method.to_string()
    }
}
```

`DomainError` must implement `Display` for `serde(try_from)`; it is a `thiserror` type, so it does. If the compiler complains, check `crates/rocket-shared/src/error.rs`.

- [ ] **Step 5: Run the shared tests to verify they pass**

Run: `cargo test -j4 -p rocket-shared http_method`
Expected: PASS (all `http_method_*` tests, including the pre-existing ones).

- [ ] **Step 6: Write the failing executor tests**

In `crates/rocket-infra/src/reqwest_executor.rs`, in the `tests` module, change `maps_all_http_methods` to unwrap the new `Result` (`assert_eq!(map_method(&HttpMethod::Get).expect("map"), Method::GET);` and so on for the seven existing lines), and add:

```rust
    #[test]
    fn maps_trace_connect_and_custom_methods() {
        assert_eq!(map_method(&HttpMethod::Trace).expect("map"), Method::TRACE);
        assert_eq!(map_method(&HttpMethod::Connect).expect("map"), Method::CONNECT);
        assert_eq!(
            map_method(&HttpMethod::Custom("PURGE".into()))
                .expect("map")
                .as_str(),
            "PURGE"
        );
    }
```

Append a new module at the end of the file:

```rust
#[cfg(test)]
mod method_tests {
    use super::*;
    use rocket_shared::types::HttpMethod;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn sends_a_custom_method_verbatim() {
        let server = MockServer::start().await;
        Mock::given(method("PURGE"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let req = HttpRequest::new(
            HttpMethod::Custom("PURGE".into()),
            format!("{}/cache/x", server.uri()),
        );
        let response = ReqwestExecutor::new().execute(&req).await.expect("send");
        assert_eq!(response.status, 204);
    }

    #[tokio::test]
    async fn sends_a_trace_request() {
        let server = MockServer::start().await;
        Mock::given(method("TRACE"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let req = HttpRequest::new(HttpMethod::Trace, format!("{}/t", server.uri()));
        let response = ReqwestExecutor::new().execute(&req).await.expect("send");
        assert_eq!(response.status, 200);
    }

    #[test]
    fn connect_builds_a_request_without_error() {
        // CONNECT is not a tunnel feature here: the request is sent like any other and the
        // response is shown. A real tunnel handshake is out of scope.
        let client = Client::new();
        let built = client
            .request(
                map_method(&HttpMethod::Connect).expect("map"),
                "http://example.com:8080",
            )
            .build();
        assert!(built.is_ok());
    }
}
```

- [ ] **Step 7: Run to verify failure**

Run: `cargo test -j4 -p rocket-infra reqwest_executor`
Expected: FAIL to compile (`map_method` returns `Method`, not `Result`, and the `HttpMethod` match is not exhaustive).

- [ ] **Step 8: Implement `map_method` and its call**

Replace `map_method` (about line 652) with:

```rust
fn map_method(method: &rocket_shared::types::HttpMethod) -> DomainResult<Method> {
    use rocket_shared::types::HttpMethod::*;
    Ok(match method {
        Get => Method::GET,
        Post => Method::POST,
        Put => Method::PUT,
        Patch => Method::PATCH,
        Delete => Method::DELETE,
        Options => Method::OPTIONS,
        Head => Method::HEAD,
        Trace => Method::TRACE,
        Connect => Method::CONNECT,
        Custom(name) => Method::from_bytes(name.as_bytes()).map_err(|e| {
            DomainError::InvalidInput(format!("Invalid HTTP method {name}: {e}"))
        })?,
    })
}
```

In `execute`, change `let method = map_method(&request.method);` to `let method = map_method(&request.method)?;`.

- [ ] **Step 9: Fix every non-exhaustive match and every lost `Copy`**

Replace `http_method_name` in `crates/rocket-collection/src/contract/snapshot.rs` (line 83) with:

```rust
fn http_method_name(m: &HttpMethod) -> String {
    m.to_string()
}
```

and add inside its `mod tests`:

```rust
    #[test]
    fn http_method_name_covers_trace_connect_and_custom() {
        assert_eq!(http_method_name(&HttpMethod::Trace), "TRACE");
        assert_eq!(http_method_name(&HttpMethod::Connect), "CONNECT");
        assert_eq!(http_method_name(&HttpMethod::Custom("PURGE".into())), "PURGE");
    }
```

Then let the compiler list the remaining sites:

Run: `cargo check -j4 --workspace --tests`
Expected: errors of the kind `cannot move out of ... which is behind a shared reference` for `HttpMethod`. Fix each by cloning at the use site, for example in `crates/rocket-app/src/execution_service.rs` (`resolve_request`) change `method: input.method,` to `method: input.method.clone(),` and in `crates/rocket-app/src/runner_sequence.rs` change `method: request.method,` to `method: request.method.clone(),`. Do not add `Copy` back. Repeat until the check is clean.

- [ ] **Step 10: Write and run the persistence round-trip test**

Append to `crates/rocket-infra/src/conversions/tests.rs`:

```rust
#[test]
fn unknown_method_survives_a_request_roundtrip() {
    let yaml = r#"
info:
  name: Purge
  type: http
http:
  method: PURGE
  url: "https://cdn.example.com/x"
"#;
    let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
    let req = oc_http_request_to_request(oc);
    assert_eq!(req.method, HttpMethod::Custom("PURGE".into()));
    let back: OcHttpRequest = OcHttpRequest::from(req);
    assert_eq!(back.http.method, "PURGE");
}
```

Run: `cargo test -j4 -p rocket-infra unknown_method_survives_a_request_roundtrip`
Expected: PASS. If `OcHttpRequest::from(Request)` has a different name, use the conversion that `param_merge_roundtrip` or the surrounding tests already call (search `request_to_oc_http_request` in `conversions/request.rs`).

Run: `cargo test -j4 -p rocket-infra reqwest_executor method_tests`
Expected: PASS.

Run: `cargo test -j4 -p rocket-app execution_service`
Expected: PASS (the script `req.setMethod('PURGE')` path now parses; the existing "unrecognized method" test, if any, must use a non-token such as `'NOT A METHOD'`; update that literal if it fails).

- [ ] **Step 11: Write the failing frontend tests for the method helpers**

Create `src/lib/__tests__/method-options.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  isValidMethodToken,
  normalizeMethod,
  STANDARD_METHODS,
  withCurrentMethod,
} from '@/lib/method-options';

describe('method-options', () => {
  it('lists the nine standard methods', () => {
    expect(STANDARD_METHODS).toEqual([
      'GET',
      'POST',
      'PUT',
      'PATCH',
      'DELETE',
      'OPTIONS',
      'HEAD',
      'TRACE',
      'CONNECT',
    ]);
  });

  it('accepts HTTP tokens and rejects everything else', () => {
    expect(isValidMethodToken('PURGE')).toBe(true);
    expect(isValidMethodToken('M-SEARCH')).toBe(true);
    for (const bad of ['', 'GET ME', 'A/B', 'BAD\n', 'café', 'A'.repeat(65)]) {
      expect(isValidMethodToken(bad)).toBe(false);
    }
  });

  it('upper-cases standard names, keeps custom tokens as typed and returns null for junk', () => {
    expect(normalizeMethod(' trace ')).toBe('TRACE');
    expect(normalizeMethod('Purge')).toBe('Purge');
    expect(normalizeMethod('bad method')).toBeNull();
    expect(normalizeMethod('   ')).toBeNull();
  });

  it('adds the current method to the options only when it is not already there', () => {
    expect(withCurrentMethod(STANDARD_METHODS, 'GET')).toBe(STANDARD_METHODS);
    expect(withCurrentMethod(['GET'], 'PURGE')).toEqual(['GET', 'PURGE']);
  });
});
```

Create `src/components/request/__tests__/MethodSelect.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MethodSelect } from '../MethodSelect';

describe('MethodSelect', () => {
  it('shows a custom method that is not in the standard list', () => {
    render(<MethodSelect value='PURGE' onChange={vi.fn()} />);
    expect(screen.getByRole('combobox')).toHaveTextContent('PURGE');
  });

  it('commits a valid custom method typed into the custom field', () => {
    const onChange = vi.fn();
    render(<MethodSelect value='GET' onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Custom method' }));
    const input = screen.getByLabelText('Custom HTTP method');
    fireEvent.change(input, { target: { value: 'PURGE' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onChange).toHaveBeenCalledWith('PURGE');
  });

  it('does not commit text that is not a method token', () => {
    const onChange = vi.fn();
    render(<MethodSelect value='GET' onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Custom method' }));
    const input = screen.getByLabelText('Custom HTTP method');
    fireEvent.change(input, { target: { value: 'not valid' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onChange).not.toHaveBeenCalled();
    expect(input).toHaveAttribute('aria-invalid', 'true');
  });
});
```

- [ ] **Step 12: Run to verify failure**

Run: `yarn test method-options MethodSelect`
Expected: FAIL (modules not found).

- [ ] **Step 13: Implement the helpers and the component**

Create `src/lib/method-options.ts`:

```ts
// The standard HTTP methods offered in the method selector, in display order.
export const STANDARD_METHODS: readonly string[] = [
  'GET',
  'POST',
  'PUT',
  'PATCH',
  'DELETE',
  'OPTIONS',
  'HEAD',
  'TRACE',
  'CONNECT',
];

// RFC 9110 token characters. Mirrors `is_valid_method_token` in rocket-shared.
const METHOD_TOKEN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]{1,64}$/;

export function isValidMethodToken(value: string): boolean {
  return METHOD_TOKEN.test(value);
}

// Turns typed text into the method to store. Standard names are upper-cased, any other
// valid token is kept exactly as typed (methods are case-sensitive). Returns null for
// text that is not a method token.
export function normalizeMethod(input: string): string | null {
  const trimmed = input.trim();
  if (!isValidMethodToken(trimmed)) return null;
  const upper = trimmed.toUpperCase();
  return STANDARD_METHODS.includes(upper) ? upper : trimmed;
}

// Keeps the current method visible in the selector even when it is a custom one.
export function withCurrentMethod(options: readonly string[], current: string): string[] {
  return options.includes(current) ? (options as string[]) : [...options, current];
}
```

Create `src/components/request/MethodSelect.tsx`:

```tsx
import { Pencil } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
import { normalizeMethod, STANDARD_METHODS, withCurrentMethod } from '@/lib/method-options';
import { cn } from '@/lib/utils';

interface MethodSelectProps {
  value: string;
  onChange: (method: string) => void;
}

// Method picker for the URL bar: the standard methods plus a custom-method field.
export function MethodSelect({ value, onChange }: MethodSelectProps) {
  const [customOpen, setCustomOpen] = useState(false);
  const [customText, setCustomText] = useState('');
  const [invalid, setInvalid] = useState(false);

  const closeCustom = () => {
    setCustomOpen(false);
    setCustomText('');
    setInvalid(false);
  };

  const commitCustom = () => {
    const method = normalizeMethod(customText);
    if (!method) {
      setInvalid(true);
      return;
    }
    onChange(method);
    closeCustom();
  };

  return (
    <div className='flex items-center gap-1'>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger className={cn('h-8 w-28 text-sm font-semibold', METHOD_TEXT_COLOR[value])}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {withCurrentMethod(STANDARD_METHODS, value).map((m) => (
            <SelectItem
              key={m}
              value={m}
              className={cn('text-sm font-semibold', METHOD_TEXT_COLOR[m])}
            >
              {m}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {customOpen ? (
        <Input
          autoFocus
          aria-label='Custom HTTP method'
          aria-invalid={invalid}
          placeholder='PURGE'
          value={customText}
          onChange={(e) => {
            setCustomText(e.target.value);
            setInvalid(false);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commitCustom();
            if (e.key === 'Escape') closeCustom();
          }}
          className={cn('h-8 w-28 text-xs font-mono', invalid && 'border-destructive')}
        />
      ) : (
        <Button
          variant='ghost'
          size='icon'
          className='h-8 w-8 text-muted-foreground'
          aria-label='Custom method'
          title='Use a custom HTTP method'
          onClick={() => setCustomOpen(true)}
        >
          <Pencil className='h-3.5 w-3.5' />
        </Button>
      )}
    </div>
  );
}
```

- [ ] **Step 14: Wire the type widening and the component into the panel**

In `src/lib/tauri-api.ts` line 14 replace the union with `export type HttpMethod = string;` and add the comment `// Standard methods or any custom method token, as the backend serializes it.` above it. Do the same for `src/types/pane-types.ts` line 384. In `src/components/request/RequestPanel.tsx`: delete the `METHODS` constant (line 91), add `import { MethodSelect } from './MethodSelect';`, and replace the `<Select value={request.method} ...>...</Select>` block in `urlBar` (lines 925-945) with:

```tsx
        <MethodSelect
          value={request.method}
          onChange={(method) => updateRequest(tab.id, { method })}
        />
```

Remove the `METHOD_TEXT_COLOR` import from `RequestPanel.tsx` only if `yarn check` reports it unused. In `handleCurlImport` leave `(parsed.method as HttpMethod) || 'GET'` as it is: the cast is now a no-op.

- [ ] **Step 15: Run the frontend checks**

Run: `yarn test method-options MethodSelect RequestPanel`
Expected: PASS.

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS. If `RequestPanel.test.tsx` queried the method `Select` by role and now finds two comboboxes or an extra button, adjust the query to `getAllByRole('combobox')[0]` rather than changing the component.

- [ ] **Step 16: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path: the Rust files you edited (`git status` lists them), `src/lib/method-options.ts`, `src/lib/__tests__/method-options.test.ts`, `src/components/request/MethodSelect.tsx`, `src/components/request/__tests__/MethodSelect.test.tsx`, `src/components/request/RequestPanel.tsx`, `src/lib/tauri-api.ts`, `src/types/pane-types.ts`. Suggested subject: `feat(http): support TRACE, CONNECT and custom request methods`.

---

## Task 2: SPARQL body option and OAuth 1.0 selectable with an editor

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/types/pane-types.ts` (line 264 `BodyState.mode`)
- Modify: `src/lib/tauri-api.ts` (line 22 `BodyMode`)
- Modify: `src/components/request/RequestPanel.tsx` (`BODY_MODES` line 93, `BASE_AUTH_TYPES` line 103, `handleAuthTypeChange` line 520)
- Modify: `src/components/request/BodyEditor.tsx` (the Monaco branch at about line 70)
- Modify: `src/lib/curl-generator.ts` (`contentTypeFor` line 19, `buildBodyParts` line 114)
- Modify: `src/lib/auth-type-options.ts`, `src/lib/auth-type-defaults.ts`
- Modify: `src/components/collections/CollectionOverviewTab.tsx` (`COLLECTION_AUTH_TYPES` line 89, `handleAuthTypeChange` line 276)
- Create: `src/components/request/OAuth1AuthEditor.tsx`
- Modify: `src/components/request/AuthEditor.tsx` (the oauth1 info card at line 217)
- Test: `src/lib/__tests__/auth-type-options.test.ts`, `src/lib/__tests__/auth-type-defaults.test.ts`, `src/lib/__tests__/curl-generator.test.ts`, `src/lib/__tests__/execute-request.test.ts`, `src/components/request/__tests__/BodyEditor.test.tsx`, `src/components/request/__tests__/AuthEditor.test.tsx`, `src/components/request/__tests__/OAuth1AuthEditor.test.tsx` (create)

**Interfaces:**
- Consumes: backend `BodyMode::Sparql` (`Content-Type: application/sparql-query` unless the user set one) and `Auth::OAuth1(Box<OAuth1Auth>)`, already implemented. The wire shape is `{ authType: 'o-auth1', ...fields }` (already produced by `toApiAuth` and `toPersistedAuth`).
- Produces:
  - `BodyState['mode']` and `BodyMode` include `'sparql'`.
  - `OAUTH1_OPTION: AuthTypeOption` exported from `auth-type-options.ts`; `withCurrentAuthType` keeps only NTLM as a read-only extra.
  - `authStateForType('oauth1', prev)` returns `{ authType: 'oauth1', oauth1: prev.oauth1 ?? { signatureMethod: 'HMAC-SHA1', placement: 'header' } }`.
  - `OAuth1AuthEditor({ value, onChange, variableContext, onNavigateToSource })` where `value: Record<string, unknown>`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`, in particular the `AuthOAuth1` fields and `RawBody` types.

- [ ] **Step 2: Write the failing SPARQL tests**

Append to `src/lib/__tests__/curl-generator.test.ts` inside the top-level `describe('generateCurlCommand', ...)`:

```ts
  it('sends a sparql body as data with the sparql content type', () => {
    const cmd = generateCurlCommand(
      baseResolved({
        body: { mode: 'sparql', content: 'SELECT * WHERE { ?s ?p ?o }' },
      }),
      'POST',
    );
    expect(cmd).toContain("--data 'SELECT * WHERE { ?s ?p ?o }'");
    expect(cmd).toContain('Content-Type: application/sparql-query');
  });
```

Append to `src/components/request/__tests__/BodyEditor.test.tsx` inside `describe('BodyEditor', ...)`:

```tsx
  it('renders the code editor for sparql mode', () => {
    const body = makeBody({ mode: 'sparql', content: 'SELECT * WHERE { ?s ?p ?o }' });
    wrap(<BodyEditor body={body} onChange={vi.fn()} />);
    expect(screen.getByTestId('monaco')).toBeInTheDocument();
  });
```

Append to `src/lib/__tests__/execute-request.test.ts` (extend the import to include `toApiBody`):

```ts
describe('toApiBody', () => {
  it('passes a sparql body through with its content', () => {
    expect(
      toApiBody({ mode: 'sparql', content: 'ASK { ?s ?p ?o }', formData: [] }, (s) => s),
    ).toEqual({ mode: 'sparql', content: 'ASK { ?s ?p ?o }' });
  });
});
```

(This last test already passes at runtime and pins the contract; `yarn tsc --noEmit` is what fails until the type is widened.)

- [ ] **Step 3: Run to verify failure**

Run: `yarn test curl-generator BodyEditor`
Expected: the two new component and curl tests FAIL.

- [ ] **Step 4: Implement the SPARQL option**

- `src/types/pane-types.ts` line 264: `mode: 'none' | 'json' | 'xml' | 'text' | 'sparql' | 'formdata' | 'formurlencoded' | 'binary';`
- `src/lib/tauri-api.ts` line 22: add `'sparql'` after `'text'` in `BodyMode`.
- `RequestPanel.tsx` `BODY_MODES`: insert `{ label: 'SPARQL', value: 'sparql' },` after the Text entry.
- `BodyEditor.tsx`: change `(body.mode === 'json' || body.mode === 'xml' || body.mode === 'text')` to `(body.mode === 'json' || body.mode === 'xml' || body.mode === 'text' || body.mode === 'sparql')`.
- `curl-generator.ts`: in `contentTypeFor` add `case 'sparql': return 'application/sparql-query';` and in `buildBodyParts` add `case 'sparql':` directly under `case 'text':` so it shares the raw-data branch.

- [ ] **Step 5: Run to verify the SPARQL tests pass**

Run: `yarn test curl-generator BodyEditor execute-request && yarn tsc --noEmit`
Expected: PASS.

- [ ] **Step 6: Write the failing OAuth 1.0 tests**

Replace the body of `src/lib/__tests__/auth-type-options.test.ts` `it('adds ntlm and oauth1 only while ...')` with:

```ts
  it('adds ntlm only while the request already uses it', () => {
    expect(withCurrentAuthType(base, 'ntlm').map((o) => o.value)).toEqual([
      'none',
      'basic',
      'ntlm',
    ]);
  });
```

and in `it('leaves the list alone for other types', ...)` add `expect(withCurrentAuthType(base, 'oauth1')).toBe(base);` (OAuth 1.0 is now a normal option, listed by the callers).

Append to `src/lib/__tests__/auth-type-defaults.test.ts`:

```ts
  it('defaults oauth1 to HMAC-SHA1 with the header placement and keeps existing fields', () => {
    expect(authStateForType('oauth1', none).oauth1).toEqual({
      signatureMethod: 'HMAC-SHA1',
      placement: 'header',
    });
    const prev = { authType: 'oauth1', oauth1: { consumerKey: 'ck', extra: 1 } } as const;
    expect(authStateForType('oauth1', prev).oauth1).toEqual({ consumerKey: 'ck', extra: 1 });
  });
```

Create `src/components/request/__tests__/OAuth1AuthEditor.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

// CodeMirror does not run in jsdom, so the variable-aware field is replaced by a plain input.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    'aria-label': label,
  }: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => <input aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} />,
}));

import { OAuth1AuthEditor } from '../OAuth1AuthEditor';

describe('OAuth1AuthEditor', () => {
  it('edits a field and keeps the fields it does not know', () => {
    const onChange = vi.fn();
    render(
      <OAuth1AuthEditor
        value={{ consumerKey: 'ck', signatureMethod: 'HMAC-SHA1', privateKey: { type: 'text' } }}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByLabelText('Consumer secret'), { target: { value: 's3' } });
    expect(onChange).toHaveBeenLastCalledWith({
      consumerKey: 'ck',
      signatureMethod: 'HMAC-SHA1',
      privateKey: { type: 'text' },
      consumerSecret: 's3',
    });
  });

  it('removes a field when it is emptied instead of saving an empty string', () => {
    const onChange = vi.fn();
    render(<OAuth1AuthEditor value={{ consumerKey: 'ck', realm: 'r' }} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Realm'), { target: { value: '' } });
    expect(onChange).toHaveBeenLastCalledWith({ consumerKey: 'ck' });
  });

  it('toggles the body hash flag', () => {
    const onChange = vi.fn();
    render(<OAuth1AuthEditor value={{}} onChange={onChange} />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Include body hash' }));
    expect(onChange).toHaveBeenLastCalledWith({ includeBodyHash: true });
  });
});
```

In `src/components/request/__tests__/AuthEditor.test.tsx` replace the last test (`shows a read-only note for oauth1 ...`) with:

```tsx
  it('shows the oauth1 editor', () => {
    render(
      <AuthEditor
        auth={{ authType: 'oauth1', oauth1: { consumerKey: 'ck' } }}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByLabelText('Consumer key')).toHaveValue('ck');
  });
```

- [ ] **Step 7: Run to verify failure**

Run: `yarn test auth-type auth-type-defaults OAuth1AuthEditor AuthEditor`
Expected: FAIL (`OAuth1AuthEditor` not found, `authStateForType('oauth1')` has no `oauth1`, the options test sees `oauth1` appended).

- [ ] **Step 8: Implement OAuth 1.0 selection and the editor**

`src/lib/auth-type-options.ts`: replace `READ_ONLY_OPTIONS` and add the shared option:

```ts
// OAuth 1.0 has an editor, so callers list it like any other type.
export const OAUTH1_OPTION: AuthTypeOption = { label: 'OAuth 1.0', value: 'oauth1' };

// Auth types that can be kept and sent but not picked from the list. They show up in the
// selector only while the request already uses one, so its current value always has a label.
const READ_ONLY_OPTIONS: AuthTypeOption[] = [{ label: 'NTLM', value: 'ntlm' }];
```

`src/lib/auth-type-defaults.ts`: add inside `authStateForType`, before `return next;`:

```ts
  if (authType === 'oauth1')
    next.oauth1 = prev.oauth1 ?? { signatureMethod: 'HMAC-SHA1', placement: 'header' };
```

`RequestPanel.tsx`: add `import { authStateForType } from '@/lib/auth-type-defaults';` and `OAUTH1_OPTION` to the existing `@/lib/auth-type-options` import. Insert `OAUTH1_OPTION,` into `BASE_AUTH_TYPES` after the OAuth 2.0 entry (turn the array literal element into `OAUTH1_OPTION`). Replace the whole `handleAuthTypeChange` callback (line 520 to the closing `[tab.id, updateRequest, request.auth],`) with:

```tsx
  const handleAuthTypeChange = useCallback(
    (authType: AuthState['authType']) => {
      updateRequest(tab.id, { auth: authStateForType(authType, request.auth) });
    },
    [tab.id, updateRequest, request.auth],
  );
```

`CollectionOverviewTab.tsx`: add `OAUTH1_OPTION` to the `@/lib/auth-type-options` import, add it after the OAuth 2.0 entry of `COLLECTION_AUTH_TYPES`, import `authStateForType`, and replace the body of its `handleAuthTypeChange` (line 276) with:

```tsx
  const handleAuthTypeChange = useCallback(
    (authType: AuthState['authType']) => {
      setAuth(authStateForType(authType, auth));
      setIsDirty(true);
    },
    [auth],
  );
```

Create `src/components/request/OAuth1AuthEditor.tsx`:

```tsx
import { SingleLineEditor } from '@/components/editor';
import { Card, CardContent } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';

// RSA-* methods are not offered: the backend fails a request that asks for one.
const SIGNATURE_METHODS = ['HMAC-SHA1', 'HMAC-SHA256', 'HMAC-SHA512', 'PLAINTEXT'];
const PLACEMENTS = [
  { value: 'header', label: 'Authorization header' },
  { value: 'query', label: 'Query parameters' },
  { value: 'body', label: 'Form body' },
];

const TEXT_FIELDS: { key: string; label: string; secret?: boolean }[] = [
  { key: 'consumerKey', label: 'Consumer key' },
  { key: 'consumerSecret', label: 'Consumer secret', secret: true },
  { key: 'accessToken', label: 'Access token' },
  { key: 'accessTokenSecret', label: 'Access token secret', secret: true },
  { key: 'realm', label: 'Realm' },
  { key: 'callbackUrl', label: 'Callback URL' },
  { key: 'verifier', label: 'Verifier' },
];

interface OAuth1AuthEditorProps {
  // The persisted OAuth 1.0 fields as stored. Fields this editor does not show are kept.
  value: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

export function OAuth1AuthEditor({
  value,
  onChange,
  variableContext,
  onNavigateToSource,
}: OAuth1AuthEditorProps) {
  // An emptied field is removed, so the file never holds an empty string where the
  // spec has an optional value.
  const setField = (key: string, next: unknown) => {
    const merged = { ...value, [key]: next };
    if (next === '' || next === undefined) delete merged[key];
    onChange(merged);
  };
  const text = (key: string) => (typeof value[key] === 'string' ? (value[key] as string) : '');
  const method = text('signatureMethod') || 'HMAC-SHA1';
  const methodOptions = SIGNATURE_METHODS.includes(method)
    ? SIGNATURE_METHODS
    : [...SIGNATURE_METHODS, method];
  const placement = text('placement') || 'header';

  return (
    <Card>
      <CardContent className='space-y-3 p-4'>
        {TEXT_FIELDS.map((f) => (
          <div key={f.key} className='space-y-1.5'>
            <Label className='text-xs text-muted-foreground'>{f.label}</Label>
            <SingleLineEditor
              aria-label={f.label}
              placeholder={f.label}
              isSecret={f.secret}
              className='text-sm'
              value={text(f.key)}
              onChange={(next) => setField(f.key, next)}
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
          </div>
        ))}
        <div className='grid grid-cols-2 gap-2'>
          <div className='space-y-1.5'>
            <Label className='text-xs text-muted-foreground'>Signature method</Label>
            <Select value={method} onValueChange={(next) => setField('signatureMethod', next)}>
              <SelectTrigger aria-label='Signature method' className='h-8 text-xs'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {methodOptions.map((m) => (
                  <SelectItem key={m} value={m} className='text-sm'>
                    {m}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className='space-y-1.5'>
            <Label className='text-xs text-muted-foreground'>Add signature to</Label>
            <Select value={placement} onValueChange={(next) => setField('placement', next)}>
              <SelectTrigger aria-label='Signature placement' className='h-8 text-xs'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {PLACEMENTS.map((p) => (
                  <SelectItem key={p.value} value={p.value} className='text-sm'>
                    {p.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
        <div className='flex items-center gap-2'>
          <Checkbox
            checked={value.includeBodyHash === true}
            onCheckedChange={(checked) => setField('includeBodyHash', checked === true)}
            aria-label='Include body hash'
          />
          <span className='text-xs text-muted-foreground'>Include body hash</span>
        </div>
      </CardContent>
    </Card>
  );
}
```

In `AuthEditor.tsx` add `import { OAuth1AuthEditor } from './OAuth1AuthEditor';` and replace the `auth.authType === 'oauth1'` info card (lines 217-227) with:

```tsx
      {auth.authType === 'oauth1' && (
        <OAuth1AuthEditor
          value={auth.oauth1 ?? {}}
          onChange={(oauth1) => onChange({ ...auth, oauth1 })}
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      )}
```

Remove the now-wrong comment on `AuthState.oauth1` in `pane-types.ts` ("OAuth 1.0 has no editor yet"), replacing it with `// Persisted OAuth 1.0 fields as stored. Unknown fields are kept on save.`.

- [ ] **Step 9: Run the checks**

Run: `yarn test auth-type auth-type-defaults OAuth1AuthEditor AuthEditor curl-generator BodyEditor RequestPanel CollectionOverview`
Expected: PASS.

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, staging by explicit path (every file listed under Files for this task). Suggested subject: `feat(ui): add the SPARQL body mode and an OAuth 1.0 editor`.

---

## Task 3: Path and query parameter resolution in the backend

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-http/src/path_params.rs`
- Modify: `crates/rocket-http/src/lib.rs` (module and re-export)
- Modify: `crates/rocket-app/src/execution_service.rs` (`resolve_request`, lines 664-735; tests near line 3218)
- Modify: `src/lib/url-params.ts` (add `applyPathParams`), `src/lib/__tests__/url-params.test.ts`
- Modify: `src/lib/execute-request.ts` (lines 243-246 and `ResolvedRequestFields`), `src/lib/__tests__/execute-request.test.ts`
- Modify: `src/lib/curl-generator.ts`, `src/lib/__tests__/curl-generator.test.ts`
- Modify: `src/components/request/LoadTestDialog.tsx`, `src/components/request/__tests__/LoadTestDialog.test.tsx`
- Modify: `src/lib/tauri-api.ts` (`Request` interface, line 122), `src/lib/pane-utils.ts` (line 39), `src/lib/request-save-mapper.ts`, `src/lib/__tests__/pane-utils.test.ts`, `src/lib/__tests__/request-save-mapper.test.ts`

**Interfaces:**
- Consumes: `rocket_shared::types::{PathParam, QueryParam}`, `rocket_environment::resolve(&str, &HashMap<String,String>)` (its `.output` is the resolved text, as used throughout `resolve_request`).
- Produces:
  - `rocket_http::path_params::substitute_path_params(url: &str, params: &[PathParam]) -> String`, re-exported as `rocket_http::substitute_path_params`. Values must already be variable-resolved; entries with an empty name or empty value are ignored.
  - `resolve_request` now returns an `HttpRequest` whose `url` has path parameters substituted and whose `query_params` have `{{var}}` resolved in `key` and `value`.
  - TS: `applyPathParams(url: string, params: { name: string; value: string }[]): string` (display only), `ResolvedRequestFields.pathParams: { name: string; value: string }[]`, `Request.pathParams?: PathParam[]` in `tauri-api.ts`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`: a `HttpRequestParam` has `type: "query" | "path"`, and path parameters live in the same `params` list as query parameters.

- [ ] **Step 2: Write the failing substitution tests**

Create `crates/rocket-http/src/path_params.rs` with only the test module, so the tests fail to compile:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, value: &str) -> PathParam {
        PathParam {
            name: name.into(),
            value: value.into(),
            description: None,
        }
    }

    #[test]
    fn replaces_every_occurrence_of_a_colon_param() {
        let url = substitute_path_params("https://h.test/a/:id/b/:id", &[p("id", "7")]);
        assert_eq!(url, "https://h.test/a/7/b/7");
    }

    #[test]
    fn a_param_never_rewrites_a_longer_name() {
        let url = substitute_path_params("https://h.test/a/:idx/:id", &[p("id", "7")]);
        assert_eq!(url, "https://h.test/a/:idx/7");
    }

    #[test]
    fn keeps_a_name_followed_by_a_dot_suffix() {
        let url = substitute_path_params("https://h.test/files/:name.json", &[p("name", "r")]);
        assert_eq!(url, "https://h.test/files/r.json");
    }

    #[test]
    fn never_treats_a_port_as_a_param() {
        let url = substitute_path_params("http://localhost:8080/u/:id", &[p("8080", "x"), p("id", "7")]);
        assert_eq!(url, "http://localhost:8080/u/7");
        let no_scheme = substitute_path_params("localhost:3000/u/:id", &[p("3000", "x"), p("id", "7")]);
        assert_eq!(no_scheme, "localhost:3000/u/7");
    }

    #[test]
    fn leaves_the_query_and_fragment_alone() {
        let url = substitute_path_params("https://h.test/a/:id?x=:id#:id", &[p("id", "7")]);
        assert_eq!(url, "https://h.test/a/7?x=:id#:id");
    }

    #[test]
    fn replaces_the_brace_form_but_never_inside_a_double_brace_variable() {
        let url = substitute_path_params("https://h.test/{id}/{{id}}", &[p("id", "7")]);
        assert_eq!(url, "https://h.test/7/{{id}}");
    }

    #[test]
    fn percent_encodes_the_value_so_it_cannot_add_segments_or_variables() {
        let url = substitute_path_params(
            "https://h.test/a/:id",
            &[p("id", "x/../{{secret}} y?z#w")],
        );
        assert_eq!(url, "https://h.test/a/x%2F..%2F%7B%7Bsecret%7D%7D%20y%3Fz%23w");
    }

    #[test]
    fn a_param_without_a_value_or_a_name_is_ignored() {
        let url = substitute_path_params("https://h.test/a/:id", &[p("id", ""), p("", "x")]);
        assert_eq!(url, "https://h.test/a/:id");
    }

    #[test]
    fn a_value_that_looks_like_another_param_is_not_substituted_again() {
        let url = substitute_path_params("https://h.test/:a/:b", &[p("a", ":b"), p("b", "2")]);
        assert_eq!(url, "https://h.test/%3Ab/2");
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -j4 -p rocket-http path_params`
Expected: FAIL to compile (`substitute_path_params` not found; the module is not declared yet).

- [ ] **Step 4: Implement `substitute_path_params`**

Prepend to `crates/rocket-http/src/path_params.rs`:

```rust
//! Path parameter substitution for a resolved request URL.
//!
//! Only the path part of the URL is rewritten. A `:name` parameter must start a path segment,
//! so a port such as `:8080` is never read as a parameter. A `{name}` parameter may appear
//! anywhere in a segment, but never inside a `{{variable}}`. Values are percent-encoded, so a
//! value cannot add path segments, a query string or a variable.

use rocket_shared::types::PathParam;

/// Replaces `:name` and `{name}` path parameters in `url` with their percent-encoded values.
/// `params` values must already have their `{{variables}}` resolved. A parameter with an empty
/// name or an empty value is ignored, so its placeholder stays visible in the URL.
pub fn substitute_path_params(url: &str, params: &[PathParam]) -> String {
    let usable: Vec<(&str, String)> = params
        .iter()
        .filter(|p| !p.name.is_empty() && !p.value.is_empty())
        .map(|p| (p.name.as_str(), urlencoding::encode(&p.value).into_owned()))
        .collect();
    if usable.is_empty() {
        return url.to_string();
    }
    let (start, end) = path_range(url);
    let rewritten = url[start..end]
        .split('/')
        .map(|segment| rewrite_segment(segment, &usable))
        .collect::<Vec<_>>()
        .join("/");
    format!("{}{}{}", &url[..start], rewritten, &url[end..])
}

fn is_delimiter(c: char) -> bool {
    matches!(c, '/' | '?' | '#')
}

/// Byte range of the path inside `url`: after the scheme and authority, before `?` or `#`.
fn path_range(url: &str) -> (usize, usize) {
    let after_scheme = url
        .find("://")
        .filter(|i| !url[..*i].contains(is_delimiter))
        .map_or(0, |i| i + 3);
    let start = url[after_scheme..]
        .find(is_delimiter)
        .map_or(url.len(), |i| after_scheme + i);
    let end = url[start..]
        .find(|c| matches!(c, '?' | '#'))
        .map_or(url.len(), |i| start + i);
    (start, end)
}

fn rewrite_segment(segment: &str, params: &[(&str, String)]) -> String {
    let mut out = segment.to_string();
    if let Some(rest) = segment.strip_prefix(':') {
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if let Some((_, encoded)) = params.iter().find(|(name, _)| *name == &rest[..name_len]) {
            out = format!("{encoded}{}", &rest[name_len..]);
        }
    }
    for (name, encoded) in params {
        out = replace_braced(&out, name, encoded);
    }
    out
}

/// Replaces `{name}` unless it is the inside of a `{{name}}` variable.
fn replace_braced(segment: &str, name: &str, encoded: &str) -> String {
    let needle = format!("{{{name}}}");
    let mut out = String::with_capacity(segment.len());
    let mut from = 0;
    while let Some(offset) = segment[from..].find(&needle) {
        let at = from + offset;
        let after = at + needle.len();
        out.push_str(&segment[from..at]);
        if segment[..at].ends_with('{') || segment[after..].starts_with('}') {
            out.push_str(&needle);
        } else {
            out.push_str(encoded);
        }
        from = after;
    }
    out.push_str(&segment[from..]);
    out
}
```

In `crates/rocket-http/src/lib.rs` add `pub mod path_params;` (alphabetical, after `pub mod oauth2;`) and `pub use path_params::substitute_path_params;`.

- [ ] **Step 5: Run to verify the substitution tests pass**

Run: `cargo test -j4 -p rocket-http path_params`
Expected: PASS (9 tests). If `a_value_that_looks_like_another_param_is_not_substituted_again` fails, the colon rewrite is being applied twice: it must run once per segment.

- [ ] **Step 6: Write the failing `resolve_request` tests**

In `crates/rocket-app/src/execution_service.rs`, in the `tests` module right after `resolve_request_handles_hyphenated_variable_names` (about line 3240), add:

```rust
    #[tokio::test]
    async fn resolve_request_resolves_placeholders_in_query_params() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("token", "abc 123"));
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let mut input = sample_input("https://api.example.com/items", Some("dev"));
        input.query_params = vec![
            QueryParam {
                key: "{{token}}-k".into(),
                value: "{{token}}".into(),
                enabled: true,
                description: None,
            },
            QueryParam {
                key: "off".into(),
                value: "{{token}}".into(),
                enabled: false,
                description: None,
            },
        ];
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert_eq!(resolved.query_params[0].key, "abc 123-k");
        assert_eq!(resolved.query_params[0].value, "abc 123");
        assert!(resolved.query_params[0].enabled);
        assert!(!resolved.query_params[1].enabled, "enabled must be preserved");
        assert_eq!(resolved.query_params[1].value, "abc 123");
    }

    #[tokio::test]
    async fn resolve_request_substitutes_path_params_with_resolved_values() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("userId", "42"));
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let mut input = sample_input(
            "https://api.example.com/users/:id/orders/:id/{kind}",
            Some("dev"),
        );
        input.path_params = vec![
            rocket_shared::types::PathParam {
                name: "id".into(),
                value: "{{userId}}".into(),
                description: None,
            },
            rocket_shared::types::PathParam {
                name: "kind".into(),
                value: "a b".into(),
                description: None,
            },
        ];
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert_eq!(resolved.url, "https://api.example.com/users/42/orders/42/a%20b");
    }

    #[tokio::test]
    async fn resolve_request_keeps_a_path_param_placeholder_when_the_value_is_empty() {
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let mut input = sample_input("https://api.example.com/users/:id", None);
        input.path_params = vec![rocket_shared::types::PathParam {
            name: "id".into(),
            value: String::new(),
            description: None,
        }];
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert_eq!(resolved.url, "https://api.example.com/users/:id");
    }
```

- [ ] **Step 7: Run to verify failure**

Run: `cargo test -j4 -p rocket-app resolve_request_resolves_placeholders_in_query_params resolve_request_substitutes_path_params resolve_request_keeps_a_path_param`
Expected: the first two FAIL (the query value stays `{{token}}`, the URL keeps `:id`), the third PASSES already (it pins the current, correct behavior).

- [ ] **Step 8: Implement the backend resolution**

In `resolve_request` (`execution_service.rs`), replace the line `let resolved_url = resolve(&input.url, &vars).output;` and add the two derived values:

```rust
        let resolved_url = resolve(&input.url, &vars).output;
        // Path parameter values may hold {{placeholders}}, so they resolve before they are
        // substituted. The substitution runs on the resolved URL, so a placeholder in the URL
        // itself never swallows a parameter.
        let resolved_path_params: Vec<rocket_shared::types::PathParam> = input
            .path_params
            .iter()
            .map(|p| rocket_shared::types::PathParam {
                name: p.name.clone(),
                value: resolve(&p.value, &vars).output,
                description: None,
            })
            .collect();
        let resolved_url =
            rocket_http::substitute_path_params(&resolved_url, &resolved_path_params);
        // Query keys and values resolve like headers do, so a runner step or a flow node sends
        // the same query string as the single send.
        let resolved_query_params: Vec<QueryParam> = input
            .query_params
            .iter()
            .map(|q| QueryParam {
                key: resolve(&q.key, &vars).output,
                value: resolve(&q.value, &vars).output,
                enabled: q.enabled,
                description: q.description.clone(),
            })
            .collect();
```

and in the returned `HttpRequest` change `query_params: input.query_params.clone(),` to `query_params: resolved_query_params,`.

- [ ] **Step 9: Run to verify the backend passes**

Run: `cargo test -j4 -p rocket-app resolve_request`
Expected: PASS (new and existing `resolve_request_*` tests).

Run: `cargo check -j4 -p rocket-app -p rocket-http`
Expected: PASS.

- [ ] **Step 10: Write the failing frontend tests**

Append to `src/lib/__tests__/url-params.test.ts` (extend its import with `applyPathParams`):

```ts
describe('applyPathParams', () => {
  const p = (name: string, value: string) => ({ name, value });

  it('replaces every occurrence and encodes the value', () => {
    expect(applyPathParams('https://h.test/a/:id/b/:id', [p('id', 'x y')])).toBe(
      'https://h.test/a/x%20y/b/x%20y',
    );
  });

  it('does not rewrite a longer name, a port, the query or a double-brace variable', () => {
    expect(applyPathParams('http://localhost:8080/a/:idx/:id', [p('id', '7')])).toBe(
      'http://localhost:8080/a/:idx/7',
    );
    expect(applyPathParams('https://h.test/a?x=:id', [p('id', '7')])).toBe('https://h.test/a?x=:id');
    expect(applyPathParams('https://h.test/{id}/{{id}}', [p('id', '7')])).toBe(
      'https://h.test/7/{{id}}',
    );
  });

  it('ignores params without a value', () => {
    expect(applyPathParams('https://h.test/:id', [p('id', '')])).toBe('https://h.test/:id');
  });
});
```

Append to `src/lib/__tests__/execute-request.test.ts` inside `describe('resolveRequestFieldsForPath', ...)`:

```ts
  it('leaves path params in the url and returns their resolved values for the backend', async () => {
    const request = {
      ...baseRequest(),
      url: '{{baseUrl}}/users/:id',
      pathParams: [
        { id: 'p1', key: 'id', value: '{{baseUrl}}', enabled: true },
        { id: 'p2', key: 'off', value: 'x', enabled: false },
      ],
    };
    const resolved = await resolveRequestFieldsForPath('demo', 'ping.yml', request);
    expect(resolved.url).toBe('https://collection.example/users/:id');
    expect(resolved.pathParams).toEqual([{ name: 'id', value: 'https://collection.example' }]);
  });
```

In `src/lib/__tests__/curl-generator.test.ts` add `pathParams: [],` to `baseResolved` and this test:

```ts
  it('substitutes path params for display', () => {
    const cmd = generateCurlCommand(
      baseResolved({
        url: 'https://api.example.com/users/:id',
        pathParams: [{ name: 'id', value: '7' }],
      }),
      'GET',
    );
    expect(cmd).toBe('curl -X GET https://api.example.com/users/7');
  });
```

In `src/components/request/__tests__/LoadTestDialog.test.tsx` add `pathParams: [{ name: 'id', value: '7' }],` to the mocked `resolveRequestFields` return and this test:

```tsx
  it('forwards the resolved path params to runLoadTest', async () => {
    (runLoadTest as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      totalRequests: 1,
      succeeded: 1,
      failed: 0,
      failedTransport: 0,
      failedStatus: 0,
      minLatencyMs: 1,
      avgLatencyMs: 1,
      p50LatencyMs: 1,
      p95LatencyMs: 1,
      p99LatencyMs: 1,
      maxLatencyMs: 1,
      requestsPerSecond: 1,
      totalDurationMs: 1,
    });
    render(<LoadTestDialog open onOpenChange={noop} request={makeRequest()} tabId='t1' />);
    fireEvent.click(screen.getByRole('button', { name: /^run$/i }));
    await waitFor(() => expect(runLoadTest).toHaveBeenCalledTimes(1));
    const inputArg = (runLoadTest as unknown as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(inputArg.pathParams).toEqual([{ name: 'id', value: '7' }]);
  });
```

Append to `src/lib/__tests__/pane-utils.test.ts`, and add `mapApiRequestToState` to its existing `from '../pane-utils'` import list:

```ts
describe('mapApiRequestToState path params', () => {
  it('restores saved path param values by name', () => {
    const state = mapApiRequestToState({
      uid: 'u',
      name: 'n',
      method: 'GET',
      url: 'https://h.test/users/:id/:other',
      headers: [],
      auth: { authType: 'none' },
      pathParams: [{ name: 'id', value: '7' }],
    });
    expect(state.pathParams.map((p) => [p.key, p.value])).toEqual([
      ['id', '7'],
      ['other', ''],
    ]);
  });
});
```

Append to `src/lib/__tests__/request-save-mapper.test.ts` (it already has the `makeTab` helper):

```ts
describe('buildRequestSavePayload path params', () => {
  it('saves the enabled path params with their values', () => {
    const payload = buildRequestSavePayload(
      makeTab({
        url: 'https://h.test/users/:id/:skip',
        pathParams: [
          { id: 'a', key: 'id', value: '7', enabled: true },
          { id: 'b', key: 'skip', value: 'x', enabled: false },
        ],
      }),
    );
    expect(payload.pathParams).toEqual([{ name: 'id', value: '7' }]);
  });
});
```

- [ ] **Step 11: Run to verify failure**

Run: `yarn test url-params execute-request curl-generator LoadTestDialog pane-utils request-save-mapper`
Expected: the new tests FAIL.

- [ ] **Step 12: Implement the frontend changes**

`src/lib/url-params.ts` append:

```ts
interface PathParamValue {
  name: string;
  value: string;
}

// Display-only mirror of `substitute_path_params` in rocket-http. The backend does the real
// substitution when a request is sent; this only builds the URL shown in the cURL copy and
// the console. Keep the two in step.
export function applyPathParams(url: string, params: PathParamValue[]): string {
  const usable = params.filter((p) => p.name && p.value);
  if (usable.length === 0) return url;
  const schemeAt = url.indexOf('://');
  const hasScheme = schemeAt !== -1 && !/[/?#]/.test(url.slice(0, schemeAt));
  const authorityStart = hasScheme ? schemeAt + 3 : 0;
  const pathOffset = url.slice(authorityStart).search(/[/?#]/);
  if (pathOffset === -1) return url;
  const start = authorityStart + pathOffset;
  const endOffset = url.slice(start).search(/[?#]/);
  const end = endOffset === -1 ? url.length : start + endOffset;
  const path = url
    .slice(start, end)
    .split('/')
    .map((segment) => rewriteSegment(segment, usable))
    .join('/');
  return url.slice(0, start) + path + url.slice(end);
}

function rewriteSegment(segment: string, params: PathParamValue[]): string {
  let out = segment;
  const colon = /^:([A-Za-z0-9_]+)/.exec(segment);
  if (colon) {
    const match = params.find((p) => p.name === colon[1]);
    if (match) out = encodeURIComponent(match.value) + segment.slice(colon[0].length);
  }
  for (const p of params) {
    const escaped = p.name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const pattern = new RegExp(`(?<!\\{)\\{${escaped}\\}(?!\\})`, 'g');
    out = out.replace(pattern, () => encodeURIComponent(p.value));
  }
  return out;
}
```

`src/lib/execute-request.ts`: delete the loop at lines 243-246 (`for (const p of request.pathParams) {...}`), change `let resolvedUrl = resolve(request.url);` to `const resolvedUrl = resolve(request.url);`, add to `ResolvedRequestFields` `pathParams: { name: string; value: string }[];`, compute and return:

```ts
  // The backend substitutes these into the url, after resolving any {{variables}} again.
  const resolvedPathParams = request.pathParams
    .filter((p) => p.enabled && p.key)
    .map((p) => ({ name: p.key, value: resolve(p.value) }));
```

and add `pathParams: resolvedPathParams,` to the returned object. In `sendRequest` replace the `pathParams: effectiveRequest.pathParams.filter(...).map(...)` expression in the `executeRequest` call with `pathParams: resolvedPathParams,` (destructure `pathParams: resolvedPathParams` from `resolveRequestFields`), and in the console entries use `url: applyPathParams(resolvedUrl, resolvedPathParams)` for both `addHttpEntry` calls (import `applyPathParams` from `@/lib/url-params`). In `executeRunnerEntry` (`src/lib/runner-execute.ts`) replace its `pathParams: requestState.pathParams.filter(...).map(...)` with `pathParams: resolved.pathParams,`; update the mock in `src/lib/__tests__/runner-execute.test.ts` to include `pathParams: []` in the `resolveRequestFieldsForPath` mock return.

`src/lib/curl-generator.ts`: in `generateCurlCommand` change `const url = buildUrlWithQuery(resolved.url, ...)` to build from `applyPathParams(resolved.url, resolved.pathParams)` (import it from `@/lib/url-params`).

`src/components/request/LoadTestDialog.tsx`: add `pathParams: resolved.pathParams,` to the object passed to `runLoadTest` (after `queryParams`). `runLoadTest` takes the same `ExecuteRequestInput` shape, which already has `pathParams?`.

`src/lib/tauri-api.ts` `Request` interface: add `pathParams?: PathParam[];` after `headers`.

`src/lib/pane-utils.ts`: replace the `pathParams: extractPathParams(req.url).map(...)` expression with:

```ts
    pathParams: extractPathParams(req.url).map((name) => ({
      id: crypto.randomUUID(),
      key: name,
      value: req.pathParams?.find((p) => p.name === name)?.value ?? '',
      enabled: true,
    })),
```

`src/lib/request-save-mapper.ts`: add to the returned payload, after `headers`:

```ts
    pathParams: tab.request.pathParams
      .filter((p) => p.enabled && p.key)
      .map((p) => ({ name: p.key, value: p.value })),
```

- [ ] **Step 13: Run all the checks**

Run: `yarn test url-params execute-request curl-generator LoadTestDialog pane-utils request-save-mapper runner-execute RequestPanel`
Expected: PASS.

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

Run: `cargo test -j4 -p rocket-http path_params && cargo test -j4 -p rocket-app resolve_request && cargo check -j4 --workspace --tests`
Expected: PASS.

Manual check (needs `yarn tauri dev`): open a request `https://httpbin.org/anything/:id?q={{x}}`, fill the `id` path param with `{{x}}`, send, and confirm the response `url` echoes the resolved value; run the same request from the collection runner and confirm identical output.

- [ ] **Step 14: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, staging by explicit path (every file listed under Files for this task). Suggested subject: `fix(http): resolve path and query parameters in the backend`.

---

## Next Plan

[Plan 02: Cookie jar, binary responses, SigV4 body signing](2026-10-05-protocol-parity-plan-02-rest-cookies-binary-sigv4.md). It does not depend on this plan's code, but it assumes `HttpMethod` is no longer `Copy` (Task 1 here), so run this plan first. Chain to it automatically when this one finishes.
