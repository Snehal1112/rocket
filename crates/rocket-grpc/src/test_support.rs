//! Shared fixtures for the unit tests in this crate.

use std::collections::HashMap;
use std::sync::Arc;

use crate::registry::{ProtoFileReader, ProtoRegistry};

pub const COMMON_PROTO: &str = r#"syntax = "proto3";
package demo.common;
enum Mood { MOOD_UNSPECIFIED = 0; HAPPY = 1; GRUMPY = 2; }
message Address { string street = 1; string city = 2; }
"#;

pub const GREETER_PROTO: &str = r#"syntax = "proto3";
package demo.greeter.v1;
import "common.proto";
import "google/protobuf/timestamp.proto";

service Greeter {
  rpc SayHello (HelloRequest) returns (HelloReply);
  rpc ListGreetings (HelloRequest) returns (stream HelloReply);
  rpc CollectNames (stream HelloRequest) returns (HelloReply);
  rpc Chat (stream HelloRequest) returns (stream HelloReply);
}

message HelloRequest {
  string name = 1;
  repeated string tags = 2;
  demo.common.Mood mood = 3;
  map<string, int64> counters = 4;
  oneof contact { string email = 5; string phone = 6; }
  demo.common.Address address = 7;
  google.protobuf.Timestamp sent_at = 8;
  bytes blob = 9;
  int64 big = 10;
  repeated demo.common.Address stops = 11;
  map<int32, demo.common.Address> by_id = 12;
}

message HelloReply { string message = 1; int32 sequence = 2; }
"#;

pub struct MemReader(pub HashMap<String, String>);

impl ProtoFileReader for MemReader {
    fn read(&self, name: &str) -> Option<String> {
        self.0.get(name).cloned()
    }
}

pub fn reader(files: &[(&str, &str)]) -> Arc<MemReader> {
    Arc::new(MemReader(
        files
            .iter()
            .map(|(n, s)| (n.to_string(), s.to_string()))
            .collect(),
    ))
}

pub fn greeter_registry() -> ProtoRegistry {
    ProtoRegistry::compile(
        "greeter.proto",
        reader(&[
            ("greeter.proto", GREETER_PROTO),
            ("common.proto", COMMON_PROTO),
        ]),
    )
    .expect("greeter fixture compiles")
}
