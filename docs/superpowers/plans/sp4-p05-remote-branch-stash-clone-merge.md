# SP4-P05: Git2 Remote, Branch, Stash, Clone, Merge

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement all remaining `Git2Service` methods: push/pull/fetch, branch CRUD + merge, stash (save/pop/apply/drop), clone, and conflict resolution. Replace all `todo!()` stubs.

**Tech Stack:** Rust, git2

**Prerequisite:** SP4-P04 complete.

---

## Task 1: Branch operations + merge

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn branch_create_switch_delete() {
    let (_dir, path) = setup_repo();
    let svc = Git2Service::new();
    svc.create_branch(&path, "feature-x").unwrap();
    let branches = svc.branches(&path).unwrap();
    assert!(branches.local.iter().any(|b| b.name == "feature-x"));
    svc.switch_branch(&path, "feature-x").unwrap();
    assert_eq!(svc.status(&path).unwrap().branch, "feature-x");
    svc.switch_branch(&path, "main").unwrap();
    svc.delete_branch(&path, "feature-x").unwrap();
    let branches2 = svc.branches(&path).unwrap();
    assert!(!branches2.local.iter().any(|b| b.name == "feature-x"));
}

#[test]
fn merge_branch_fast_forward() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();
    svc.create_branch(&path, "feature").unwrap();
    svc.switch_branch(&path, "feature").unwrap();
    fs::write(dir.path().join("new.bru"), "content").unwrap();
    svc.stage(&path, &["new.bru"]).unwrap();
    svc.commit(&path, "feature commit").unwrap();
    svc.switch_branch(&path, "main").unwrap();
    svc.merge_branch(&path, "feature").unwrap();
    let log = svc.log(&path, 5).unwrap();
    assert!(log.iter().any(|c| c.message == "feature commit"));
}
```

- [ ] **Step 2: Implement branches, switch_branch, create_branch, delete_branch, merge_branch**

- `branches`: iterate `repo.branches(None)`, classify local vs remote
- `switch_branch`: `repo.set_head("refs/heads/{name}")` + checkout
- `create_branch`: `repo.branch(name, &head_commit, false)`
- `delete_branch`: `repo.find_branch(name, Local)?.delete()`
- `merge_branch`: find branch commit, `repo.merge(&[&annotated_commit])`, if fast-forward → checkout, else create merge commit

- [ ] **Step 3: Run tests, commit**

```bash
cargo test -p rocket-git -- git2_service::tests
git commit -am "feat(git): Git2 branch CRUD + merge"
```

---

## Task 2: Stash operations (save/pop/apply/drop) + remote (push/pull/fetch) + clone

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Write failing tests for stash**

```rust
#[test]
fn stash_save_and_pop() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();
    fs::write(dir.path().join("test.bru"), "changed for stash").unwrap();
    svc.stash_save(&path, "WIP").unwrap();
    let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
    assert_eq!(content, "meta { name: Test }"); // reverted
    let stashes = svc.stash_list(&path).unwrap();
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.contains("WIP"));
    svc.stash_pop(&path, 0).unwrap();
    let content2 = fs::read_to_string(dir.path().join("test.bru")).unwrap();
    assert_eq!(content2, "changed for stash"); // restored
}

#[test]
fn stash_apply_keeps_stash() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();
    fs::write(dir.path().join("test.bru"), "stash this").unwrap();
    svc.stash_save(&path, "keep me").unwrap();
    svc.stash_apply(&path, 0).unwrap();
    let stashes = svc.stash_list(&path).unwrap();
    assert_eq!(stashes.len(), 1); // still there
    let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
    assert_eq!(content, "stash this"); // restored
}
```

- [ ] **Step 2: Implement stash methods**

- `stash_save`: `repo.stash_save(&sig, message, None)`
- `stash_pop`: `repo.stash_pop(index, None)`
- `stash_apply`: `repo.stash_apply(index, None)`
- `stash_drop`: `repo.stash_drop(index)`
- `stash_list`: `repo.stash_foreach(|index, message, oid| { ... })`

- [ ] **Step 3: Implement push/pull/fetch**

- `push`: find remote, build refspec, call `remote.push()` with credential callbacks
- `pull`: `fetch` + merge (fast-forward or merge commit)
- `fetch`: find remote, `remote.fetch()` with credential callbacks
- `clone_repo`: `git2::build::RepoBuilder::new().clone(url, dest_path)` with credential callbacks

Credential callback maps `GitCredentials` to git2 types:
```rust
fn build_callbacks(creds: &GitCredentials) -> git2::RemoteCallbacks {
    let mut callbacks = git2::RemoteCallbacks::new();
    match creds {
        GitCredentials::SshKey { private_key_path, passphrase } => {
            callbacks.credentials(|_url, username, _allowed| {
                git2::Cred::ssh_key(
                    username.unwrap_or("git"),
                    None,
                    Path::new(private_key_path),
                    passphrase.as_deref(),
                )
            });
        }
        GitCredentials::SshAgent => {
            callbacks.credentials(|_url, username, _allowed| {
                git2::Cred::ssh_key_from_agent(username.unwrap_or("git"))
            });
        }
        GitCredentials::UserPass { username, password } => {
            callbacks.credentials(|_url, _username, _allowed| {
                git2::Cred::userpass_plaintext(username, password)
            });
        }
        GitCredentials::Token { token } => {
            callbacks.credentials(|_url, _username, _allowed| {
                git2::Cred::userpass_plaintext("oauth2", token)
            });
        }
    }
    callbacks
}
```

Note: push/pull/fetch tests require a remote — mark with `#[ignore]` for CI.

- [ ] **Step 4: Implement conflict detection + resolution**

- `conflicts`: `repo.index()?.conflicts()`, for each entry read ours/theirs/ancestor blob content
- `resolve_conflict`: based on resolution enum, write the chosen content to file, `index.add_path()`, `index.conflict_remove()`

- [ ] **Step 5: Verify NO `todo!()` remains**

```bash
grep -rn "todo!()" crates/rocket-git/src/git2_service.rs
```
Expected: no results.

- [ ] **Step 6: Run all tests, full workspace check**

```bash
cargo test -p rocket-git
cargo test --workspace
cargo clippy --workspace
```

- [ ] **Step 7: Commit**

```bash
git commit -am "feat(git): Git2 remote, stash (incl apply), clone, merge, conflicts — all todo!() resolved"
```

---

## Milestone Checklist — P05

- [ ] `branches`, `switch_branch`, `create_branch`, `delete_branch` — tested
- [ ] `merge_branch` — fast-forward tested
- [ ] `stash_save`, `stash_pop`, `stash_apply`, `stash_drop`, `stash_list` — tested (apply keeps stash)
- [ ] `push`, `pull`, `fetch` — implemented with credential callbacks
- [ ] `clone_repo` — implemented
- [ ] `conflicts`, `resolve_conflict` — implemented
- [ ] Zero `todo!()` remaining in git2_service.rs
- [ ] `cargo test --workspace` passes
