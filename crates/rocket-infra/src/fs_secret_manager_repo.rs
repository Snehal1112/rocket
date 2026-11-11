//! Filesystem persistence for RocketVault Secret Manager connections.
//! Stores the full connection list as one flat YAML file, matching the
//! pattern `FsWorkspaceRepo` uses for `workspaces.yml`. Client secrets never
//! appear here — `SecretManagerConnection` has no such field; they live only
//! in the OS keychain (see `KeyringSecretStore::new_vault_connections`,
//! Plan 05's `SecretManagerService`).

use std::fs;
use std::path::PathBuf;

use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::tolerant_list::TolerantList;

pub struct FsSecretManagerRepo {
    path: PathBuf,
}

impl FsSecretManagerRepo {
    /// `path` should point directly at the YAML file (e.g.
    /// `<app_data_dir>/secret_managers.yml`), not a containing directory —
    /// unlike `FsWorkspaceRepo::new`, which takes the app data directory and
    /// joins the filename itself.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read_all(&self) -> DomainResult<TolerantList<SecretManagerConnection>> {
        if !self.path.exists() {
            return Ok(TolerantList::empty());
        }
        let content = fs::read_to_string(&self.path)
            .map_err(|e| DomainError::Io(format!("Failed to read secret_managers.yml: {e}")))?;
        if content.trim().is_empty() {
            // A zero-byte file (e.g. left behind by an interrupted first
            // write) is not an error — treat it the same as "missing".
            return Ok(TolerantList::empty());
        }
        TolerantList::parse(&content, "secret_managers.yml").map_err(|e| {
            DomainError::InvalidInput(format!(
                "Failed to parse secret_managers.yml: {e}. The file may have been written by a newer version of Rocket."
            ))
        })
    }

    fn write_all(&self, list: &TolerantList<SecretManagerConnection>) -> DomainResult<()> {
        let yaml = list.render().map_err(|e| {
            DomainError::InvalidInput(format!("Failed to serialize secret_managers.yml: {e}"))
        })?;
        atomic_write(&self.path, yaml.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write secret_managers.yml: {e}")))
    }
}

