# Environments & variables

Rocket templates requests with `{{variable}}` syntax, resolved from the active
**environment** at send time. Environments live alongside the collection, so
switching from "Local" to "Staging" is a dropdown, not an edit to every
request.

## Manage Environments

The Manage Environments dialog lists every environment on the left (for
example `local`, `staging`, `production`) and, for the selected one, an
**Inherits from** dropdown, a `.env` file path, and three tabs: **Variables**,
**External Secrets** and **Certificates**.

![The Manage Environments dialog, Variables tab](/screenshots/environment-variables.png)

Each variable row has an enabled checkbox, a key, a value, a per-row **reveal**
toggle (the eye icon) and a delete button. A secret-looking value starts
masked; toggling reveal is a view only — it's never written anywhere unmasked.

## Environments that extend another

An environment can **extend** another one via **Inherits from**, inheriting
its variables. A disabled child variable shadows the parent's variable of the
same key; a collection's `.env` file wins over the global environment's file,
and the real OS environment wins over both. Only variables are inherited —
external secret bindings and client certificates are not (yet) inherited
across the `extends` chain.

## `.env` files

Besides an environment's own variables, Rocket reads a `.env` file: the
default `<collection>/.env` (and a workspace base `.env`) are read even with
no environment selected, as the lowest layer; an environment's explicit `.env`
file (set via the folder-picker next to **.env file**) overrides those. Every
`.env` value is masked in the Console the same way a secret variable is, with
the same reveal toggle. Reading any `.env` file at all requires this
collection to have host-environment access allowed — see
[Security & compliance](/guide/security-and-compliance).

## External secrets — binding a vault

The **External Secrets** tab binds an **alias** to a secret-manager connection
and a vault name. Click **Fetch Secrets** to pull the list of available secret
names from that vault, then reference one in any field as
`{{alias.secret-name}}` — a different template form from a plain
`{{variable}}`, used specifically for vault-backed values.

![The External Secrets tab, with a vault connection bound to the alias "pass" and its fetched secret names](/screenshots/external-secrets-binding.png)

The connections themselves (RocketVault, Azure Key Vault, …) are configured
once in Settings → Secret Managers — see [Secrets & vault](/guide/secrets-and-vault).
A collection must be allowed to use RocketVault values before
`{{alias.secret}}` placeholders resolve; an unapproved collection sends
the placeholder text unresolved with a Console warning instead of silently
leaking a value.

## Client certificates

The **Certificates** tab attaches an mTLS client identity to the environment —
see [Authentication](/guide/authentication) for the three ways to supply one
and how domain matching works.

## Secrets stay masked

Any variable backed by a secret is masked everywhere it would otherwise show
up in plaintext:

- The **Console** (the HTTP log) shows masked values by default, with a
  per-row reveal toggle. Revealing a value is a view — it is never persisted
  to disk unmasked. A request whose environment isn't loaded yet shows
  "May contain secrets" instead, with no reveal available, rather than risk
  showing something unmasked.
- **History** entries and **Flow** traces mask the same values.

Where the real value actually comes from is covered in
[Secrets & vault](/guide/secrets-and-vault) — an environment variable binding
is one of several homes a secret can have, and none of them is the collection
file itself.
