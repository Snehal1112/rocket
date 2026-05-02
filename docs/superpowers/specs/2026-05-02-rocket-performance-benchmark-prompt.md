# Rocket Performance Benchmark Prompt

**Purpose:** A self-contained prompt to feed to an AI agent (Claude, GPT-4, etc.) that will design, execute, and report a structured performance comparison of Rocket vs Postman vs Bruno across six dimensions.

---

## The Prompt

```
You are a systematic benchmark analyst. Your task is to design and execute a reproducible performance comparison of three HTTP API client tools — Rocket, Postman, and Bruno — and produce a scored comparison matrix plus a narrative report.

---

## Tools Under Test

**Rocket**
- A Tauri desktop app (Rust backend + React/TypeScript frontend).
- Load testing is built-in via the Load Test tab (simple and advanced modes).
- Advanced mode uses a phase-based V2 streaming runtime (run_load_test_v2_command).
- Simple mode uses a Hold phase derived from concurrency + totalRequests settings.
- Metrics exposed: p50/p95/p99 latency, req/sec, error rate, active concurrent users.
- CLI access: none — measurements must be taken from within the desktop UI or from Tauri's event stream.

**Postman**
- Electron-based desktop app with a Collection Runner for basic load testing.
- For sustained load testing, Postman requires the newman CLI or Postman's cloud-based load testing (paid tier).
- Use the free desktop app + newman CLI for all measurements in this benchmark.
- Install: `npm install -g newman`

**Bruno**
- Electron-based desktop app with a bru CLI for scripted runs.
- Native load testing is not built in — simulate concurrency with parallel `bru run` processes or use the `--reporter` flag with custom scripts.
- Install: `npm install -g @usebruno/cli`

---

## Environment Requirements

Before starting, record and include in your report:
- OS name and version
- CPU model, core count, and clock speed
- Total RAM
- Node.js version (`node --version`)
- Rust version (`rustc --version`) — if available
- Rocket version (from the app's About dialog or package.json)
- Postman desktop version
- Bruno desktop version and bru CLI version (`bru --version`)
- newman version (`newman --version`)

All three tools must run on the same machine. No other network-intensive processes should be running during measurement. Close all browser tabs and unrelated applications.

---

## Test Servers

### Server A — Localhost Echo Server (latency precision)

Start a minimal HTTP echo server that returns 200 OK with a small JSON body. Use this exact implementation to eliminate variance from external networks:

```bash
# Node.js — save as echo-server.js and run with: node echo-server.js
const http = require('http');
http.createServer((req, res) => {
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({ ok: true, method: req.method, path: req.url }));
}).listen(4444, () => console.log('Echo server on http://localhost:4444'));
```

Verify it is running before starting any measurements: `curl http://localhost:4444/ping`

### Server B — Real Public API (realistic conditions)

Use `https://httpbin.org/get` for all real-API measurements. This endpoint returns a JSON object with request metadata and is stable, free, and suitable for benchmarking. If httpbin.org is unavailable, fall back to `https://jsonplaceholder.typicode.com/posts/1`.

---

## Benchmark Methodology

Apply these rules to every measurement:

1. **Warm-up:** Before each timed measurement, send 10 requests to the target endpoint and discard the results. This ensures connection pools are warm and JIT compilation effects are excluded.
2. **Sample size:** Use a minimum of 100 requests for single-request latency tests; 60-second runs for load tests.
3. **Repetitions:** Run each measurement 3 times and report the median result. Discard the highest outlier if it deviates more than 2× from the median.
4. **Isolation:** Restart each tool between measurement sets. Do not run two tools simultaneously.
5. **Concurrency setting:** For all load tests, use 25 concurrent users / workers.
6. **Timing precision:** Use millisecond precision throughout. Do not round to seconds.

---

## Dimension 1: Request Execution Latency

**Goal:** Measure the end-to-end round-trip latency of a single GET request as seen by each tool.

**Target:** Server A (localhost:4444/ping) and Server B (httpbin.org/get) — run separately, report both.

**Method:**

For each tool, send 100 sequential GET requests (no concurrency, no load test mode) to the target endpoint. Record the latency reported by each tool's own response timing:
- **Rocket:** The response time shown in the response panel (ms).
- **Postman:** The "Time" field in the response panel (ms).
- **Bruno:** The response time shown in the response panel or `bru run` output.

