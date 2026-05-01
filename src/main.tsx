import React from 'react';
import ReactDOM from 'react-dom/client';
import './globals.css';
import App from './App';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

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

// Register Monaco workers before any editor mounts. Only JSON and the base
// editor worker are needed — TypeScript/CSS/HTML workers are not used in an
// API client and each adds several MB to the initial load.
import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
import jsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';

self.MonacoEnvironment = {
  getWorker(_: unknown, label: string) {
    if (label === 'json') return new jsonWorker();
    return new editorWorker();
  },
};

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 2,
      refetchOnWindowFocus: false,
    },
  },
});

// Mount React immediately — Monaco loads on demand when the first editor renders.
ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
