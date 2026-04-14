use rocket_audit::profile::{default_profile, ComplianceProfile, ProfileRepository};
use rocket_shared::error::{DomainError, DomainResult};
use std::{fs, path::PathBuf};

pub struct FsComplianceProfileRepo {
    path: PathBuf,
}

impl FsComplianceProfileRepo {
    pub fn new(path: PathBuf) -> DomainResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self { path })
    }
}

impl ProfileRepository for FsComplianceProfileRepo {
    fn load(&self) -> DomainResult<ComplianceProfile> {
        if !self.path.exists() {
            return Ok(default_profile());
        }
        let raw = fs::read_to_string(&self.path)?;
        serde_yaml::from_str(&raw).map_err(|e| DomainError::Serialization(e.to_string()))
    }

    fn save(&self, profile: &ComplianceProfile) -> DomainResult<()> {
        let raw = serde_yaml::to_string(profile)
            .map_err(|e| DomainError::Serialization(e.to_string()))?;
        fs::write(&self.path, raw)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_audit::{control::Framework, profile::EnforcementLevel};
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_when_missing() {
        let dir = TempDir::new().unwrap();
        let repo = FsComplianceProfileRepo::new(dir.path().join("profile.yml")).unwrap();
        let loaded = repo.load().unwrap();
        assert_eq!(loaded, default_profile());
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let repo = FsComplianceProfileRepo::new(dir.path().join("profile.yml")).unwrap();
        let mut p = default_profile();
        p.active_frameworks.insert(Framework::Soc2);
        p.active_frameworks.insert(Framework::Iso27001);
        p.enforcement = EnforcementLevel::Warn;
        repo.save(&p).unwrap();
        let loaded = repo.load().unwrap();
        assert_eq!(loaded, p);
    }
}
