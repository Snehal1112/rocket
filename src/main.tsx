import React from 'react';
import ReactDOM from 'react-dom/client';
import './globals.css';
import App from './App';

// Apply theme class synchronously before React renders.
const storedTheme = localStorage.getItem('rocket-theme');
const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
if (storedTheme === 'dark' || (!storedTheme && prefersDark)) {
  document.documentElement.classList.add('dark');
} else {
  document.documentElement.classList.remove('dark');
}
ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
