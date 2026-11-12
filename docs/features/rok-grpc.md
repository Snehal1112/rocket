# rok.grpc: gRPC calls from scripts

A script can call a gRPC method with `rok.grpc.run` (a saved gRPC request) or `rok.grpc.call` (an ad hoc call). All four method types work. Roadmap item R-05 (Part E of `docs/features/rok-api-parity-notes.md`). Spec: `docs/superpowers/specs/2026-11-12-rok-grpc-design.md`, plans: `docs/superpowers/plans/rok-grpc/`. Manual checks: `docs/manual-tests/rok-grpc.md` (open, nobody has run the app yet).

Bruno's gRPC scripting API could not be checked while this was written (its JavaScript API reference was not reachable). The names are Rocket-native and not verified against Bruno. If Bruno has such an API, a later item aligns the names (R-92).

Typings are in `src/components/editor/rok-types.ts`. The runtime is in `crates/rocket-infra/src/scripting/bootstrap.js` (`rok.grpc`) and `ops/host.rs` (`op_rok_grpc`).

## API

Both functions return a Promise and also take an optional Node-style callback `(err, result)`, like `rok.sendRequest`. With a callback, the Promise resolves to what the callback returns.

```js
// Saved request: proto, server, auth and variables come from the collection.
const r = await rok.grpc.run('orders/get-order.yml', {
  messages: [{ id: 7 }],        // optional, replaces the saved message
  metadata: { 'x-trace': 'a' }, // optional, added to the saved metadata
  timeoutMs: 5000,              // optional
});

// Ad hoc.
const r2 = await rok.grpc.call({
  url: 'grpc://localhost:50051',
  service: 'orders.Orders',
  method: 'GetOrder',
  protoFile: 'protos/orders.proto', // relative path inside the collection
  message: { id: 7 },
  metadata: {},
  tls: false,                       // true for grpcs
  timeoutMs: 5000,
});
```

`run` takes the path of the request file relative to the collection root. The extension (`.yml`, `.yaml`, `.json`, `.bru`) is optional, `\` is read as `/`, and outer slashes are dropped. `run(path, callback)` is allowed too.

`call` needs `url`, `service`, `method` and `protoFile` as non-empty strings. The method is sent as `service/method`. A `url` without `://` gets `grpc://`, or `grpcs://` when `tls` is true. `tls` must be a boolean when given.

### Input

- `messages` is an array of JSON objects. `message` (one object) is a shortcut for a one-item array. Giving both rejects with a TypeError.
- Unary and server streaming take exactly one input message. More than one is a setup error. Client streaming and bidirectional take one or more, at most 1000.
- With no message at all, the call sends the saved request's selected message once.
- `metadata` is an object with string values. Keys are lowercased. A script entry replaces a saved entry of the same name (compared without case).
- `timeoutMs` is a finite number above 0. See the limits table.
- A message that does not fit the proto is a setup error naming `message N`, never the value.

### Result

The same shape for both forms and all four method types.

| Member | Meaning |
|---|---|
| `status` | `{ code, name, message }`. Code 0 and name `OK` on success. |
| `message` | The first response message, or `undefined` when none arrived. |
| `messages` | Every response message (parsed JSON), in arrival order. |
| `headers`, `trailers` | Plain objects, lowercase keys, string values. |
| `truncated` | `true` when a size cap ended the call early. |

A response message that is not valid JSON rejects with `rok.grpc.run: response message N is not valid JSON` (or `rok.grpc.call: ...`). Only the index is reported.

## Streaming model

The exchange is non-interactive. The call sends all input messages, half-closes the request side, then collects every reply until the server finishes (or a cap or the time limit stops it). Sending and reading run side by side, so a server that answers while input is still going out does not block. A script cannot send, read, then send again. That is a follow-up (R-88).

Examples, one per method type (the greeter service from `crates/rocket-infra/test-fixtures/grpc/greeter.proto`):

```js
const unary = await rok.grpc.call({ url: 'grpc://localhost:50051', service: 'demo.greeter.v1.Greeter',
  method: 'SayHello', protoFile: 'greeter.proto', message: { name: 'ann' } });

const server = await rok.grpc.run('greeter/list-greetings', { message: { name: 'ann' } });
// server.messages has every reply.

const client = await rok.grpc.run('greeter/collect-names', { messages: [{ name: 'a' }, { name: 'b' }] });
// client.message is the single reply.

const bidi = await rok.grpc.run('greeter/chat', { messages: [{ name: 'a' }, { name: 'b' }] });
// bidi.messages has one reply per input in this server.
```

## Limits

Constants live in `crates/rocket-grpc/src/limits.rs`. The exchange loop is in `crates/rocket-app/src/grpc_service/exchange.rs`.

| Limit | Value | When hit |
|---|---|---|
| Response messages | 1000 | The call is aborted, the messages so far are kept, status `RESOURCE_EXHAUSTED` (8) with the text "the response was cut at the size limit", `truncated` true. |
| Total response bytes | 10 MiB (of message JSON text) | Same. |
| Input messages | 1000 | Setup error (rejects). |
| Time | `timeoutMs`, default 30 s, ceiling 5 min (a value above the ceiling is lowered, 0 or missing means the default) | Status `DEADLINE_EXCEEDED` (4), text "the call did not finish within N ms". Messages so far are kept. The stream is aborted. |

