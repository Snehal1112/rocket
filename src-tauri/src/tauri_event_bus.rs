use rocket_infra::RecentWrites;
use rocket_shared::events::{EventPublisher, PublishedEvent};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

/// Process-wide event counter, shared by every `TauriEventBus` instance.
static NEXT_SEQ: AtomicU64 = AtomicU64::new(1);

/// Failed emits per channel, used to rate-limit the warning.
static EMIT_FAILURES: Mutex<BTreeMap<&'static str, u64>> = Mutex::new(BTreeMap::new());

pub struct TauriEventBus {
    app: AppHandle,
}

impl TauriEventBus {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn emit_value(&self, channel: &'static str, payload: &Value) {
        if let Err(err) = self.app.emit(channel, payload) {
            // The payload is left out on purpose, it may hold secrets. The
            // `backend-log` emitter does not use this bus, so this cannot loop.
            if let Some(count) = failure_to_log(&EMIT_FAILURES, channel) {
                tracing::warn!(channel, failures = count, error = %err, "event emit failed");
            }
        }
    }
}

impl EventPublisher for TauriEventBus {
    fn publish(&self, event: &dyn PublishedEvent) {
        // Record in-app collection writes first, so the file watcher can
        // drop its own report of the same write.
        if let Some(writes) = self.app.try_state::<Arc<RecentWrites>>() {
            writes.record(event);
        }
        let channel = event.channel();
        let seq = next_seq();
        let payload = match envelope(event, seq, now_ms()) {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(channel, error = %err, "event serialize failed");
                return;
            }
        };
        // A branch switch also fires "collection-changed", so an expanded
        // CollectionNode re-fetches its request tree.
        for name in emit_channels(event) {
            self.emit_value(name, &payload);
        }
    }
}

