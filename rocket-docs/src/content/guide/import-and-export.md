# Import & export

Moving to Rocket, or away from it, is meant to cost one dialog, not a rebuild
of your collection.

## Importing

Rocket imports from **Postman**, **Bruno**, **WSDL**, and **OpenAPI** (3.0,
3.1, and Swagger 2.0).

### Quickstart: import an existing collection

1. From a workspace's Overview tab, press **Import Collection** (or use the
   same option from the collection menu if you're importing into an existing
   workspace).
2. Pick the source format at the top of the dialog: **Bruno**, **Postman**,
   **WSDL**, or **OpenAPI**.
3. Choose **Link** or **File** as the source, then paste the URL (a GitHub
   file link works directly) or pick the file from disk. For a private GitHub
   link, add an **Access token** — it's kept in your OS keychain, never
   written into the collection.
4. For an OpenAPI/WSDL source, choose **Group folders by Tag or Path**, and
   decide whether to leave **Check for spec updates** on (it is, by default) —
   see [OpenAPI Sync](/guide/openapi-sync) for what that does afterward.
5. Press **Import**. The format, and whether it's a link or a file, is
   auto-detected from there — Rocket reports anything it couldn't bring over
   cleanly rather than silently dropping it.

![Rocket's Import Collection dialog, with Bruno, Postman, WSDL and OpenAPI source tabs](/screenshots/import-collection.png)

OpenAPI and WSDL links also pull in any remote `$ref` files the spec points
at, and you can choose to import into the current workspace or create a new
one on the way in.

### Known import gaps worth knowing about

- A Bruno 3 import copies request, `folder.yml` and environment files byte for
  byte rather than normalizing them — most real-world collections import
  cleanly, but if a file genuinely can't be read back afterward it's reported
  in the import results rather than silently dropped.
- There's no `folder.bru` importer yet — a Bruno collection's folder-level
  settings (headers/auth/scripts set on a folder, not a request) don't carry
  over on import.

## Exporting

Collections export back out as a **Bruno ZIP** or a **Postman Collection
v2.1** file, with Postman environments bundled when the collection has them.

### Quickstart: export a collection

1. Right-click the collection in the sidebar and choose **Export...**.
2. Pick **Bruno** or **Postman** as the output format.
3. For a Postman export, decide whether to turn on **Preserve scripts** (keeps
   the original `rok.*` text instead of translating it to `pm.*`). Leave
   **Include environments** on (the default) to bundle them.
4. Save the file. Afterward, the dialog lists what was exported and what was
   left out, grouped by action — check that list before handing the file off.

A few things to know about what survives the trip:

- **Literal credentials are always blanked** on export — there's no option to
  include them. A secret variable whose value is a plain `{{variable}}`
  reference keeps that reference intact.
- Any field whose *name* looks sensitive (`token`, `secret`, `password`,
  `authorization`, `cookie`, `api key`, and similar) is blanked by name alone,
  even if Rocket doesn't know its actual value is secret — headers, query
  params, form fields and variables are all covered. A credential with an
  innocuous name like `session_id` won't be caught by this rule, since the
  check is name-based, not content-based.
- Scripts are translated from `rok.*` to `pm.*` where a mapping exists;
  anything without one keeps a marker and is called out in the export report.
- **Flows, saved examples, gRPC and WebSocket requests, client certificates
  and proxy settings are not exported** — Postman and Bruno's own formats have
  no equivalent for them, and each omission is listed in the post-export
  report so nothing goes missing silently.

## Keeping an imported OpenAPI collection in sync

A collection imported from OpenAPI doesn't have to drift from its spec
afterward — see [OpenAPI Sync](/guide/openapi-sync) for reviewing and applying
spec changes over time.
