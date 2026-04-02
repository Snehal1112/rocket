# Plan 1 — VariableContext (Rust)

> **For agentic workers:** Use `superpowers:subagent-driven-development`.
> Read `docs/superpowers/specs/variables-design.md` before starting.

**Depends on:** Plan 0  
**Spec:** `docs/superpowers/specs/variables-design.md`

**Goal:** Add `VariableContext` struct to `rocket-environment` crate for merging all 7 scopes with the correct priority order.

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-environment/src/context.rs` | New — VariableContext struct |
| `crates/rocket-environment/src/lib.rs` | Export VariableContext |

---

### Task 1: Implement VariableContext

- [ ] **Step 1: Write failing tests in context.rs**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test] fn env_beats_collection() {
        let ctx = VariableContext { env: m(&[("k","env")]), collection: m(&[("k","col")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "env");
    }
    #[test] fn folder_beats_env() {
        let ctx = VariableContext { folder: m(&[("k","folder")]), env: m(&[("k","env")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "folder");
    }
    #[test] fn request_beats_folder() {
        let ctx = VariableContext { request: m(&[("k","req")]), folder: m(&[("k","folder")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "req");
    }
    #[test] fn runtime_beats_all() {
        let ctx = VariableContext { runtime: m(&[("k","rt")]), request: m(&[("k","req")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "rt");
    }
    #[test] fn global_beats_nothing_but_process() {
        let ctx = VariableContext { global_env: m(&[("k","global")]), collection: m(&[("k","col")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "col"); // collection beats global
    }
    #[test] fn process_env_uses_dotted_key() {
        let ctx = VariableContext { process_env: m(&[("API_KEY","secret")]), ..Default::default() };
        let flat = ctx.flatten_with_process_env();
        assert!(flat.get("API_KEY").is_none());
        assert_eq!(flat.get("process.env.API_KEY").unwrap(), "secret");
    }
    #[test] fn env_beats_global() {
        let ctx = VariableContext { env: m(&[("t","env")]), global_env: m(&[("t","global")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("t").unwrap(), "env");
    }
    #[test] fn empty_is_empty() {
        assert!(VariableContext::default().flatten().is_empty());
    }
    #[test] fn folder_chain_innermost_wins() {
        // folder field already contains merged result from backend chain walk
        let ctx = VariableContext { folder: m(&[("k","inner")]), env: m(&[("k","env")]), ..Default::default() };
        assert_eq!(ctx.flatten().get("k").unwrap(), "inner");
    }
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cargo test -p rocket-environment -- context::tests 2>&1 | tail -5
```

- [ ] **Step 3: Implement**

```rust
// crates/rocket-environment/src/context.rs
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    pub runtime:     HashMap<String, String>,
    pub request:     HashMap<String, String>,
    pub folder:      HashMap<String, String>,
    pub env:         HashMap<String, String>,
    pub collection:  HashMap<String, String>,
    pub global_env:  HashMap<String, String>,
    pub process_env: HashMap<String, String>,
}

impl VariableContext {
    /// Merge all scopes. Insertion order: global → collection → env → folder → request → runtime.
    /// Each later layer overwrites earlier on collision. process_env excluded (use flatten_with_process_env).
    pub fn flatten(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        out.extend(self.global_env.clone());
        out.extend(self.collection.clone());
        out.extend(self.env.clone());
        out.extend(self.folder.clone());
        out.extend(self.request.clone());
        out.extend(self.runtime.clone());
        out
    }

    /// Same as flatten() but also includes process.env.KEY prefixed entries (lowest priority).
    pub fn flatten_with_process_env(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for (k, v) in &self.process_env {
            out.insert(format!("process.env.{}", k), v.clone());
        }
        out.extend(self.global_env.clone());
        out.extend(self.collection.clone());
        out.extend(self.env.clone());
        out.extend(self.folder.clone());
        out.extend(self.request.clone());
        out.extend(self.runtime.clone());
        out
    }
}
```

- [ ] **Step 4: Export from lib.rs**

```rust
pub mod context;
pub use context::VariableContext;
```

- [ ] **Step 5: Run tests — expect PASS**

```bash
cargo test -p rocket-environment -- context::tests
cargo clippy -p rocket-environment
```

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-environment/src/context.rs crates/rocket-environment/src/lib.rs
git commit -m "feat(environment): VariableContext — merges all 7 scopes with correct priority"
```

---
