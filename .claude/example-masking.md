# Example masking at commit

Saved request examples (`examples:` in a request `.yml`) can hold tokens and cookies from real responses. `Git2Service::commit` masks them in the commit, not on disk.

- Code: `crates/rocket-git/src/sanitize.rs` (`mask_examples`), called from `mask_staged_examples` in `git2_service/staging.rs`.
- Scope: only staged `.yml`/`.yaml` files that changed against HEAD. Only the `examples:` block is touched. `http.auth` and the rest of the file are not.
- Masked: header and param values whose name looks secret (authorization, cookie, token, secret, password, apikey, ...), secret query values in the example URL, JSON body values under secret keys, and any JWT-shaped JSON string. The value becomes `[masked]`.
- The index copy is rewritten, so the working tree keeps the real values. After a commit, a masked file shows as modified in the git panel, because disk and HEAD differ. This is expected.
- `CommitInfo.maskedFiles` lists the masked files. The git store shows a toast.
- Not covered: non-JSON bodies, secrets in non-secret-named keys, and `http.auth.token` in the request itself.
- Examples are kept by `save_request` (`fs_collection/requests.rs`). There is no UI to create or edit them yet.
