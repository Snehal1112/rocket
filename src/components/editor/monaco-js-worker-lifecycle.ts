import * as monacoNs from 'monaco-editor';

// Monaco's TypeScript/JavaScript language-service worker (a full TS compiler
// running in a Web Worker, ~50-60MB once loaded) has no idle-stop timer and
// is shared process-wide across every JS/TS-language editor in the app
// (ScriptsTab, ResponseBodyViewer, DiffViewer). It only tears down via a
// private WorkerManager method with no public entry point — the only public
// way to trigger that teardown is to re-apply the current compiler options,
// which fires `onDidChange` and forces the worker to restart lazily on next
// use. This module ref-counts every mounted JS/TS editor across the app so
// the worker is released only once none of them are visible.
let jsWorkerRefCount = 0;

// Teardown must not run synchronously inside releaseJsWorker(). Re-applying
// the compiler options fires `onDidChange`, which also drives the
// diagnostics adapter to recompute diagnostics for every still-live JS/TS
// model; if any model is still alive at that instant, that recomputation
// immediately recreates the worker (see workerManager.js's createWebWorker),
// silently undoing the teardown. This matters because @monaco-editor/react
// disposes its own model inside its own unmount effect cleanup, and React
// runs passive-effect cleanups parent-first — so a release triggered from an
// ancestor's cleanup (the natural call site) can run while the model is
// still alive. Deferring to a macrotask lets the whole passive-effect flush
// (including the model disposal) finish first, and re-checking the ref
// count when the deferred callback fires lets a new acquire that arrives
// during the deferral window (e.g. React StrictMode's dev mount -> cleanup
// -> remount) cancel the teardown.
let teardownScheduled = false;

export function acquireJsWorker(): void {
  jsWorkerRefCount += 1;
}

export function releaseJsWorker(): void {
  if (jsWorkerRefCount === 0) return;
  jsWorkerRefCount -= 1;
  if (jsWorkerRefCount === 0 && !teardownScheduled) {
    teardownScheduled = true;
    setTimeout(() => {
      teardownScheduled = false;
      if (jsWorkerRefCount === 0) {
        const defaults = monacoNs.typescript.javascriptDefaults;
        defaults.setCompilerOptions(defaults.getCompilerOptions());
      }
    }, 0);
  }
}
