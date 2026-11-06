# Secrets & vault

Credentials in Rocket have three possible homes, and none of them is your
collection file or your git history:

1. **RocketVault** — Rocket's own secret store.
2. **An environment variable.**
3. **The operating system keychain.**

A collection **references** a secret by name; it never carries the value
itself. That's true whether the secret backs an auth header, an OAuth2 client
secret, a client certificate, or a variable used somewhere else in a request.

## External secret managers

Beyond the three built-in homes, Rocket can connect to an external manager —
currently **RocketVault** and **Azure Key Vault**. These same connections back
both environment secret bindings and AI Agent credentials.

## Quickstart: connect a vault and use a secret in a request

1. Open **Settings** (gear icon, or Ctrl/Cmd+,) → **Secret Managers**.
2. Press **+ Add Connection**, and fill in:
   - **Label** — a name for this connection.
   - **Provider** — for example RocketVault.
   - **Base URL** — the vault's endpoint (for example `https://vault.internal:8774`).
   - **Client ID** and **Client Secret** — its credentials.
   - **Verify SSL** (on by default) and **Allow insecure HTTP** (off by
     default — only matters for a non-loopback host, since loopback is always
     allowed over plain HTTP).

   ![The Add Connection form — provider, base URL, and credentials](/screenshots/add-connection-dialog.png)

   Press **Save**, then **Test** on the connection's row to confirm it
   actually connects before moving on.

   ![Settings → Secret Managers, with a RocketVault and an Azure Key Vault connection](/screenshots/secret-manager-connections.png)
3. If the collection you'll use this in was imported, cloned, or opened from
   somewhere else on disk, note that it starts with RocketVault/vault access
   **denied by default** — Rocket will prompt you to allow it the first time
   something in the collection actually tries to resolve a vault value, rather
   than through a settings page you'd go find ahead of time. See
   [Security & compliance](/guide/security-and-compliance) for why.
4. Open **Manage Environments** for the environment you want the secret in,
   and switch to the **External Secrets** tab.
5. Give the binding an **alias**, pick the **connection** you just created and
   the **vault name**, then press **Fetch Secrets** to list what's available.

   ![Binding an alias to a vault connection, with the fetched secret names available to reference](/screenshots/external-secrets-binding.png)
6. Press **Save**. Reference any fetched secret in a request, header, or
   anywhere else a value is typed, as `{{alias.secret-name}}` — a
   distinct template form from a plain `{{variable}}`, specifically for
   values that live in a vault rather than in the environment file.

An unapproved collection sends a `{{alias.secret}}` placeholder unresolved,
exactly as written, with a Console warning — never a silently leaked value —
until you approve the prompt Rocket shows when it's actually needed.

## Vault-backed client certificates

A client certificate for mTLS can also come straight from RocketVault instead
of a PEM/PKCS12 file — fetched fresh at send time, held in memory only, and
never written to disk or saved into the environment file. See
[Authentication](/guide/authentication) for the other two certificate sources
and how domain matching works.

## Masking, everywhere a secret could leak

- The **Console** (HTTP log) shows masked values by default, with a per-row
  reveal toggle that **never persists the unmasked value to disk** — revealing
  is a view, not a write. A request whose environment hasn't loaded yet shows
  "May contain secrets" with no reveal option at all, rather than risk a false
  negative.
- **History** entries and **Flow** traces mask the same values.
- **AI Assistant** conversations are masked the same way before they're
  written to disk, and before the backend ever sends workspace content to an
  agent as a chat "chip."
- Saved **response examples** are masked for secret-shaped values, auth
  headers and cookies **at commit time**, so a secret captured from a live
  response doesn't end up readable in your git history. See
  [Git integration](/guide/git-integration).

## Exports never carry literal credentials

Exporting a collection (to Bruno or Postman) always blanks literal
credentials — there's no option to include them. `{{variable}}`
references are preserved, so the exported collection still resolves correctly
once pointed at its own secret source.
