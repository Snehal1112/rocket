# Export name-based scrub: design

Closes the gap where the collection export (Bruno ZIP and Postman v2.1) still writes credential-like values as written. Origin: the plan-1 review (Important #1, Minor 8) and the investigation of 2026-10-23 (four read-only checks of the Bruno scrub, the Postman scrub, the data model and false-positive risk). Background: `.claude/collection-export.md`, `crates/rocket-export/CLAUDE.md`.

## Decisions (agreed with the user)

1. One shared name rule decides whether a key/value row holds a secret. It matches the **last word** of a name, never a substring.
2. The rule applies to key/value rows and to the values in them, in both formats.
3. Non-secret variables whose name matches the rule are blanked too. The note says "by name".
4. A secret variable whose value is a lone `{{variable}}` reference keeps the reference in Bruno files and in Postman collection, folder and request variables. Postman environments are the exception: secret values come from the keychain and the service clears them right after the read, so a secret environment value is always exported empty. The keep-a-ref rule in `postman_env.rs` only matters when the function is called with a ref directly.
5. URL userinfo without a `:` (`https://TOKEN@host`) is not blanked. It is documented only, because blanking it would also blank plain usernames.
6. Request bodies, scripts, descriptions, `.js` and `.proto` files stay as written and are not scanned.
7. Identifiers stay: basic/digest/NTLM usernames, awsv4 `accessKeyId`, OAuth2 `clientId`.
8. `crates/rocket-app/src/redaction.rs` and the two TypeScript copies keep the five-header list for Console masking. They are not changed here, because the TypeScript copies must stay in step with them.

## The name rule

`is_sensitive_name(name: &str) -> bool`, in `crates/rocket-export/src/sensitive.rs`.

- Split the name into lowercase words at non-alphanumeric characters, at a lower-to-upper boundary (`accessToken` gives `access`, `token`) and at an acronym boundary (`APIKey` gives `api`, `key`).
- Allow-list first: if the words joined with `_` equal `token_type`, `page_token`, `next_token`, `prev_token`, `previous_token` or `max_tokens`, the name is not sensitive.
- Sensitive when the last word is one of `token`, `secret`, `password`, `passwd`, `credential`, `credentials`, `authorization`, `cookie`, `signature`, `apikey`.
- Sensitive when the name is exactly one word `pass` or `pwd`.
- Sensitive when the last two words are `api key`, `private key`, `secret key` or `client assertion`.
- Everything else is not sensitive (`author`, `keyword`, `sort_key`, `Cache-Key`, `session_id`, `auth_type`, `grant_type`, `bypass`, `compass`, `X-Request-Id`).
- The five current headers all still match (`authorization`, `proxy-authorization`, `cookie`, `set-cookie`, `x-api-key`).

A value is blanked only when it is a non-empty literal and not `is_variable_reference`. The report note is `Blanked`, with the feature name of the location and the detail `<name> (by name)`.

## Where the rule applies

| Location | Bruno ZIP | Postman |
|---|---|---|
| Header rows, gRPC and WebSocket metadata | yes | headers |
| `params` rows and query pairs in the URL string | yes | `url.raw`, `url.query` |
| Form-urlencoded and multipart text rows | yes | `urlencoded`, `formdata` text rows |
| OAuth2 `additionalParameters` rows | yes | not exported |
| OAuth2 and OAuth1 URL fields (`accessTokenUrl`, `refreshTokenUrl`, `authorizationUrl`, `callbackUrl`) | userinfo password and query pairs | token, authorize, refresh URLs |
| Variables (non-secret by name, secret always) | yes, including environments | collection, folder, request, environments |

## Also fixed

- Postman typed scrub gains the OAuth1 `accessToken` slot (Bruno already blanks it).
- Postman writes the `Blanked` note for an auth only when that auth is exported (not for OAuth1 and WSSE, which are skipped).
- Dialog text and docs state the new coverage.