/// The next process-wide sequence number. It starts at 1 and only grows.
fn next_seq() -> u64 {
    NEXT_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Unix time in milliseconds. A clock before the epoch reads as 0.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The event as a JSON object plus the `seq` and `tsMs` envelope keys.
/// A key the event already has is never overwritten.
fn envelope(event: &dyn PublishedEvent, seq: u64, ts_ms: u64) -> Result<Value, serde_json::Error> {
    Ok(add_envelope(event.to_json()?, seq, ts_ms))
}

/// Adds `seq` and `tsMs` to a JSON object unless it has those keys already.
fn add_envelope(mut value: Value, seq: u64, ts_ms: u64) -> Value {
    if let Value::Object(map) = &mut value {
        map.entry("seq").or_insert_with(|| Value::from(seq));
        map.entry("tsMs").or_insert_with(|| Value::from(ts_ms));
    }
    value
}

/// Counts one failure on `channel`. Returns the count when it should be
/// logged: the first failure, then every 100th.
fn failure_to_log(
    failures: &Mutex<BTreeMap<&'static str, u64>>,
    channel: &'static str,
) -> Option<u64> {
    let mut map = failures.lock().unwrap_or_else(|e| e.into_inner());
    let count = map.entry(channel).or_insert(0);
    *count += 1;
    (*count == 1 || count.is_multiple_of(100)).then_some(*count)
}

/// The channels an event is emitted on: its own first, then the extras.
fn emit_channels(event: &dyn PublishedEvent) -> Vec<&'static str> {
    let mut channels = vec![event.channel()];
    channels.extend_from_slice(event.also_emit_on());
    channels
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::AcpEvent;
    use rocket_app::execution_events::ExecutionEvent;
    use rocket_app::openapi_sync_events::OpenApiSyncEvent;
    use rocket_collection::CollectionEvent;
    use rocket_flow::FlowEvent;
    use rocket_git::GitEvent;
    use rocket_network::{
        ConflictKind, ConflictView, Direction, Outcome, SyncEvent, SyncFinish, SyncRole,
        SyncSummary, TransferEvent,
    };

    #[test]
    fn emit_channels_adds_collection_changed_for_branch_switch_and_merge() {
        let switched = GitEvent::BranchSwitched {
            collection: "api".into(),
            branch: "dev".into(),
        };
        assert_eq!(
            emit_channels(&switched),
            vec!["git-changed", "collection-changed"]
        );
        let merged = GitEvent::BranchMerged {
            collection: "api".into(),
            branch: "dev".into(),
        };
        assert_eq!(
            emit_channels(&merged),
            vec!["git-changed", "collection-changed"]
        );
    }

    #[test]
    fn emit_channels_is_one_channel_for_other_events() {
        let saved = CollectionEvent::RequestSaved {
            collection: "api".into(),
            path: "r.yml".into(),
        };
        assert_eq!(emit_channels(&saved), vec![saved.channel()]);
        let committed = GitEvent::GitCommit {
            collection: "api".into(),
            message: "m".into(),
            sha: "s".into(),
        };
        assert_eq!(emit_channels(&committed), vec!["git-changed"]);
        assert_eq!(
            emit_channels(&rocket_history::HistoryEvent::HistoryCleared),
            vec!["history-changed"]
        );
    }

    #[test]
    fn emit_channels_is_one_channel_for_environment_events() {
        use rocket_environment::EnvironmentEvent;
        assert_eq!(
            emit_channels(&EnvironmentEvent::EnvironmentSaved { name: "dev".into() }),
            vec!["environment-changed"]
        );
        assert_eq!(
            emit_channels(&EnvironmentEvent::EnvironmentDeleted { name: "dev".into() }),
            vec!["environment-changed"]
        );
    }

    #[test]
    fn emit_channels_is_one_channel_for_workspace_events() {
        use rocket_workspace::WorkspaceEvent;
        assert_eq!(
            emit_channels(&WorkspaceEvent::WorkspaceCreated {
                id: "w1".into(),
                name: "n".into(),
                path: "/p".into(),
            }),
            vec!["workspace-created"]
        );
        assert_eq!(
            emit_channels(&WorkspaceEvent::WorkspaceDescriptionUpdated {
                id: "w1".into(),
                description: None,
            }),
            vec!["workspace-description-updated"]
        );
    }

    #[test]
    fn acp_events_emit_on_their_own_single_channel() {
        let started = AcpEvent::AcpSessionStarted {
            session_id: "s1".into(),
        };
        assert_eq!(emit_channels(&started), vec!["agent-session-started"]);
        assert_eq!(
            emit_channels(&AcpEvent::AssistantConversationsCleared),
            vec!["assistant-conversations-cleared"]
        );
    }

    #[test]
    fn openapi_sync_events_emit_on_their_own_single_channel() {
        let checked = OpenApiSyncEvent::OpenApiSyncChecked {
            collection: "api".into(),
            status: "changed".into(),
        };
        assert_eq!(emit_channels(&checked), vec!["openapi-sync-checked"]);
        let applied = OpenApiSyncEvent::OpenApiSyncApplied {
            collection: "api".into(),
            applied: 3,
            stale: 1,
        };
        assert_eq!(emit_channels(&applied), vec!["openapi-sync-applied"]);
    }

    #[test]
    fn execution_events_emit_on_their_own_single_channel() {
        let runner = ExecutionEvent::RunnerFinished {
            run_id: "r".into(),
            stopped_reason: "completed".into(),
            step_count: 1,
            failed_count: 0,
        };
        assert_eq!(emit_channels(&runner), vec!["runner-finished"]);
        let websocket = ExecutionEvent::WebSocketStatus {
            session_id: "s".into(),
            state: rocket_app::execution_events::WebSocketSessionState::Open,
            subprotocol: None,
            code: None,
            reason: None,
        };
        assert_eq!(emit_channels(&websocket), vec!["ws:status"]);
        let script = ExecutionEvent::ScriptVariableWritten {
            scope: "collection".into(),
            environment: None,
            collection: Some("c".into()),
            key: "K".into(),
        };
        assert_eq!(emit_channels(&script), vec!["script-variable-written"]);
    }

    #[test]
    fn assistant_conversation_events_have_their_own_channels() {
        let id = || "c1".to_string();
        assert_eq!(
            PublishedEvent::channel(&AcpEvent::AssistantConversationSaved {
                conversation_id: id()
            }),
            "assistant-conversation-saved"
        );
        assert_eq!(
            PublishedEvent::channel(&AcpEvent::AssistantConversationDeleted {
                conversation_id: id()
            }),
            "assistant-conversation-deleted"
        );
        assert_eq!(
            AcpEvent::AssistantConversationsCleared.channel(),
            "assistant-conversations-cleared"
        );
    }

    #[test]
    fn file_watcher_failed_has_its_own_channel() {
        let event = CollectionEvent::FileWatcherFailed {
            message: "m".into(),
        };
        assert_eq!(event.channel(), "file-watcher-failed");
    }

    #[test]
    fn flow_request_paths_rewritten_has_its_own_channel() {
        let event = FlowEvent::FlowRequestPathsRewritten {
            collection: "c".into(),
            from: "a".into(),
            to: "b".into(),
            flows: Vec::new(),
            failed: Vec::new(),
        };
        assert_eq!(event.channel(), "flow-request-paths-rewritten");
        assert_eq!(emit_channels(&event), vec!["flow-request-paths-rewritten"]);
    }

    #[test]
    fn permission_events_have_their_own_channels() {
        assert_eq!(
            PublishedEvent::channel(&AcpEvent::AcpPermissionRequested {
                session_id: "s".into(),
                request_id: "perm-1".into(),
                call_id: "c".into(),
                title: "t".into(),
                kind: "other".into(),
                summary: String::new(),
                options: Vec::new(),
            }),
            "agent-permission-requested"
        );
        assert_eq!(
            PublishedEvent::channel(&AcpEvent::AcpPermissionResolved {
                session_id: "s".into(),
                request_id: "perm-1".into(),
                option_id: None,
            }),
            "agent-permission-resolved"
        );
    }

    #[test]
    fn load_test_events_keep_their_channel_names() {
        use rocket_http::{LoadTestEvent, LoadTestProgressEvent, LoadTestResult};
        let progress = LoadTestProgressEvent {
            elapsed_ms: 0,
            completed: 0,
            active_concurrent: 0,
            succeeded: 0,
            failed_status: 0,
            failed_transport: 0,
            requests_per_second: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            current_phase_index: 0,
            recent_log: vec![],
        };
        assert_eq!(
            emit_channels(&LoadTestEvent::LoadTestProgress(progress)),
            vec!["load_test_progress"]
        );
        let result = LoadTestResult {
            total_requests: 0,
            succeeded: 0,
            failed: 0,
            failed_transport: 0,
            failed_status: 0,
            min_latency_ms: 0.0,
            avg_latency_ms: 0.0,
            p50_latency_ms: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            max_latency_ms: 0.0,
            requests_per_second: 0.0,
            total_duration_ms: 0.0,
            phase_timeline: vec![],
            request_log: vec![],
            time_series: vec![],
        };
        assert_eq!(
            emit_channels(&LoadTestEvent::LoadTestComplete(result)),
            vec!["load_test_complete"]
        );
    }

    #[test]
    fn grpc_events_keep_their_channel_names() {
        use rocket_grpc::GrpcEvent;
        assert_eq!(
            emit_channels(&GrpcEvent::GrpcSessionStarted {
                session_id: "s".into(),
                method_type: "bidi-streaming".into(),
            }),
            vec!["grpc-session-started"]
        );
        assert_eq!(
            emit_channels(&GrpcEvent::GrpcSessionHeaders {
                session_id: "s".into(),
                headers: vec![],
            }),
            vec!["grpc-session-headers"]
        );
        assert_eq!(
            emit_channels(&GrpcEvent::GrpcSessionMessage {
                session_id: "s".into(),
                index: 0,
                json: "{}".into(),
            }),
            vec!["grpc-session-message"]
        );
        assert_eq!(
            emit_channels(&GrpcEvent::GrpcSessionFinished {
                session_id: "s".into(),
                code: 0,
                code_name: "OK".into(),
                message: String::new(),
                trailers: vec![],
                duration_ms: 0,
            }),
            vec!["grpc-session-finished"]
        );
    }

    #[test]
    fn envelope_adds_seq_and_ts_ms_to_the_event_object() {
        let event = CollectionEvent::FileWatcherFailed {
            message: "m".into(),
        };
        let value = envelope(&event, 7, 1234).expect("envelope");
        assert_eq!(
            value,
            serde_json::json!({
                "type": "fileWatcherFailed",
                "message": "m",
                "seq": 7,
                "tsMs": 1234,
            })
        );
    }

    #[test]
    fn envelope_keeps_unit_variants_as_objects() {
        let value = envelope(&AcpEvent::AssistantConversationsCleared, 1, 2).expect("envelope");
        assert_eq!(value["type"], "assistantConversationsCleared");
        assert_eq!(value["seq"], 1);
    }

    #[test]
    fn transfer_events_emit_on_the_network_transfer_channel() {
        let progress = TransferEvent::Progress {
            transfer_id: "t1".into(),
            direction: Direction::Send,
            done_bytes: 10,
            total_bytes: 100,
        };
        assert_eq!(emit_channels(&progress), vec!["network-transfer"]);
        let finished = TransferEvent::Finished {
            transfer_id: "t1".into(),
            direction: Direction::Receive,
            outcome: Outcome::Completed,
            display_name: "Ann".into(),
            collection_name: "Orders".into(),
            message: None,
        };
        assert_eq!(emit_channels(&finished), vec!["network-transfer"]);
    }

    #[test]
    fn transfer_events_carry_seq_and_ts_ms_and_keep_snake_case_fields() {
        let event = TransferEvent::Finished {
            transfer_id: "t1".into(),
            direction: Direction::Receive,
            outcome: Outcome::Declined,
            display_name: "Ann".into(),
            collection_name: "Orders".into(),
            message: None,
        };
        let value = envelope(&event, 7, 1234).expect("envelope");
        assert_eq!(value["type"], "finished");
        assert_eq!(value["seq"], 7);
        assert_eq!(value["tsMs"], 1234);
        assert_eq!(value["direction"], "receive");
        assert_eq!(value["outcome"], "declined");
        assert!(value["message"].is_null(), "no message is a JSON null");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "collection_name",
                "direction",
                "display_name",
                "message",
                "outcome",
                "seq",
                "transfer_id",
                "tsMs",
                "type"
            ]
        );
    }

    #[test]
    fn a_receiver_failure_message_travels_in_the_finished_event() {
        let event = TransferEvent::Finished {
            transfer_id: "t1".into(),
            direction: Direction::Receive,
            outcome: Outcome::Failed,
            display_name: "Ann".into(),
            collection_name: "Orders".into(),
            message: Some("The import failed.".into()),
        };
        let value = envelope(&event, 8, 99).expect("envelope");
        assert_eq!(value["message"], "The import failed.");
        assert_eq!(value["outcome"], "failed");
    }

    #[test]
    fn envelope_does_not_overwrite_existing_keys() {
        let value = add_envelope(serde_json::json!({ "seq": "own", "tsMs": "own" }), 9, 9);
        assert_eq!(value["seq"], "own");
        assert_eq!(value["tsMs"], "own");
    }

    #[test]
    fn seq_is_strictly_increasing() {
        let a = next_seq();
        let b = next_seq();
        let c = next_seq();
        assert!(a < b && b < c);
        assert!(a >= 1);
    }

    fn sync_samples() -> Vec<SyncEvent> {
        let summary = SyncSummary {
            added_here: 1,
            changed_here: 2,
            deleted_here: 0,
            sent_added: 0,
            sent_changed: 1,
            sent_deleted: 0,
            scripts_changed: vec!["users/get.yml".to_string()],
            auth_changed: Vec::new(),
        };
        vec![
            SyncEvent::ReviewReady {
                sync_id: "s1".into(),
                link_id: "l1".into(),
                device_id: "dev".into(),
                short_id: "K7QF-2M9X".into(),
                display_name: "Ann".into(),
                collection_name: "Orders".into(),
                summary: summary.clone(),
                conflicts: vec![ConflictView {
                    path: "users/get.yml".into(),
                    kind: ConflictKind::BothChanged,
                    mine_hash: Some("aa".repeat(32)),
                    theirs_hash: None,
                }],
                role: SyncRole::Planner,
            },
            SyncEvent::ConfirmNeeded {
                sync_id: "s1".into(),
                link_id: "l1".into(),
                device_id: "dev".into(),
                short_id: "K7QF-2M9X".into(),
                display_name: "Ann".into(),
                collection_name: "Orders".into(),
                summary,
            },
            SyncEvent::Progress {
                sync_id: "s1".into(),
                done_bytes: 10,
                total_bytes: 100,
            },
            SyncEvent::Finished {
                sync_id: "s1".into(),
                link_id: "l1".into(),
                display_name: "Ann".into(),
                collection_name: "Orders".into(),
                outcome: SyncFinish::Applied,
                message: None,
            },
            SyncEvent::LinkChanged {
                link_id: "l1".into(),
            },
        ]
    }

    fn sorted_keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn sorted(mut names: Vec<&str>) -> Vec<String> {
        names.sort_unstable();
        names.into_iter().map(str::to_string).collect()
    }

    #[test]
    fn sync_events_emit_on_the_network_sync_channel() {
        for event in sync_samples() {
            assert_eq!(emit_channels(&event), vec!["network-sync"]);
        }
    }

    #[test]
    fn wire_contract_sync_events_keep_the_key_styles() {
        let samples = sync_samples();
        let envelope_of = |event: &SyncEvent| envelope(event, 7, 1234).expect("envelope");

        let review = envelope_of(&samples[0]);
        assert_eq!(review["type"], "reviewReady");
        assert_eq!(review["seq"], 7);
        assert_eq!(review["tsMs"], 1234);
        assert_eq!(
            sorted_keys(&review),
            sorted(vec![
                "collection_name",
                "conflicts",
                "device_id",
                "display_name",
                "link_id",
                "role",
                "seq",
                "short_id",
                "summary",
                "sync_id",
                "tsMs",
                "type"
            ])
        );
        assert_eq!(
            sorted_keys(&review["summary"]),
            sorted(vec![
                "addedHere",
                "authChanged",
                "changedHere",
                "deletedHere",
                "scriptsChanged",
                "sentAdded",
                "sentChanged",
                "sentDeleted"
            ])
        );
        assert_eq!(
            sorted_keys(&review["conflicts"][0]),
            sorted(vec!["kind", "mineHash", "path", "theirsHash"])
        );
        assert!(review["conflicts"][0]["theirsHash"].is_null());
        assert!(review["role"].is_string());

        let confirm = envelope_of(&samples[1]);
        assert_eq!(confirm["type"], "confirmNeeded");
        assert_eq!(
            sorted_keys(&confirm),
            sorted(vec![
                "collection_name",
                "device_id",
                "display_name",
                "link_id",
                "seq",
                "short_id",
                "summary",
                "sync_id",
                "tsMs",
                "type"
            ])
        );

        let progress = envelope_of(&samples[2]);
        assert_eq!(progress["type"], "progress");
        assert_eq!(
            sorted_keys(&progress),
            sorted(vec![
                "done_bytes",
                "seq",
                "sync_id",
                "total_bytes",
                "tsMs",
                "type"
            ])
        );

        let finished = envelope_of(&samples[3]);
        assert_eq!(finished["type"], "finished");
        assert!(finished["message"].is_null(), "no message is a JSON null");
        assert!(finished["outcome"].is_string());
        assert_eq!(
            sorted_keys(&finished),
            sorted(vec![
                "collection_name",
                "display_name",
                "link_id",
                "message",
                "outcome",
                "seq",
                "sync_id",
                "tsMs",
                "type"
            ])
        );

        let changed = envelope_of(&samples[4]);
        assert_eq!(changed["type"], "linkChanged");
        assert_eq!(
            sorted_keys(&changed),
            sorted(vec!["link_id", "seq", "tsMs", "type"])
        );
    }

    #[test]
    fn now_ms_is_after_2020() {
        assert!(now_ms() > 1_577_836_800_000);
    }

    #[test]
    fn failure_log_is_first_then_every_hundredth_per_channel() {
        let failures = Mutex::new(BTreeMap::new());
        assert_eq!(failure_to_log(&failures, "a"), Some(1));
        for _ in 2..100 {
            assert_eq!(failure_to_log(&failures, "a"), None);
        }
        assert_eq!(failure_to_log(&failures, "a"), Some(100));
        assert_eq!(failure_to_log(&failures, "a"), None);
        // Another channel counts on its own.
        assert_eq!(failure_to_log(&failures, "b"), Some(1));
    }
}
