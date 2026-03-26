# Area D: Response Viewer Enhancements — Reference Spec

**Date:** 2026-03-26
**Status:** Reference — not yet scheduled for implementation
**Goal:** Enhance response viewing for better analysis and debugging.

## Improvements

### 1. Response body search (high priority)
Add a search input (Ctrl+F) to the response body viewer. For Monaco-based views (Pretty, Raw), use Monaco's built-in search. For Preview (iframe), use window.find or highlight.js.

### 2. JSON path click-to-copy (medium priority)
In Pretty view for JSON responses, clicking a key or value copies its JSON path (e.g., `data.users[0].name`) to clipboard. Show a brief tooltip "Copied: data.users[0].name."

### 3. Response download (medium priority)
"Save as..." button to write response body to a file. Auto-suggest filename from Content-Disposition header or URL path. Use Tauri save dialog.

### 4. Image preview (low priority)
When Content-Type is `image/*`, show the response as an inline image instead of raw binary text. Supports PNG, JPEG, GIF, SVG, WebP.

### 5. Response headers sorting (low priority)
Add sort toggle (A-Z / Z-A) to the response headers table.

## Files

| File | Changes |
|---|---|
| `src/components/response/ResponseBodyViewer.tsx` | Search, download, image preview |
| `src/components/response/ResponseHeadersTable.tsx` | Sort toggle |
| `src/components/request/RequestPanel.tsx` | Ctrl+F shortcut routing |
