import React from "react";
import ReactDOM from "react-dom/client";
import { loader } from "@monaco-editor/react";
import "./index.css";
import App from "./App";

// Apply theme class synchronously before React renders.
// This prevents a flash of wrong theme in Monaco and other components
// that read document.documentElement.classList during initialization.
const storedTheme = localStorage.getItem('rocket-theme');
const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
if (storedTheme === 'dark' || (!storedTheme && prefersDark)) {
  document.documentElement.classList.add('dark');
} else {
  document.documentElement.classList.remove('dark');
}

// Bundle Monaco entirely from the local package — no CDN dependency.
// Workers are loaded via Vite's ?worker import, and the editor core
// is passed to @monaco-editor/react's loader via dynamic import.
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import cssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import htmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import tsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";

self.MonacoEnvironment = {
  getWorker(_: unknown, label: string) {
    if (label === "json") return new jsonWorker();
    if (label === "css" || label === "scss" || label === "less") return new cssWorker();
    if (label === "html" || label === "handlebars" || label === "razor") return new htmlWorker();
    if (label === "typescript" || label === "javascript") return new tsWorker();
    return new editorWorker();
  },
};

// Load monaco-editor locally and pass it to @monaco-editor/react's loader.
// This prevents any CDN fetch — the entire editor is bundled by Vite.
import("monaco-editor").then((monaco) => {
  loader.config({ monaco });

  // Define custom themes before any editor renders to avoid the flash of
  // wrong theme on first mount.
  monaco.editor.defineTheme('rocket-light', {
    base: 'vs',
    inherit: true,
    rules: [
      { token: 'string', foreground: 'a31515' },
      { token: 'string.key.json', foreground: '0451a5' },
      { token: 'number', foreground: '098658' },
      { token: 'keyword', foreground: '0000ff' },
      { token: 'comment', foreground: '008000' },
      { token: 'type', foreground: '267f99' },
      { token: 'variable', foreground: '001080' },
      { token: 'constant', foreground: '0070c1' },
    ],
    colors: {
      'editor.background': '#ffffff',
      'editor.foreground': '#000000',
      'editor.lineHighlightBackground': '#add6ff26',
      'editorLineNumber.foreground': '#237893',
      'editorLineNumber.activeForeground': '#0b216f',
    },
  });

  monaco.editor.defineTheme('rocket-dark', {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'string', foreground: 'ce9178' },
      { token: 'string.key.json', foreground: '9cdcfe' },
      { token: 'number', foreground: 'b5cea8' },
      { token: 'keyword', foreground: '569cd6' },
      { token: 'comment', foreground: '6a9955' },
      { token: 'type', foreground: '4ec9b0' },
      { token: 'variable', foreground: '9cdcfe' },
      { token: 'constant', foreground: '4fc1ff' },
    ],
    colors: {
      'editor.background': '#1f1f1f',
      'editor.foreground': '#d4d4d4',
      'editor.lineHighlightBackground': '#2a2d2e',
      'editorLineNumber.foreground': '#858585',
      'editorLineNumber.activeForeground': '#c6c6c6',
    },
  });

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});
