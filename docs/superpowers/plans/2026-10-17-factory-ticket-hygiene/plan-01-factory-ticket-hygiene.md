# Factory Ticket Hygiene, Plan 1: Issue References, Follow-ups and the Merge Footer

> **For agentic workers:** This plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file. Steps use checkbox (`- [ ]`) syntax. The driver commits, so there are no commit steps.

**Goal:** Read issue references from `#N` and GitHub issue URLs, read the `## Follow-ups` bullets of a spec, and write `Relates to` instead of `Fixes` in the merge footer when a spec has follow-ups.

**Architecture:** `factory-core` gets `IssueRef` and `parse_issue_refs`; `parse_issues` keeps its type and now also returns URL numbers. `factory-engine` gets a small `followups.rs` module built on the existing `section_body` and `is_list_item` helpers, and `MessageRequest` gets a `partial` flag that `driver_footers` turns into the footer key.

**Tech Stack:** Rust (the `tools/factory` workspace, a separate cargo workspace). All cargo commands run from `tools/factory/`.

**Spec:** `docs/superpowers/specs/2026-10-17-factory-ticket-hygiene-design.md`

## Global Constraints

- Run only `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>` from `tools/factory/`. Never a bare `cargo check -j4`, never `--workspace`, never `--all`.
- No unwrap calls in production code (tests may use `expect`). Comments are short full sentences ending with a period. No emojis.
- `parse_issues` still returns a sorted, de-duplicated `Vec<u32>`. The board JSON, the ledger format and `/api/state` do not change. No board field is added.
- A spec without `## Follow-ups` gives the same merge message as today (`Fixes: #N` when the feature text has issues).
- Ruling: `parse_issues` now includes the number of a GitHub issue URL even when the URL names another repository, because the driver does not know the repository at job creation. The tickets step (plan 2) is what refuses a foreign repository.

## Review Focus

- A URL that ends a sentence (`.../issues/76.`) or sits in parentheses still yields 76. Pinned by `url_followed_by_punctuation` in `commitmsg.rs`.
- A URL with a fragment (`.../issues/76#issuecomment-5`) yields 76 once, not twice. Pinned by `url_with_fragment_counts_once`.
- A URL that is not an issue URL (`https://github.com/o/r/pull/9`) yields nothing. Pinned by `pull_request_url_is_not_an_issue`.
- A follow-up bullet spanning several lines is one item, and a blank line ends it. Pinned by `multi_line_bullet_is_one_item` and `blank_line_ends_a_bullet`.
- A bullet with no full stop is cut at 100 characters on a character boundary, not a byte boundary. Pinned by `long_title_is_cut_at_100_chars` (uses a multi-byte character).

---

### Task 1: parse_issue_refs in factory-core

**Files:**
- Modify: `tools/factory/crates/factory-core/src/commitmsg.rs` (the `parse_issues` function near line 59, and its tests module)
- Modify: `tools/factory/crates/factory-core/src/lib.rs` (the `pub use commitmsg::{...}` list)

**Interfaces:**
- Produces: `pub struct IssueRef { pub number: u32, pub repo: Option<String> }` (derives `Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash`; `repo` is `owner/repo` for a URL and `None` for `#N`), and `pub fn parse_issue_refs(text: &str) -> Vec<IssueRef>` (sorted by number then repo, de-duplicated). `parse_issues(text) -> Vec<u32>` keeps its signature.

- [ ] **Step 1: Write the failing tests.** Open the `#[cfg(test)] mod tests` block of `commitmsg.rs` and add these tests (keep the existing ones):

