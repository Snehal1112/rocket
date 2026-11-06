# Load testing

Rocket runs load tests against a saved request without leaving the app —
no separate tool, no exporting a collection to run it elsewhere. A **Load
test** sub-tab sits right on the request itself, alongside Params, Scripts and
Assertions, so the test is configured in the same place as everything else
about that request.

## Run shapes

- **Phase-based** — ramp-up, hold, and ramp-down, so you can model a realistic
  traffic curve instead of a flat burst.
- **Fixed concurrency or fixed RPS** — a steady, specified load for a set
  duration.

## While it runs

A live dashboard tracks latency, throughput, error rate and concurrency as the
run progresses, along with percentile stats (p50/p95/p99-style breakdowns) so
a handful of slow outliers don't hide behind a clean average.

![Rocket's load testing dashboard](/screenshots/load-testing.png)

## Afterward

Results export to **HTML, CSV, JSON or PDF** — HTML and PDF for sharing a
readable report, CSV/JSON if you want to pull the numbers into something else.

Load test runs show up in History with their own `load test` run-source badge,
same as [Collection Runner](/guide/collection-runner) runs do.
