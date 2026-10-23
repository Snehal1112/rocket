# Example masking at commit

Saved request examples (`examples:` in a request `.yml`) can hold tokens and cookies from real responses. `Git2Service::commit` masks them in the commit, not on disk.

- Code: `crates/rocket-git/src/sanitize.rs` (`mask_examples`), called from `mask_staged_examples` in `git2_service/staging.rs`.
- Scope: only staged `.yml`/`.yaml` files that changed against HEAD. Only the `examples:` block is touched. `http.auth` and the rest of the file are not.
- Masked (the value becomes `[masked]`):
  - Header and param values whose name looks secret (authorization, cookie, token, secret, password, apikey, ...). `Set-Cookie` and `Cookie` values are masked by header name, whatever the cookie name inside.
  - Body values by body type: `json` (values under secret keys), `form-urlencoded` and `multipart-form` (secret-named fields), and `text`, `xml`, `html`, `sparql` (secret-named `a=b&c=d` fields, secret-named XML elements, a body that holds JSON is masked as JSON, bracketed non-JSON text gets the word pass). Binary and file bodies are never edited.
  - Secret-shaped values under any name. In header values, param values, query values and whole JSON string values: JWTs, `Bearer ` or `Basic ` plus a value, `sk_live_`, `sk_test_`, `ghp_`, `gho_`, `ghs_`, `github_pat_`, `xoxb-`, `xoxp-`, `xoxa-`, `xoxr-`, `xoxs-` plus a value, `AKIA` plus 16 uppercase letters or digits, and PEM private key blocks. In free text (text, XML, HTML bodies) only the one-word shapes are masked, as whole words: JWTs, the prefixed tokens and `AKIA` keys. There is no generic "long random string" rule, because it would mask ids and hashes.
  - Secret query values in the example URL, and URL userinfo (`https://user:pass@host` becomes `https://[masked]@host`). Host and path stay readable.
- The index copy is rewritten, so the working tree keeps the real values. After a commit, a masked file shows as modified in the git panel, because disk and HEAD differ. This is expected.
- `CommitInfo.maskedFiles` lists the masked files. The git store shows a toast.
- Not covered: `Bearer`/`Basic` values and PEM blocks inside free text (text, XML, HTML bodies), because the word pass splits on whitespace; secrets in non-secret-named keys whose values have no known shape, `http.auth.*` of the request itself, and multipart file parts.
- Examples are kept by `save_request` (`fs_collection/requests.rs`). See `.claude/response-examples.md` (added with the example UI).
