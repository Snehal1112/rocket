# Export name-based scrub: plan index

Spec: `docs/superpowers/specs/2026-10-23-export-name-scrub-design.md`

Run the plans in order. Each plan has at most 3 tasks.

| Plan | Tasks | Result |
|---|---|---|
| [plan-01-shared-rule-and-bruno.md](plan-01-shared-rule-and-bruno.md) | 1 name rule, 2 rows and URL query pairs, 3 OAuth URLs and variables | The Bruno ZIP blanks secret-like rows, URL queries, OAuth URL fields and variables by name |
| [plan-02-postman-and-docs.md](plan-02-postman-and-docs.md) | 4 Postman rows, URLs and variables, 5 OAuth1 slot, notes, environments, dialog, docs | The Postman export matches, the dialog and docs state the coverage |

Related: GitHub issue #120 (item 8, the origin of this work) and #121.