```rust
    #[test]
    fn refs_read_hash_numbers_and_urls() {
        let refs = parse_issue_refs("fix #20 and https://github.com/Snehal1112/rocket/issues/76 now");
        assert_eq!(
            refs,
            vec![
                IssueRef { number: 20, repo: None },
                IssueRef { number: 76, repo: Some("Snehal1112/rocket".to_string()) },
            ]
        );
    }

    #[test]
    fn url_followed_by_punctuation() {
        let refs = parse_issue_refs("see (https://github.com/o/r/issues/76).");
        assert_eq!(refs, vec![IssueRef { number: 76, repo: Some("o/r".to_string()) }]);
        let refs = parse_issue_refs("https://github.com/o/r/issues/76.");
        assert_eq!(refs, vec![IssueRef { number: 76, repo: Some("o/r".to_string()) }]);
    }

    #[test]
    fn url_with_fragment_counts_once() {
        let refs = parse_issue_refs("https://github.com/o/r/issues/76#issuecomment-5");
        assert_eq!(refs, vec![IssueRef { number: 76, repo: Some("o/r".to_string()) }]);
    }

    #[test]
    fn pull_request_url_is_not_an_issue() {
        assert!(parse_issue_refs("https://github.com/o/r/pull/9").is_empty());
        assert!(parse_issue_refs("https://example.com/o/r/issues/9").is_empty());
    }

    #[test]
    fn refs_are_sorted_and_deduplicated() {
        let refs = parse_issue_refs("#9 #3 #9 https://github.com/o/r/issues/3");
        assert_eq!(
            refs,
            vec![
                IssueRef { number: 3, repo: None },
                IssueRef { number: 3, repo: Some("o/r".to_string()) },
                IssueRef { number: 9, repo: None },
            ]
        );
    }

    #[test]
    fn parse_issues_keeps_hash_behavior_and_adds_url_numbers() {
        assert_eq!(parse_issues("fix #20, #21 and #20"), vec![20, 21]);
        assert_eq!(parse_issues("no issues here"), Vec::<u32>::new());
        assert_eq!(parse_issues("abc#5 and #0 and #x"), Vec::<u32>::new());
        assert_eq!(parse_issues("https://github.com/o/r/issues/76 and #5"), vec![5, 76]);
    }
```

- [ ] **Step 2: Run the tests to see them fail.** From `tools/factory/`: `cargo test -j4 -p factory-core refs_read_hash_numbers_and_urls`. Expected: compile error, `IssueRef` and `parse_issue_refs` are not defined.

- [ ] **Step 3: Implement.** In `commitmsg.rs`, replace the whole `parse_issues` function (and its doc comment) with this block:

```rust
/// One issue reference found in text. `repo` is `owner/repo` when the text was a GitHub issue URL.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IssueRef {
    pub number: u32,
    pub repo: Option<String>,
}

const GITHUB_PREFIX: &str = "https://github.com/";

/// True for a GitHub owner or repository name: letters, digits, `-`, `_` and `.`.
fn is_repo_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// The `#N` references. The `#` must start the text or follow a character that is not alphanumeric
/// or `_`, and N must be at least 1 and fit `u32`.
fn hash_refs(text: &str) -> Vec<IssueRef> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        if *c != '#' {
            continue;
        }
        if i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
            continue;
        }
        let digits: String = chars[i + 1..].iter().take_while(|d| d.is_ascii_digit()).collect();
        if let Ok(number) = digits.parse::<u32>() {
            if number >= 1 {
                found.push(IssueRef { number, repo: None });
            }
        }
    }
    found
}

/// The `https://github.com/<owner>/<repo>/issues/N` references.
fn url_refs(text: &str) -> Vec<IssueRef> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(GITHUB_PREFIX) {
        let tail = &rest[at + GITHUB_PREFIX.len()..];
        let mut parts = tail.splitn(4, '/');
        let (owner, repo, kind, after) = (parts.next(), parts.next(), parts.next(), parts.next());
        if let (Some(owner), Some(repo), Some("issues"), Some(after)) = (owner, repo, kind, after) {
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(number) = digits.parse::<u32>() {
                if number >= 1 && is_repo_name(owner) && is_repo_name(repo) {
                    found.push(IssueRef { number, repo: Some(format!("{owner}/{repo}")) });
                }
            }
        }
        rest = tail;
    }
    found
}

/// Every issue reference in the text, `#N` and GitHub issue URLs, sorted and de-duplicated.
pub fn parse_issue_refs(text: &str) -> Vec<IssueRef> {
    let mut refs = hash_refs(text);
    refs.extend(url_refs(text));
    refs.sort();
    refs.dedup();
    refs
}

