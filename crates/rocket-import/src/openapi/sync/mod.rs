//! Three-way merge of a synced request: the baseline, the user's copy and the new spec.

pub(crate) mod json_merge;
pub(crate) mod merge;

pub use merge::{
    merge_request, request_is_edited, Choice, FieldChange, FieldOutcome, MergeOutcome,
};
