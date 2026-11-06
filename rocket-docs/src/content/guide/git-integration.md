# Git integration

Because a collection is just YAML files on disk, it works with git the normal
way — and Rocket builds git support directly into the app rather than asking
you to shell out to a terminal.

## What's built in

Staging, commits, branches, merges, push/pull/fetch (including
force-with-lease), stash, remote management, SSH key discovery, and real
per-file conflict resolution. It's implemented on `libgit2` (the `git2` Rust
crate) — a real embedded implementation, not a wrapper that shells out to the
`git` CLI underneath.

## The Git UI tab

A workspace has its own **Git UI** tab (alongside Overview, Environments and
Audit Log). It shows the current branch, a commit-message box with **Commit**,
and the **Unstaged Changes** list with a count. Below that: **Commits**,
**Stashes** and **Remotes**. The main panel shows **Fetch**/**Pull**/**Push**
(with a dropdown for force-with-lease and similar options), how many commits
you're ahead, and when the branch was last fetched.

![The Git UI's branch overview, with Fetch/Pull/Push and the commit-ahead count](/screenshots/git-overview.png)

Rocket's own **History** entries (saved request-run records) live as files
under `history/`, so a run you've made can itself show up as an untracked
change in the Unstaged Changes list — worth knowing before a commit, since
"22 unstaged changes" can include history files, not just edited requests.

## Reviewing a diff

Selecting a changed file opens a side-by-side diff with **Text**/**Visual**
and **Working**/**Staged** toggles — Text shows a classic line-level diff
(additions in green, removals in red), useful for confirming exactly what a
save changed, for example in a test script or a `folder.yml`.

![A Working-vs-Staged text diff of a changed file](/screenshots/git-diff-view.png)

## Secrets don't leak through committed examples

Saved response examples can capture real tokens, cookies or JWTs from a live
API call — and those are exactly the kind of thing that ends up accidentally
committed by API clients that don't think about it. Rocket masks
secret-shaped values, auth headers, and cookies **at commit time**, in the
commit itself, not just at rest in the app. See
[Secrets & vault](/guide/secrets-and-vault) and
[Response examples](/guide/response-examples) for where this matters most.
