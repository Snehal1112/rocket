//! Security audit domain: events, compliance profiles, and tamper-evident log trait.

pub mod chain;
pub mod control;
pub mod event;
pub mod profile;
pub mod publisher;
pub mod repository;

pub use chain::{hash_event, verify_chain, ChainVerification};
pub use control::{ControlId, Framework, CONTROL_CATALOG};
pub use event::{AuditEventId, AuditEventKind, SecurityAuditEvent};
pub use profile::{ComplianceProfile, EnforcementLevel, ProfileRepository};
pub use publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher};
pub use repository::AuditLogRepository;