Collect all 100 values, then compute: p50, p95, p99, min, max, mean.

If a tool does not natively report per-request latency, use the system clock:
```bash
# Example for any HTTP tool with curl as a fallback check:
for i in $(seq 1 100); do
  curl -o /dev/null -s -w "%{time_total}\n" http://localhost:4444/ping
done
```

**Report:** p50, p95, p99 latency (ms) per tool per server. Lower is better.

---

## Dimension 2: Load Test Throughput

**Goal:** Measure the maximum sustained request rate each tool achieves under concurrent load.

**Target:** Server A (localhost:4444/ping) for throughput measurement. Run Server B separately.

**Method:**

Run a 60-second load test at 25 concurrent users on each tool:

- **Rocket (Advanced mode):** Create a single Hold phase: duration=60s, concurrency=25. Run via the Load Test tab. Record the "req/sec" value from the StatBar at the end of the run, and the average req/sec from the time series.
- **Newman (Postman equivalent):** `newman run <collection.json> --iteration-count 9999 --delay-request 0 -n 25 2>&1 | grep "req/s"` — run for 60 seconds, then terminate. Record req/sec from newman's summary.
- **Bruno CLI:** Simulate 25 concurrent workers by running 25 parallel `bru run` processes pointing at a single-request collection, for 60 seconds. Use `time` and count total completions to compute req/sec.

Record: total requests completed, total duration, average req/sec, peak req/sec.

**Report:** Average req/sec and total requests completed per tool. Higher is better.

---

## Dimension 3: Startup / Cold-Start Time

**Goal:** Measure how long each tool takes from process launch to the moment the first request can be sent.

**Method:**

Define "ready" as: the UI is fully loaded and the user can click "Send" (for desktop tools). Measure 5 cold starts per tool and average them. Do not count time to open a specific collection or workspace — only time to first-usable state.

- **Rocket:** `time open /Applications/Rocket.app` (macOS) or equivalent. Stop the timer when the main window is interactive (not loading spinner).
- **Postman:** `time open /Applications/Postman.app` — stop when the main UI is interactive.
- **Bruno:** `time open /Applications/Bruno.app` — stop when the main UI is interactive.

For CLI tools (newman, bru), measure time to first HTTP response:
```bash
time newman run <collection.json> -n 1
time bru run <request.bru>
```

**Report:** Average cold-start time (ms) per tool. Lower is better.

---

## Dimension 4: Memory Footprint

**Goal:** Measure RAM consumption at idle and under peak load.

**Method:**

- **Idle:** Launch each tool, wait 30 seconds for initialization to settle, then record RSS (Resident Set Size) using:
  - macOS/Linux: `ps aux | grep -i <tool-name> | awk '{sum += $6} END {print sum " KB"}'`
  - Windows: Task Manager → Details tab → Memory (working set)

- **Under load:** During an active 60-second load test at 25 concurrent users (same as Dimension 2), sample RSS every 5 seconds using the same command. Report peak RSS and average RSS during the run.

- For Electron-based tools (Postman, Bruno), sum all processes with the tool's name (Electron spawns multiple processes).
- For Rocket (Tauri), sum the main process and any WebKit/WebView helper processes.

**Report:** Idle RSS (MB) and peak RSS under load (MB) per tool. Lower is better.

---

## Dimension 5: UI Responsiveness Under Load

**Goal:** Measure whether the tool's UI remains interactive while a load test is actively running.

**Method:**

While each tool is running a 60-second load test at 25 concurrent users (same config as Dimension 2), perform this interaction every 10 seconds:
1. Click on the URL input field.
2. Type 5 characters.
3. Delete them.
4. Record whether the input registered within 500ms (pass) or felt sluggish / dropped keystrokes (fail).

