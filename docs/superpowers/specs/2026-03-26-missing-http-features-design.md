# Area C: Missing HTTP Features — Reference Spec

**Date:** 2026-03-26
**Status:** Reference — not yet scheduled for implementation
**Goal:** Add HTTP features that exist in the backend but lack frontend UI, plus critical missing capabilities.

## Features

### 1. Form Data file upload (high priority)
BodyEditor.tsx supports `formdata` mode but only renders text key-value fields. The `FormDataEntry` type has `entryType: 'text' | 'file'` but the file option is not in the UI. Add a toggle per entry to switch between text and file, with a Tauri file picker for file selection.

### 2. Cookie jar viewer (medium priority)
Backend already manages cookies per collection. Add a "Cookies" tab in RequestPanel showing cookies for the current collection with view/edit/delete capabilities.

### 3. Request history browser (medium priority)
Backend tracks request execution history. Wire the existing `HistoryPanel` component to show request/response pairs with timestamps. Allow re-sending historical requests.

### 4. Response download (medium priority)
Add a "Download" button to ResponseBodyViewer that saves the response body to a file via Tauri's save dialog. Detect Content-Type for file extension.

### 5. AWS Signature v4 backend (low priority)
AuthEditor.tsx has full AWS Sig v4 UI but backend logs "not supported, falling back to none." Either implement the signing logic in Rust or clearly disable/label the UI option as "Coming Soon."

### 6. cURL import (low priority)
Parse cURL commands and populate the request editor. Useful for developers copying cURL from browser DevTools.

## Files

| File | Changes |
|---|---|
| `src/components/request/BodyEditor.tsx` | File upload toggle in FormData mode |
| `src/components/request/RequestPanel.tsx` | Add Cookies tab |
| `src/components/response/ResponseBodyViewer.tsx` | Download button |
| `src/components/history/HistoryPanel.tsx` | Wire to request panel |
| `crates/rocket-app/src/collection_service.rs` | AWS Sig v4 implementation |
