/**
 * Lightweight telemetry abstraction.
 *
 * Calls are intentionally no-ops in this release.  Replace the body of
 * `track` with a real provider (PostHog, Mixpanel, etc.) when one is
 * adopted.  Every call site is wrapped in `try {} catch {}` so provider
 * errors are never surfaced to the user.
 */

// biome-ignore lint/suspicious/noExplicitAny: event properties are intentionally open-ended
export function track(_event: string, _props?: Record<string, any>): void {
  // Wire to an analytics provider here:
  // e.g. posthog.capture(_event, _props)
}
