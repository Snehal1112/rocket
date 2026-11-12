# rok.grpc: gRPC calls from scripts (R-05, Part E)

Date: 2026-11-12. Status: design approved in chat, spec awaiting review.

## Goal

A script can make gRPC calls with `rok.grpc.run` (a saved gRPC request) and `rok.grpc.call` (an ad hoc call), for all four method types. The result is one object with the status, the messages, the headers and the trailers.

Bruno's JavaScript API reference could not be reached while writing this spec (DNS failure), so this API is Rocket-native and not checked against Bruno. `docs/features/rok-api-parity-notes.md` (Part E) records that. If Bruno has a gRPC scripting API, a later item aligns names.

Read `docs/superpowers/specs/opencollection-spec-reference.md` (section 2.5, GrpcRequest) before any task that touches gRPC request files or the collection model.

## Decisions (from the brainstorm)

| Topic | Decision |
|---|---|
| Call forms | Saved request path and ad hoc call. Both ship. |
| Method types | Unary, server streaming, client streaming, bidirectional. |
| Streaming model | Non-interactive exchange: send all input messages, half-close, collect replies. No send-then-read loops from a script. |
| Trust | Both forms need Developer mode. Safe mode refuses them. |
| Request guard | The same host check `rok.sendRequest` uses runs before connecting. |
| Bruno parity | Unverified. Rocket-native names. |

## Script API

Both functions return a Promise and also take an optional Node-style callback (`(err, result)`), like `rok.sendRequest`.

```js
// Saved request: proto, server, auth and variables come from the collection.
const r = await rok.grpc.run('orders/get-order.yml', {
  messages: [{ id: 7 }],        // optional, replaces the saved message
  metadata: { 'x-trace': 'a' }, // optional, added to the saved metadata
  timeoutMs: 5000,              // optional
});

// Ad hoc.
const r = await rok.grpc.call({
  url: 'grpc://localhost:50051',
  service: 'orders.Orders',
  method: 'GetOrder',
  protoFile: 'protos/orders.proto', // relative path inside the collection
  messages: [{ id: 7 }],
  metadata: {},
  tls: false,                       // true for grpcs
  timeoutMs: 5000,
});
```

Result, the same for both forms and all four method types:

| Member | Meaning |
|---|---|
| `status` | `{ code, name, message }`. `code` 0 and `name` `OK` on success. |
| `message` | The first response message, or `undefined` when none arrived. |
| `messages` | Every response message, in arrival order. |
| `headers`, `trailers` | Plain objects, lowercase keys, string values. |
| `truncated` | `true` when a cap ended the call early. |

Input rules:
- `messages` is an array of JSON objects. `message` (one object) is a shortcut for a one-item array. Giving both is an error.
- Unary and server streaming take exactly one input message. Client streaming and bidirectional take one or more. Zero messages on a saved request means the request's selected saved message.
- A message that does not fit the proto is a setup error that names the field path, never the value.

## Limits

All in `rocket-grpc` as named constants.

| Limit | Value | When hit |
|---|---|---|
| Response messages | 1000 | The call ends, the messages so far are kept, status `RESOURCE_EXHAUSTED`, `truncated` true. |
| Total response bytes | 10 MiB | Same. |
| Input messages | 1000 | Setup error. |
| Time | `timeoutMs`, default 30 s, ceiling 5 min | Status `DEADLINE_EXCEEDED`, messages so far kept. |

A nested call counts against the same depth and time budget as `rok.runRequest` (Part B rulings).

## Errors

- The Promise rejects only for setup problems: bad or missing path, an item that is not a gRPC request, unknown service or method, a message that does not fit the proto, a proto path outside the collection, Safe mode, a blocked host, a wrong argument type. Messages never contain variable values.
- Anything the server or transport answers resolves with `status`. A failed connect or TLS error resolves as `UNAVAILABLE` with a short message, like the gRPC pane.
- Extra input messages for a unary or server-streaming method are a setup error. One message to a client-streaming method is fine.

## Trust and safety

- Developer mode only. The refusal names the mode.
- The request guard checks the target host. gRPC has no redirects, so the R-68 redirect limit does not apply. The DNS-rebinding limit (R-68) does.
- Ad hoc `protoFile` must be relative, with no `..`, empty or `.` segments, no backslash, colon or NUL, and must stay inside the collection after symlinks resolve. This is the rule of `validate_data_file_path` in `rocket-collection`; the tasks reuse or generalise it, not copy it.
- Variables and auth resolve as in the gRPC pane. Secret values are not echoed in errors or logs.

## Architecture (DDD layers)

| Layer | Contents |
|---|---|
| `rocket-scripting` (contract) | `ScriptHost::grpc(HostGrpcRequest) -> Result<HostGrpcResponse, HostError>`, default "unavailable". `HostGrpcRequest` and `HostGrpcResponse` are plain data. |
| `rocket-grpc` (domain) | The limit constants. No I/O. |
| `rocket-app` | `GrpcService::exchange(...)`: reuses variable and auth resolution and the proto cache, sends all input messages, half-closes, collects replies up to the limits. `ExecutionScriptHost` implements `grpc`: resolves the saved path or the ad hoc fields, checks Developer mode and the guard, calls `exchange`. A helper finds a gRPC item by path (the HTTP-only `find_run_target` stays as it is). |
| `rocket-infra` | Op `op_rok_grpc` and `rok.grpc.run` / `rok.grpc.call` in `bootstrap.js` with argument checks. `TonicGrpcExecutor` and `FsProtoLoader` are unchanged. |
| `src-tauri` | Wiring only: give the script host the `GrpcService`. No new command, so `app-commands.txt` is unchanged. |
| Frontend | Typings in `src/components/editor/rok-types.ts`. |

`rocket-infra` defines no new traits. `rocket-app` reads no files. The script host reaches protos through `ProtoLoader`.

## Testing

- `rocket-grpc`: limit constants.
- `rocket-app`: `exchange` with the in-memory executor for all four types, the caps, half-close, ordering, the time limit.
- `ExecutionScriptHost`: Safe mode refusal, the guard, path containment, variable and auth resolution, saved-path lookup, a non-gRPC item.
- Real script engine end to end: real `rok.grpc.run` and `rok.grpc.call` scripts against the fake executor (not `ProgrammableEngine`, which cannot run JS).
- `rocket-infra`: one script-level call over the real transport with the greeter fixture in `crates/rocket-infra/test-fixtures/grpc`.
- Frontend: a typings test that the members exist.
- Manual test file against a local greeter server.

## Plans (3 plans, 3 tasks each)

1. **Core exchange.** Host types, limit constants, `GrpcService::exchange` for all four types.
2. **Script surface.** The op, `bootstrap.js` functions, `ExecutionScriptHost::grpc` for both forms, trust and guard checks, end-to-end tests.
3. **Typings, docs, manual test, roadmap.** `rok-types.ts`, feature doc, manual test, crate guides, parity notes (Part E, Bruno unverified), roadmap R-05 done and follow-ups.

## Out of scope (follow-up items)

- Interactive streaming from scripts (send, read, send).
- gRPC support in `rok.runRequest`, which still skips gRPC items.
- Server reflection from scripts.
- gRPC calls in Flow nodes.
- Aligning names with Bruno once its gRPC scripting API is known.