Additionally, open browser DevTools (or Tauri's webview inspector) during the run and record:
- JS event loop lag: the longest "long task" (>50ms) observed in the Performance timeline during the run.
- Frame rate: average FPS reported during the run (target: ≥ 30fps).

**Quantitative scoring input:**
- Longest long task (ms) — lower is better
- Average FPS under load — higher is better
- Input latency pass rate (N/6 interactions) — higher is better

**Report:** Longest long task (ms), average FPS, input pass rate per tool.

---

## Dimension 6: Real API Accuracy (Latency Delta)

**Goal:** Measure how closely each tool's reported latency matches the true network round-trip time.

**Method:**

Run 50 requests to `https://httpbin.org/get` using each tool and record their reported p50 latency. Simultaneously, measure the true p50 latency using curl as a reference:

```bash
for i in $(seq 1 50); do
  curl -o /dev/null -s -w "%{time_total}\n" https://httpbin.org/get
done | awk '{sum+=$1; count++} END {print "curl p50 approx:", sum/count*1000, "ms"}'
```

Compute: `delta = |tool_reported_p50 - curl_p50|`

A smaller delta means the tool's timing is closer to true network latency (less overhead from the tool's own processing being included in the measurement).

**Report:** Reported p50 (ms), curl reference p50 (ms), delta (ms) per tool. Smaller delta is better.

---

## Scoring Rubric

After collecting all measurements, convert raw results to scores using relative ranking:

For each dimension, assign scores 1–10:
- Best-performing tool on that dimension: **10**
- Worst-performing tool: score = `10 × (best_value / worst_value)` for "lower is better" metrics, or `10 × (tool_value / best_value)` for "higher is better" metrics.
- Middle tool: interpolate linearly.

Minimum score per dimension: 1 (never 0).

**Dimension weights:**

| Dimension | Weight |
|---|---|
| Request execution latency (p95) | 25% |
| Load test throughput (req/sec) | 25% |
| UI responsiveness under load | 20% |
| Memory footprint (peak under load) | 15% |
| Startup / cold-start time | 10% |
| Real API latency accuracy (delta) | 5% |

**Weighted total score** = sum of (raw score × weight) for each dimension. Maximum possible: 10.

---

## Output Format

Produce a report with exactly these sections, in this order:

### 1. Environment

A table listing all environment details (OS, CPU, RAM, tool versions).

### 2. Scored Comparison Matrix

A markdown table:

| Dimension | Weight | Rocket | Postman | Bruno |
|---|---|---|---|---|
| Request latency (p95, localhost) | 25% | X/10 | X/10 | X/10 |
| Load test throughput (req/sec) | 25% | X/10 | X/10 | X/10 |
| UI responsiveness (longest task) | 20% | X/10 | X/10 | X/10 |
| Memory footprint (peak MB) | 15% | X/10 | X/10 | X/10 |
| Startup time (ms) | 10% | X/10 | X/10 | X/10 |
| Real API latency accuracy (delta) | 5% | X/10 | X/10 | X/10 |
| **Weighted Total** | 100% | **X.X** | **X.X** | **X.X** |

### 3. Per-Dimension Findings

For each of the 6 dimensions, write 2–4 sentences covering:
- What the raw numbers showed
- Which tool performed best and why you think that is
- Any surprising or noteworthy observations

### 4. Verdict

- Which tool wins overall and by how much
- Where Rocket specifically leads vs. lags
- 3–5 concrete, actionable recommendations for the Rocket team based on the findings (e.g., "Rocket's p99 latency is 2× Postman's on localhost — this suggests overhead in the Tauri IPC round-trip that could be profiled with cargo-flamegraph")

### 5. Raw Data Appendix

One table per dimension with all individual measurements, so findings can be reproduced or challenged.

---

## Constraints and Caveats

- If any tool cannot perform a specific measurement (e.g., Bruno has no native load test UI), document the limitation and use the closest equivalent method. Do not skip the dimension — adapt and note the methodology deviation.
- If httpbin.org is rate-limiting or unavailable, substitute `https://jsonplaceholder.typicode.com/posts/1` and note the substitution.
- Do not extrapolate beyond what the measurements show. If a result is ambiguous, say so.
- Version differences between tools may explain performance gaps — note the versions and flag if a newer version might change the result.
- This benchmark measures tool overhead and UX quality, not the underlying HTTP library performance. A tool that uses reqwest (Rocket) vs. axios (Postman/Bruno) will have different baselines at the HTTP layer — acknowledge this in the verdict.
```