/// Every issue number in the text, sorted and de-duplicated. `#N` and GitHub issue URLs both count.
pub fn parse_issues(text: &str) -> Vec<u32> {
    let mut numbers: Vec<u32> = parse_issue_refs(text).into_iter().map(|r| r.number).collect();
    numbers.sort_unstable();
    numbers.dedup();
    numbers
}
```

In `factory-core/src/lib.rs` add `IssueRef` and `parse_issue_refs` to the `pub use commitmsg::{ ... }` list (keep the list sorted the way it is now: put `parse_issue_refs` before `parse_issues`, and `IssueRef` before `MAX_COMMITS_PER_RUN`).

- [ ] **Step 4: Run the tests.** `cargo test -j4 -p factory-core commitmsg` then `cargo check -j4 -p factory-core`. Expected: all pass, including the older `parse_issues` tests.

### Task 2: follow_ups in factory-engine

**Files:**
- Create: `tools/factory/crates/factory-engine/src/followups.rs`
- Modify: `tools/factory/crates/factory-engine/src/runloop.rs` (make `section_body` and `is_list_item` `pub(crate)`)
- Modify: `tools/factory/crates/factory-engine/src/lib.rs` (add `mod followups;` and `pub use followups::{follow_ups, FollowUp};`)

**Interfaces:**
- Consumes: `crate::runloop::section_body(spec: &str, title: &str) -> Option<String>` and `crate::runloop::is_list_item(line: &str) -> bool`.
- Produces: `pub struct FollowUp { pub title: String, pub body: String }` (derives `Debug, Clone, PartialEq, Eq`) and `pub fn follow_ups(spec: &str) -> Vec<FollowUp>`.

- [ ] **Step 1: Make the helpers crate-visible.** In `runloop.rs` change `fn section_body(` to `pub(crate) fn section_body(` and `fn is_list_item(` to `pub(crate) fn is_list_item(`. Check that `runloop` is declared as `mod runloop;` in `lib.rs` (it is), so `crate::runloop::section_body` resolves.

- [ ] **Step 2: Write the failing tests.** Create `followups.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_bullet() {
        let spec = "## Goal\nx\n\n## Follow-ups\n\n- Update the tier tracker tickets.\n\n## Acceptance criteria\n- a\n";
        let items = follow_ups(spec);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Update the tier tracker tickets");
        assert_eq!(items[0].body, "Update the tier tracker tickets.");
    }

    #[test]
    fn several_bullets_with_different_markers() {
        let spec = "## Follow-ups\n- First thing. More detail.\n* Second thing\n1. Third thing\n";
        let titles: Vec<String> = follow_ups(spec).into_iter().map(|f| f.title).collect();
        assert_eq!(titles, vec!["First thing", "Second thing", "Third thing"]);
    }

    #[test]
    fn multi_line_bullet_is_one_item() {
        let spec = "## Follow-ups\n- Merge QueryParam and Header\n  into one helper for both types.\n- Other\n";
        let items = follow_ups(spec);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Merge QueryParam and Header into one helper for both types");
        assert_eq!(items[0].body, "Merge QueryParam and Header into one helper for both types.");
    }

    #[test]
    fn blank_line_ends_a_bullet() {
        let spec = "## Follow-ups\n- One\n\nstray paragraph\n- Two\n";
        let items = follow_ups(spec);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].body, "One");
    }

    #[test]
    fn no_section_and_empty_section_give_nothing() {
        assert!(follow_ups("## Goal\nx\n").is_empty());
        assert!(follow_ups("## Follow-ups\n\n## Acceptance criteria\n- a\n").is_empty());
    }

    #[test]
    fn bullets_inside_a_code_fence_are_ignored_by_the_section_reader() {
        let spec = "## Goal\n```\n## Follow-ups\n- fake\n```\n";
        assert!(follow_ups(spec).is_empty());
    }

    #[test]
    fn long_title_is_cut_at_100_chars() {
        let long = "\u{e9}".repeat(150);
        let spec = format!("## Follow-ups\n- {long}\n");
        let items = follow_ups(&spec);
        assert_eq!(items[0].title.chars().count(), 100);
        assert_eq!(items[0].body.chars().count(), 150);
    }
}
```

- [ ] **Step 3: Run it to see it fail.** Add `mod followups;` to `lib.rs` first so the file compiles into the crate, then run `cargo test -j4 -p factory-engine followups`. Expected: compile error, `follow_ups` is not defined.

- [ ] **Step 4: Implement.** Put this at the top of `followups.rs`, above the test module:

```rust
//! The `## Follow-ups` section of a spec: one item per bullet, the pieces of a ticket that are not built yet.

use crate::runloop::{is_list_item, section_body};

/// Longest ticket title, in characters.
const MAX_TITLE_CHARS: usize = 100;

/// One remaining piece of work. The title is the first sentence of the bullet, the body is the whole bullet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    pub title: String,
    pub body: String,
}

/// The bullet text without its marker (`- `, `* ` or `1. `).
fn strip_marker(line: &str) -> &str {
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
        return rest.trim();
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    t[digits..].strip_prefix(". ").unwrap_or(t).trim()
}

/// The first sentence of a bullet without its closing full stop, cut at 100 characters.
fn title_of(body: &str) -> String {
    let first = body.split_once(". ").map_or(body, |(head, _)| head);
    let first = first.trim().trim_end_matches('.').trim();
    first.chars().take(MAX_TITLE_CHARS).collect::<String>().trim_end().to_string()
}

