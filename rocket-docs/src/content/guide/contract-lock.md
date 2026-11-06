# Contract Lock

Contract Lock attaches an SLA or API contract to a collection, a folder, or a
single request, so the contract travels with the API it describes instead of
living in a separate document nobody checks. Everything lives on disk in the
workspace as files — contracts are shared and reviewed through git, the same
as the rest of a collection, not through a Rocket server or account.

## The Contracts dashboard

Right-click a collection in the sidebar and choose **Manage contracts** to
open its Contracts tab — the same menu you'd use for New Request or Export.

A collection's **Contracts** tab opens with five live stats: **Total** (and
how many are still drafts), **Active & healthy**, **Drifting** (needs review),
**Breaching** (consumer at risk), and **Changes** in the last 30 days. Below
that, a searchable, filterable, sortable list of every contract.

![The Contracts dashboard, with its status stats and a contract card](/screenshots/contracts-overview.png)

Each contract card shows its version, scope, provider → consumer teams, the
effective date, expiry (or "No expiry · evergreen"), its breaking-change
policy and notice period, how many endpoints it monitors, and a running
"breaks in the last 30 days" count — plus a recent-changes panel and
**Open contract**/**Edit**/**…** actions.

## Lifecycle

A contract moves through **Draft → Active → Drift/Breach/InReview/Paused →
ExpiringIn30Days → Expired → Archived** as its underlying API and the
collection's requests change relative to what it promised.

## Creating or editing a contract

Both **New contract** and **Edit** open the same form:

![The contract form — scope, dates, breaking-change policy and SLA fields](/screenshots/contract-edit-form.png)

- **Title**, **Version**, **Provider team**, and one or more comma-separated
  **Consumer team(s)**.
- **Scope** — Collection, a specific Folder, or a single Request.
- **Effective date** and an optional **Expiry date**.
- **Breaking change policy** — Strict, Lenient, or Additive OK — which governs
  what counts as a breach versus acceptable drift.
- **Notice period** (days) and an optional **Uptime SLA** (%).
- **Save as Draft**, to park a contract before publishing it, or
  **Create & Publish** to make it active immediately.

## What it tracks

- **Drift** — a changelog of what moved against what the contract promised.
- **PDF preview** — contracts render natively, no separate viewer needed.
- **Export as OpenAPI YAML** — a contract can be exported in a standard,
  portable format rather than staying locked inside Rocket.
- **Sync** — refresh a contract's monitored endpoints and compliance status
  against the collection's current requests.

## Why attach it to a collection at all

A contract that lives next to the requests it governs is easier to keep
honest — when a request changes, the drift shows up against the same
collection, not in a document that was last opened at signing time. For the
broader audit trail this feeds into, see
[Security & compliance](/guide/security-and-compliance).
