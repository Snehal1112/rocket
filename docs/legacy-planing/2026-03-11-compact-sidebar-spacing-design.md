# Compact Sidebar Spacing Design

## Summary

The collections sidebar should remain compact and Postman-like, but its
spacing system needs to be regularized. The current panel uses mixed row
heights, inconsistent insets, and overlapping indentation strategies
that make the dense layout feel uneven instead of intentional.

This design keeps the existing structure and behavior, and improves the
look and feel by standardizing padding, margin, row rhythm, and action
gutters.

## Current Problem

The sidebar currently mixes several spacing patterns:

- compact header controls with `h-6` and `h-7`
- collection rows with looser vertical padding
- tree nodes using both manual `paddingLeft` and wrapper offsets
- collection tree content using `ml-4` plus border rails
- history rows following a different spacing rhythm from collection rows

The result is a panel that is dense, but not consistently dense.

## Goals

- Preserve the compact Postman-like density.
- Standardize horizontal insets across sections.
- Standardize row heights and vertical rhythm.
- Make tree indentation predictable and visually economical.
- Keep hover actions aligned without causing row jitter.

## Non-Goals

- Redesign sidebar information architecture.
- Add new sidebar features.
- Change data loading or collection behavior.

## Recommended Approach

### 1. Introduce a consistent compact spacing rhythm

Use a small, repeated set of spacing values across the sidebar:

- one section inset
- one compact row height
- one trailing action gutter
- one per-level indentation increment

These values should be applied consistently rather than tuned per row
type.

### 2. Normalize top-level section spacing

The header and search areas should share the same horizontal rhythm as
the content area below them.

Controls can remain compact, but their size and gap choices should feel
deliberate and ordered rather than mixed.

### 3. Unify collection, folder, request, and history row density

Collection rows, folder rows, request rows, and history rows should all
feel like part of the same sidebar system.

Differences in hierarchy should come from indentation, typography,
method color, and active state rather than inconsistent padding.

### 4. Replace ad hoc indentation with one tree rule

Nested tree rows should use one indentation increment per level, with
the active rail and content aligned to that rule.

Avoid combining:

- wrapper margins
- manual left padding bumps
- extra border offsets

for the same nesting effect.

## Layout Rules

### Header

- keep compact tabs and quick actions
- normalize button heights where possible
- keep icon buttons aligned to the same rhythm

### Search

- match content inset
- keep search input compact but not visually cramped

### Collection rows

- one row height
- one internal left/right inset
- stable action zone width
- no hover reflow

### Tree rows

- one indentation increment per level
- same vertical rhythm for folder and request rows
- active rail aligned consistently with row content

### History rows

- reuse compact row spacing language from collections
- keep metadata readable without creating a looser visual block

## Expected Outcome

After the cleanup:

- the sidebar remains dense and efficient
- alignment feels sharper
- nested levels are easier to scan
- action icons feel anchored
- collection and history tabs feel visually related

## Testing

Manual verification should cover:

- multiple collections in collapsed and expanded states
- nested folders and requests
- active request highlight
- hover action alignment
- history tab density
- narrow-width behavior

The feature is complete when the panel feels uniformly compact rather
than unevenly compressed.
