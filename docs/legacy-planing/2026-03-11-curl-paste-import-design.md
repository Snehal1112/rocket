# cURL Paste Import Design

## Summary

Rocket should support pasting a full cURL command into the request URL
field and converting it into a structured request the same way users
expect from Postman. The feature should work as a best-effort import:
apply all safely mappable fields, populate advanced request parts like
form-data and cookies when possible, and show warnings for unsupported
or lossy pieces instead of rejecting the paste.

## Current Problem

The request URL field in Rocket currently behaves as a plain URL editor.
Users who paste a cURL command do not get automatic request
reconstruction, which makes migration from terminal snippets or Postman
less efficient.

## Goals

- Detect pasted cURL commands in the URL field.
- Parse cURL into Rocket request state on the frontend.
- Populate method, URL, headers, query params, auth, body, multipart
  form-data, cookies, and file parts when representable.
- Show non-blocking warnings for unsupported or ambiguous flags.
- Preserve normal URL paste behavior when the text is not cURL.

## Non-Goals

- Full shell execution or interpolation support.
- Backend parsing service for this interaction.
- Perfect parity with every cURL flag on first ship.

## Recommended Approach

### 1. Frontend-owned cURL paste detection

Handle cURL import directly in the request-builder URL input flow.

When paste content starts with `curl` or otherwise matches a cURL command
shape, intercept the paste and send the full text to a cURL parser
instead of treating it as a normal URL string.

### 2. Structured parser and normalization layer

Add a frontend parser/normalizer that converts cURL text into a Rocket
request model.

The normalized output should include:

- method
- url
- headers
- query params
- auth
- body type and content
- form-data fields and file parts
- cookie import result
- warnings

This should be an explicit intermediate model rather than mutating UI
state during parse.

### 3. One-pass request state application

Once parsed, apply the imported request fields to the active request in
one request-builder update flow. Defined cURL fields should replace the
existing request fields they correspond to rather than partially merging
with stale values.

### 4. Best-effort warnings

Unsupported or ambiguous flags should not block import. Instead, Rocket
should show a concise warning summary near the request builder or in a
toast/dialog surface.

Examples:

- ignored `--compressed`
- unsupported transport flag
- imported cookie as header only
- unresolved shell expression kept as literal text
- file path could not be mapped cleanly

## Supported Import Scope

Initial supported cURL mapping should include:

- `-X`, `--request`
- URL and query string
- `-H`, `--header`
- `-d`, `--data`, `--data-raw`, `--data-binary`, `--data-urlencode`
- `-F`, `--form`
- `-u`, `--user`
- `-b`, `--cookie`
- common JSON and form body inference

Advanced behavior:

- multipart file parts become Rocket form-data file fields when path
  extraction is possible
- cookies are mapped into a `Cookie` header or other closest current
  Rocket representation
- unknown flags are retained only as warnings, not silently dropped

## Expected User Experience

When the user pastes a plain URL:

- Rocket keeps the current URL-editing behavior

When the user pastes a cURL command:

- Rocket imports the request immediately
- the request builder updates method, URL, headers, body, and related
  fields
- Rocket surfaces any warnings without blocking the import

## Error Handling

If the pasted command cannot be parsed at all:

- Rocket should leave the current request untouched
- show a clear parse error

If only part of the command is unsupported:

- import supported fields
- show warnings for the rest

## Testing

Add targeted frontend tests for:

- non-cURL URL paste
- JSON cURL import
- multipart cURL import with files
- auth and cookie inference
- warning generation for unsupported flags
- replacement of existing request state on import

Manual verification should include pasting real cURL commands copied
from browser devtools and APIs with multipart payloads.
