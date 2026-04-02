# Remove App Root Blue Gradient

**Date:** 2026-04-02  
**Status:** Approved  
**Scope:** Single class change in `src/App.tsx`

## Overview

The root app container has a `bg-gradient-to-br from-background via-background to-accent/25` that produces a subtle blue tint bleeding into the bottom-right corner of the center panel. Remove it in favour of a flat `bg-background`.

## Change

**File:** `src/App.tsx`, line 79

**Before:**
```tsx
<div className="h-full flex flex-col overflow-hidden bg-gradient-to-br from-background via-background to-accent/25 text-sm">
```

**After:**
```tsx
<div className="h-full flex flex-col overflow-hidden bg-background text-sm">
```

## Out of Scope

No other files need changes. This is the only gradient in the codebase.
