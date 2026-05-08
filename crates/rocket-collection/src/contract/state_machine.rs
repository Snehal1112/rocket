use super::types::ContractStatus;

pub enum StatusEvent {
    Publish,
    DriftDetected,
    BreachDetected,
    Resign,
    MarkBreaking,
    Pause,
    Resume,
    SendForReview,
    Approve,
    Reject,
    Renew,
    ExpiryLapsed,
    ExpiringSoon,
}

#[derive(Debug, PartialEq)]
pub struct InvalidTransition {
    pub from: ContractStatus,
    pub event: String,
}

pub fn transition(
    current: &ContractStatus,
    event: &StatusEvent,
) -> Result<ContractStatus, InvalidTransition> {
    use ContractStatus::*;
    use StatusEvent::*;

    let next = match (current, event) {
        // Draft lifecycle
        (Draft, Publish) => Active,

        // Active transitions
        (Active, DriftDetected) => Drift,
        (Active, BreachDetected) => Breach,
        (Active, Pause) => Paused,
        (Active, ExpiryLapsed) => Expired,
        (Active, ExpiringSoon) => ExpiringIn30Days,

        // ExpiringIn30Days — same as Active for most events
        (ExpiringIn30Days, DriftDetected) => Drift,
        (ExpiringIn30Days, BreachDetected) => Breach,
        (ExpiringIn30Days, Pause) => Paused,
        (ExpiringIn30Days, ExpiryLapsed) => Expired,

        // Drift transitions
        (Drift, Resign) => Active,
        (Drift, MarkBreaking) => Breach,
        (Drift, Pause) => Paused,
        (Drift, BreachDetected) => Breach,

        // Breach transitions
        (Breach, Resign) => Active,
        (Breach, Pause) => Paused,

        // Paused transitions
        (Paused, Resume) => Active,

        // Expired transitions
        (Expired, Renew) => Active,

        // InReview transitions
        (InReview, Approve) => Active,
        (InReview, Reject) => Draft,

        // SendForReview: valid from Active, Drift, Breach, Paused
        (Active | Drift | Breach | Paused, SendForReview) => InReview,

        // Any status can lapse into Expired
        (_, ExpiryLapsed) => Expired,

        // All other combinations are invalid
        _ => {
            return Err(InvalidTransition {
                from: current.clone(),
                event: format!("{:?}", std::mem::discriminant(event)),
            });
        }
    };

    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_publish_to_active() {
        let result = transition(&ContractStatus::Draft, &StatusEvent::Publish).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn active_drift_detected() {
        let result = transition(&ContractStatus::Active, &StatusEvent::DriftDetected).unwrap();
        assert_eq!(result, ContractStatus::Drift);
    }

    #[test]
    fn active_breach_detected() {
        let result = transition(&ContractStatus::Active, &StatusEvent::BreachDetected).unwrap();
        assert_eq!(result, ContractStatus::Breach);
    }

    #[test]
    fn drift_resign_to_active() {
        let result = transition(&ContractStatus::Drift, &StatusEvent::Resign).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn drift_mark_breaking_to_breach() {
        let result = transition(&ContractStatus::Drift, &StatusEvent::MarkBreaking).unwrap();
        assert_eq!(result, ContractStatus::Breach);
    }

    #[test]
    fn breach_resign_to_active() {
        let result = transition(&ContractStatus::Breach, &StatusEvent::Resign).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn paused_resume_to_active() {
        let result = transition(&ContractStatus::Paused, &StatusEvent::Resume).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn expired_renew_to_active() {
        let result = transition(&ContractStatus::Expired, &StatusEvent::Renew).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn in_review_approve_to_active() {
        let result = transition(&ContractStatus::InReview, &StatusEvent::Approve).unwrap();
        assert_eq!(result, ContractStatus::Active);
    }

    #[test]
    fn in_review_reject_to_draft() {
        let result = transition(&ContractStatus::InReview, &StatusEvent::Reject).unwrap();
        assert_eq!(result, ContractStatus::Draft);
    }

    #[test]
    fn any_status_send_for_review() {
        for status in [
            ContractStatus::Active,
            ContractStatus::Drift,
            ContractStatus::Breach,
            ContractStatus::Paused,
        ] {
            let result = transition(&status, &StatusEvent::SendForReview).unwrap();
            assert_eq!(result, ContractStatus::InReview);
        }
    }

    #[test]
    fn invalid_transition_returns_err() {
        let result = transition(&ContractStatus::Draft, &StatusEvent::DriftDetected);
        assert!(result.is_err());
    }

    #[test]
    fn paused_cannot_drift() {
        let result = transition(&ContractStatus::Paused, &StatusEvent::DriftDetected);
        assert!(result.is_err());
    }
}