/// The bullets under `## Follow-ups`. A continuation line joins the bullet above it, a blank line ends
/// the bullet. A missing or empty section gives an empty list.
pub fn follow_ups(spec: &str) -> Vec<FollowUp> {
    let Some(section) = section_body(spec, "Follow-ups") else {
        return Vec::new();
    };
    let mut bodies: Vec<String> = Vec::new();
    let mut open = false;
    for line in section.lines() {
        if line.trim().is_empty() {
            open = false;
        } else if is_list_item(line) {
            bodies.push(strip_marker(line).to_string());
            open = true;
        } else if open {
            if let Some(last) = bodies.last_mut() {
                last.push(' ');
                last.push_str(line.trim());
            }
        }
    }
    bodies.into_iter().filter(|b| !b.is_empty()).map(|body| FollowUp { title: title_of(&body), body }).collect()
}
```

Add `pub use followups::{follow_ups, FollowUp};` to the re-export block of `lib.rs`.

- [ ] **Step 5: Run the tests.** `cargo test -j4 -p factory-engine followups` and `cargo check -j4 -p factory-engine`. Expected: all pass. If `one_bullet` fails on the body, note that the body keeps the closing full stop and the title does not.

### Task 3: Merge footer key from follow-ups

**Files:**
- Modify: `tools/factory/crates/factory-engine/src/message.rs` (`MessageRequest`, `driver_footers`, tests)
- Modify: `tools/factory/crates/factory-engine/src/compat.rs` (`Driver::commit_message`)

**Interfaces:**
- Consumes: `crate::follow_ups` from Task 2.
- Produces: `MessageRequest.partial: bool` ("true when the job's spec lists follow-ups, so the merge footer must not close the ticket"), and `pub(crate) fn spec_has_follow_ups(job_dir: &std::path::Path) -> bool` in `message.rs`.

- [ ] **Step 1: Write the failing tests.** In the `tests` module of `message.rs` add:

```rust
    #[test]
    fn merge_footer_relates_to_when_the_spec_has_follow_ups() {
        let req = MessageRequest { kind: MessageKind::Merge, job_id: "001", slug: "s", title: "t", base: "a", tip: "b", issues: &[76], partial: true };
        assert_eq!(driver_footers(&req), vec!["Relates to: #76".to_string(), "Factory-Job: 001".to_string()]);
        let full = MessageRequest { partial: false, ..req };
        assert_eq!(driver_footers(&full), vec!["Fixes: #76".to_string(), "Factory-Job: 001".to_string()]);
    }

    #[test]
    fn spec_has_follow_ups_reads_spec_md_of_the_job_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!spec_has_follow_ups(dir.path()), "no spec.md means no follow-ups");
        std::fs::write(dir.path().join("spec.md"), "## Goal\nx\n").expect("write");
        assert!(!spec_has_follow_ups(dir.path()));
        std::fs::write(dir.path().join("spec.md"), "## Follow-ups\n- One\n").expect("write");
        assert!(spec_has_follow_ups(dir.path()));
    }
```

Also add `partial: false` to the four existing `MessageRequest { ... }` literals in this test module (lines using `issues: &[]` and `issues: &[20]`); the struct-update ones (`..req`) need no change.

- [ ] **Step 2: Run to see them fail.** `cargo test -j4 -p factory-engine merge_footer_relates_to`. Expected: compile error, no field `partial`.

- [ ] **Step 3: Implement.** In `message.rs`:
  - Add to `MessageRequest`, after `issues`:

```rust
    /// True when the job's spec lists follow-ups, so the merge footer says `Relates to` and does not close the ticket.
    pub partial: bool,
```

  - In `driver_footers` change the `Merge` arm to `MessageKind::Merge if req.partial => "Relates to", MessageKind::Merge => "Fixes",` and update the doc comment to mention that.
  - Add above `fixed_message`:

```rust
/// True when `spec.md` in the job folder has at least one `## Follow-ups` bullet. A missing file is false.
pub(crate) fn spec_has_follow_ups(job_dir: &Path) -> bool {
    std::fs::read_to_string(job_dir.join("spec.md")).map(|spec| !crate::follow_ups(&spec).is_empty()).unwrap_or(false)
}
```

In `compat.rs` `Driver::commit_message`, add `partial: crate::message::spec_has_follow_ups(&self.job_dir(&job)),` to the `MessageRequest` literal (after `issues: &job.issues,`). `message` is a private module of the crate, so use the path `crate::message::spec_has_follow_ups`.

- [ ] **Step 4: Run the tests.** `cargo test -j4 -p factory-engine message` (runs the unit tests and the `tests/message.rs` integration tests, which must still pass) and `cargo check -j4 -p factory-engine`. Expected: all pass.

Next plan: docs/superpowers/plans/2026-10-17-factory-ticket-hygiene/plan-02-factory-ticket-hygiene.md
