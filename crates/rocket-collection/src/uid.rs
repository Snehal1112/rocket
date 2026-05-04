/// The single source of truth for Request and Folder UID generation.
///
/// All persistence layers must call this rather than minting UUIDs directly,
/// so the format and policy are owned by the domain.
pub fn generate_uid() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_uid_is_uuid_v4() {
        let s = generate_uid();
        let parsed: uuid::Uuid = s.parse().expect("must parse as uuid");
        assert_eq!(parsed.get_version_num(), 4);
        assert_ne!(s, generate_uid());
    }
}
