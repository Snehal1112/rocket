# Glossary

Which name is the right one when a concept has several. Written for D-87 in [../roadmaps/ddd-roadmap.md](../roadmaps/ddd-roadmap.md) on 2026-11-09. Rulings from [ddd-decisions.md](ddd-decisions.md) (D-05) apply.

This file names a preferred term. It renames nothing. A rename touches wire formats and many files, so it needs its own decision. Every row below says "rename: no" until such a decision exists.

How to read a row: the preferred term, the other names in use with their crate and file, a one-line reason, and the rename status. Paths are under `crates/`.

## Run summaries

Rule: a summary type is prefixed with its context, unless it is that context's only one.

| Term | Other names | Reason | Rename |
|---|---|---|---|
| `RunSummary` | `FlowRunSummary` (`rocket-app/src/flow_execution_service.rs`) | `RunSummary` is the result of one collection runner run (`rocket-app/src/collection_runner_service.rs`): run id, collection, folder path, steps, stop reason. `FlowRunSummary` is the result of one Flow run: run id, steps, stop reason, partial-run info. They share a shape but not a meaning. Keep both names. | no |

## Variables

Rule: "variable" means a named value in a scope. The type name carries the scope.

| Term | Other names | Reason | Rename |
|---|---|---|---|
| `Variable` | | A variable of an environment: key, value, enabled, secret (`rocket-environment/src/variable.rs`). | no |
| `CollectionVariable` | | A variable in collection settings (requests also carry a list of them): key, value, initial value, enabled, secret (`rocket-collection/src/settings.rs`). | no |
| `VariableValue` | | The shared value enum, a plain string or a typed `{data, value_type}` (`rocket-shared/src/variable_value.rs`). It is a value, not a variable. | no |
| `EnvVarWrite` | | A script's request to write one environment variable (`rocket-scripting/src/result.rs`). A `null` value means delete. It is a request to write, not a variable. | no |

## Key-value rows

Rule: use `Header` for HTTP headers. Do not add another pair type; reuse an existing one. The three existing types stay.

| Term | Other names | Reason | Rename |
|---|---|---|---|
| `Header` | `KeyValueEntry` (`rocket-collection/src/contract/snapshot.rs`), `GrpcMetadataPair` (`rocket-grpc/src/events.rs`) | `Header` (`rocket-shared/src/types.rs`) is an HTTP header row with an enabled flag and a description. `KeyValueEntry` is a bare key and value inside a contract request snapshot (headers, query params, form fields). `GrpcMetadataPair` is a name and value line of a gRPC call as shown to the user. | no |

## acp and assistant

| Term | Other names | Reason | Rename |
|---|---|---|---|
| acp | assistant | acp is the protocol and its crate (`rocket-acp`). Assistant is the user-facing feature. This is the D-05 ruling. | no |

## Assistant mode

| Term | Other names | Reason | Rename |
|---|---|---|---|
| `AssistantMode` | none | `AssistantMode` (`rocket-acp/src/mode.rs`) is the session mode: Ask, Edit or Agent. It is also the mode saved in a conversation file (`ConversationRecord.mode`). The old `ConversationMode` copy was merged into it (D-82, 2026-11-09). | no |

## Saved request blueprint

| Term | Other names | Reason | Rename |
|---|---|---|---|
| template (`Template`) | request (`Request` in `rocket-collection/src/request.rs`) | A template is a saved request blueprint: name, method, URL, headers, optional body (`rocket-collection/src/template.rs`). It is keyed by name and is not part of any collection. A request lives in a collection and has a uid. | no |
