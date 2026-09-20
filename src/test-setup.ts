import '@testing-library/jest-dom';

// Polyfill ResizeObserver for tests (used by @radix-ui/react-scroll-area).
if (typeof global.ResizeObserver === 'undefined') {
  class ResizeObserverPolyfill {
    observe() {
      // No-op for test polyfill.
    }
    unobserve() {
      // No-op for test polyfill.
    }
    disconnect() {
      // No-op for test polyfill.
    }
  }
  // biome-ignore lint/suspicious/noExplicitAny: intentional polyfill.
  global.ResizeObserver = ResizeObserverPolyfill as any;
}