A gRPC call takes one nested-run slot while it runs. The slot count is shared with `rok.runRequest`: at most 16 nested runs are active per send (`MAX_ACTIVE_NESTED_RUNS`). Past it, the call rejects with `too many nested requests running (limit 16)`.

## Errors

The Promise rejects only for setup problems. The message starts with `rok.grpc.run:` or `rok.grpc.call:` and never contains variable values.

- A wrong argument type or shape (TypeError, nothing reaches the host).
- Safe mode: `gRPC calls need Developer mode`.
- A bad or missing path (`invalid request path - <path>`), or an item that is not a gRPC request (`<path> is not a gRPC request`).
- An unknown service or method, a proto that does not compile, a message that does not fit the proto, more input messages than the method takes.
- A proto path outside the collection, or a blocked host.
- `gRPC is not available here`: see "Where it is unavailable".
- `too many nested requests running`.

Everything else resolves with `status`:

- A status from the server, such as `NOT_FOUND`, resolves normally.
- A connect failure, a TLS failure and a server reflection failure resolve with status `UNAVAILABLE` (14) and the fixed text `could not connect to the server`, with no messages. The text is fixed on purpose: a transport error can quote the URL, which may hold credentials.
- The time limit and the response caps resolve (see Limits).

## Trust and safety

- Developer mode only, for both forms. In Safe mode the call rejects before anything is resolved.
- Request guard. When the workspace policy "Block script redirects to internal hosts" is on, the guard runs on the exact URL the transport dials, once, before server reflection and before any connect. The URL is resolved only once, so a dynamic variable cannot give the guard one host and the transport another. `grpc://` and `grpcs://` are read like `http(s)://` by the same parser (`rocket_grpc::parse_target`), so shorthand IPv4 hosts such as `127.1`, `2130706433` and `0x7f.1` are blocked like any other loopback form. The guard checks host names, not resolved IPs (the same limit as `rok.sendRequest`, see R-68). The guard is skipped when the target host equals the host of the calling request. gRPC has no redirects, so there are no redirect hops to check.
- Proto confinement. A script never names a proto file outside the collection. The path must pass the lexical rule `rocket_collection::validate_relative_path` (not empty, relative, `/` separators, no `\`, `:` or NUL, no empty, `.` or `..` part) and must not start with `~`. Then `ProtoLoader::resolve_within` canonicalises it (symlinks followed) and refuses a result outside the collection folder, a directory or a missing file. This applies to the saved request's `protoFilePath` as well as to `protoFile`, for every `rok.grpc` call. The gRPC pane is not confined (it still accepts an absolute path).
- Known limit: `ProtoLoader::load` does not re-check confinement for the imports of an allowed proto. Exploiting it needs write access to the collection folder. A follow-up re-checks it (R-95).
- Secrets. Variables resolve as in the gRPC pane, and secret values are not echoed in errors. The Console line holds the target (path, or URL and `service/method`) and the status name, never message content.

### Auth and headers

- `rok.grpc.run` keeps the saved request's own auth and inheritance exactly as the gRPC pane does.
- `rok.grpc.call` carries no inherited collection auth and no collection default headers. Only the script's `metadata` is sent. A script that picks the host never gets the collection token or headers. Put what the server needs in `metadata`.

### Variables

`{{var}}` fills from the usual scopes plus the script's runtime variables. The request's own variables win over runtime variables. Unlike `rok.runRequest`, `rok.grpc` does not see environment writes made earlier in the same script (they are not carried to the call). A later item can change that (R-93).

## Where it is unavailable

The call needs the `GrpcService` on the execution service. The main execution service has it (`with_grpc_service` in `src-tauri/src/lib.rs`). The service built for the agent tool-call path (`mcp_exec_svc`) does not, so a script run through it rejects with `rok.grpc.run: gRPC is not available here` (R-94). Flow transform and condition nodes run without a script host, so `rok.grpc` is unavailable there too (R-91).

`rok.runRequest` still skips gRPC items. Use `rok.grpc.run` for them (R-89).

## Layers

| Layer | Contents |
|---|---|
| `rocket-scripting` | `ScriptHost::grpc(HostGrpcRequest) -> Result<HostGrpcResponse, HostError>`, default "unavailable". Plain data types `HostGrpcRequest`, `HostGrpcTarget` (saved or ad hoc), `HostGrpcResponse`, `HostGrpcStatus`. |
| `rocket-grpc` | `limits.rs` (constants, `clamp_call_timeout`) and `target.rs` (`parse_target`). No I/O. |
| `rocket-app` | `GrpcService::exchange` and `exchange_checked` (`grpc_service/exchange.rs`). `ExecutionScriptHost::grpc` (`execution_service/script_host.rs`) resolves the saved path or the ad hoc fields and runs the guard. `find_grpc_target` (`run_request.rs`) finds a gRPC item. |
| `rocket-infra` | `op_rok_grpc` and `rok.grpc` in `bootstrap.js`. `TonicGrpcExecutor` and `FsProtoLoader` (`resolve_within`) are the transport and the proto reader. |
| `src-tauri` | Wiring only (`with_grpc_service`). No new command, so `app-commands.txt` is unchanged. |

## Follow-ups

Roadmap rows R-88 to R-95 in `docs/roadmaps/roadmap.md`: interactive streaming, gRPC items in `rok.runRequest`, reflection control, Flow nodes, Bruno names, pending env writes, the agent tool-call service, and the confinement re-check inside `ProtoLoader::load`.