impl SecretManagerRepository for FsSecretManagerRepo {
    fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
        Ok(self.read_all()?.items)
    }

    fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
        Ok(self.read_all()?.items.into_iter().find(|c| c.id == id))
    }

    fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
        let mut list = self.read_all()?;
        list.items.retain(|c| c.id != connection.id);
        list.items.push(connection.clone());
        self.write_all(&list)
    }

    fn delete(&self, id: &str) -> DomainResult<()> {
        let mut list = self.read_all()?;
        list.items.retain(|c| c.id != id);
        self.write_all(&list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::SecretProviderKind;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsSecretManagerRepo) {
        let dir = TempDir::new().expect("create temp dir");
        let repo = FsSecretManagerRepo::new(dir.path().join("secret_managers.yml"));
        (dir, repo)
    }

    fn sample(id: &str) -> SecretManagerConnection {
        SecretManagerConnection {
            id: id.to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: Default::default(),
            config: None,
        }
    }

    #[test]
    fn list_on_missing_file_returns_empty() {
        let (_dir, repo) = setup();
        assert_eq!(repo.list().expect("list"), Vec::new());
    }

    #[test]
    fn get_on_missing_file_returns_none() {
        let (_dir, repo) = setup();
        assert_eq!(repo.get("conn-1").expect("get"), None);
    }

    #[test]
    fn save_get_list_delete_roundtrip() {
        let (_dir, repo) = setup();
        let conn = sample("conn-1");
        repo.save(&conn).expect("save");

        assert_eq!(repo.get("conn-1").expect("get"), Some(conn.clone()));
        assert_eq!(repo.list().expect("list"), vec![conn.clone()]);

        repo.delete("conn-1").expect("delete");
        assert_eq!(repo.get("conn-1").expect("get after delete"), None);
        assert!(repo.list().expect("list after delete").is_empty());
    }

    #[test]
    fn save_replaces_existing_entry_with_same_id_instead_of_duplicating() {
        let (_dir, repo) = setup();
        repo.save(&sample("conn-1")).expect("save first");

        let mut updated = sample("conn-1");
        updated.label = "Renamed".to_string();
        repo.save(&updated).expect("save update");

        let all = repo.list().expect("list");
        assert_eq!(all.len(), 1, "same id must replace, not append");
        assert_eq!(all[0].label, "Renamed");
    }

    #[test]
    fn save_appends_distinct_ids() {
        let (_dir, repo) = setup();
        repo.save(&sample("conn-1")).expect("save conn-1");
        repo.save(&sample("conn-2")).expect("save conn-2");
        assert_eq!(repo.list().expect("list").len(), 2);
    }

    #[test]
    fn delete_of_missing_id_is_a_no_op() {
        let (_dir, repo) = setup();
        repo.save(&sample("conn-1")).expect("save");
        repo.delete("no-such-id")
            .expect("delete of missing id must not error");
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    #[test]
    fn an_old_file_with_no_provider_loads_as_rocketvault() {
        let (dir, repo) = setup();
        std::fs::write(
            dir.path().join("secret_managers.yml"),
            "- id: old-1\n  label: Prod\n  base_url: https://v:8774\n  client_id: rocketapi\n  verify_ssl: true\n  allow_insecure_http: false\n",
        )
        .expect("write old file");

        let rows = repo.list().expect("old file loads");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider, SecretProviderKind::RocketVault);
        assert!(rows[0].config.is_none());
    }

    #[test]
    fn a_saved_rocketvault_row_has_no_provider_key_on_disk() {
        let (dir, repo) = setup();

        repo.save(&sample("c1")).expect("save");

        let text =
            std::fs::read_to_string(dir.path().join("secret_managers.yml")).expect("read back");
        assert!(
            !text.contains("provider"),
            "older builds must read this file: {text}"
        );
        assert!(
            !text.contains("config"),
            "older builds must read this file: {text}"
        );
    }

    #[test]
    fn a_row_with_an_unknown_provider_is_hidden_not_turned_into_rocketvault() {
        let (dir, repo) = setup();
        let path = dir.path().join("secret_managers.yml");
        std::fs::write(&path, "- id: x\n  label: X\n  provider: bogus\n").expect("write");

        assert_eq!(repo.list().expect("list"), Vec::new());
        assert_eq!(repo.get("x").expect("get"), None);

        repo.save(&sample("conn-1")).expect("save");
        let raw = std::fs::read_to_string(&path).expect("read");
        assert!(
            raw.contains("bogus"),
            "the entry must stay as written: {raw}"
        );
    }

    #[test]
    fn persisted_yaml_never_contains_client_secret() {
        // `SecretManagerConnection` (rocket-environment, Plan 01) has no
        // `client_secret` field — the client secret lives only in the OS
        // keychain (Plan 05's `SecretManagerService`, via
        // `KeyringSecretStore::new_vault_connections()`). This is really a
        // compile-time guarantee since the struct has no such field to leak;
        // this test is a defensive regression guard so a future edit that
        // added a secret-bearing field to `SecretManagerConnection` would be
        // caught here, at the point it would first reach disk in cleartext.
        let (dir, repo) = setup();
        repo.save(&sample("conn-1")).expect("save");

        let raw = std::fs::read_to_string(dir.path().join("secret_managers.yml"))
            .expect("read persisted file");
        assert!(
            !raw.contains("client_secret"),
            "secret_managers.yml must never contain a client_secret field: {raw}"
        );
    }

    #[test]
    fn an_entry_from_a_newer_build_is_hidden_but_kept_on_save() {
        let (dir, repo) = setup();
        let path = dir.path().join("secret_managers.yml");
        std::fs::write(
            &path,
            "- id: x\n  label: X\n  base_url: https://v\n  client_id: a\n  provider: azure\n  config: !FutureProvider {region: eu}\n",
        )
        .expect("write");

        assert_eq!(
            repo.list().expect("an unknown config tag must not fail"),
            Vec::new()
        );

        repo.save(&sample("conn-1")).expect("save");
        let raw = std::fs::read_to_string(&path).expect("read");
        assert!(
            raw.contains("FutureProvider"),
            "the newer entry was lost: {raw}"
        );
        assert_eq!(repo.list().expect("list").len(), 1);

        repo.delete("conn-1").expect("delete");
        let raw = std::fs::read_to_string(&path).expect("read");
        assert!(
            raw.contains("FutureProvider"),
            "the newer entry was lost: {raw}"
        );
    }

    #[test]
    fn text_that_is_not_a_list_is_still_an_error() {
        let (dir, repo) = setup();
        std::fs::write(dir.path().join("secret_managers.yml"), "id: x\n").expect("write");
        let err = repo.list().expect_err("a map is not a list");
        assert!(
            err.to_string().contains("secret_managers.yml"),
            "got: {err}"
        );
    }
}
