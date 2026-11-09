# Sustained browser-controller benchmark

Measured 2026-11-09 on Apple M4/macOS 27.2, Chrome 157.0.8103.38 and Node 25.9.0. Versions: agent-browser 0.41.2 native binary, Playwright 1.65.0 and Puppeteer 25.16.0. [Raw results](browser-controller-sustained-results.json) include all task samples, failures and recoveries.

## Results

**Agent-browser remained fastest in this longer workflow, with a required scrolling recovery.** Its median task took **3.02 seconds**, about 28% less time than Playwright and 56% less than Puppeteer. Those advantages are smaller than the earlier short-loop benchmark. Without the recovery, agent-browser failed this fixture's Save click; Playwright was the fastest tested option that completed it without an external workaround.

We completed **216 successful workflows: 72 per controller**, comprising 6,912 prescribed UI actions plus observation, verification and recovery work. All 216 passed the full form-state check and independent server persistence oracle. Each controller ran for more than two minutes; first-task warmups remain in success counts but are excluded from the 69 latency samples per controller.

| Controller | Task median | Task p95 | Completed | Explicit scroll recoveries | Total block time |
|---|---:|---:|---:|---:|---:|
| agent-browser native | 3.02 s | 3.56 s | 72/72 | 72 | 223 s |
| Playwright persistent API | 4.17 s | 4.91 s | 72/72 | 0 | 291 s |
| Puppeteer persistent API | 6.84 s | 9.43 s | 72/72 | 0 | 500 s |

No tasks timed out or remained failed in the recovered sustained run. The 72 agent-browser recoveries are real failed first click attempts, not hidden successes. Counts describe harness recovery; libraries may wait or retry internally. The separate unassisted baseline failed 24/24 agent-browser tasks.

| Controller | Block 0 median | Block 1 median | Block 2 median | Early / late task median |
|---|---:|---:|---:|---:|
| agent-browser | 3.36 s | 2.93 s | 3.01 s | 3.01 / 3.00 s |
| Playwright | 4.27 s | 4.47 s | 3.25 s | 4.07 / 4.22 s |
| Puppeteer | 7.63 s | 7.57 s | 5.61 s | 7.32 / 5.95 s |

Early/late means tasks 1–6 versus 18–23 in each block, excluding task 0 warmups. The final block narrowed the agent-browser/Playwright gap substantially. These variations are observed, not attributed to caching or host load without evidence. There was no consistent task-latency deterioration during this run; this is not an hours-long endurance test.

| Persistent controller RSS | Before block | After warmup | End of block | Range across task samples |
|---|---:|---:|---:|---:|
| agent-browser daemon | 19.6–20.1 MiB | 20.0–20.5 MiB | 13.5–18.3 MiB | 13.1–20.5 MiB |
| Playwright Node worker | 132.0–132.5 MiB | 148.5–150.1 MiB | 98.7–163.0 MiB | 91.2–170.8 MiB |
| Puppeteer Node worker | 82.9–83.0 MiB | 89.8–92.8 MiB | 86.6–133.8 MiB | 59.5–133.8 MiB |

These are RSS snapshots, not total browser memory or measured peaks. Node rows include the benchmark worker; native agent-browser excludes its separate orchestration worker and transient CLI processes. Memory varied within and between blocks, so the numbers do not establish leak rates or unique physical footprint.

Three screenshots per controller were timed outside task latency: agent-browser 89–191 ms, Playwright 54–246 ms, Puppeteer 121–284 ms. Three samples are insufficient for a screenshot-performance ranking. Captures of the reopened form are preserved: [agent-browser](assets/browser-controller-benchmark/agent-browser-form.png), [Playwright](assets/browser-controller-benchmark/playwright-form.png), [Puppeteer](assets/browser-controller-benchmark/puppeteer-form.png).

## Workload and controls

This experiment uses a local interactive work-order application, not the prior three-element projection fixture. Every task performs 32 native UI actions/navigation operations: open the application, navigate to records, create an order, fill multiple fields, choose team/priority, toggle notifications, add a custom field dynamically, replace its value, fill a budget, approve, save, filter records, reopen, edit using real keyboard typing, change dropdowns and fields, save again, navigate away, search again, reopen and verify the full record.

The source is preserved as [sustained.mjs](../../scripts/bench/browser-controllers/sustained.mjs) and [fixture.html](../../scripts/bench/browser-controllers/fixture.html). The fixture scrolls below the fold and uses required/email/numeric input constraints. List responses and editor rendering wait 180 ms, custom-field rendering 150 ms, and save responses 250 ms. These deterministic delays emulate asynchronous UI/data operations rather than relying on network noise. They total approximately 1.73 seconds per successful task; they are part of task duration and are not subtracted to manufacture a controller-only result.

The controllers use native navigation, click, fill, select and keyboard APIs. JavaScript evaluation is limited to observations; it never mutates the DOM or fixture data. The final check compares every field in the rendered form and, independently, fetches the persisted record from a server-only oracle endpoint. Persistence here means server state surviving page navigation, not durable disk storage or server restarts. The workflow is deterministic with known selectors, not an autonomous LLM benchmark. Native snapshots and token costs are not compared.

Each controller performs 24 tasks in each of three blocks, with orders AB→Playwright→Puppeteer, Playwright→Puppeteer→AB, and Puppeteer→AB→Playwright. The first task of each block warms the controller and is retained in success counts but excluded from latency percentiles. That leaves 69 measured task latencies per controller. Each block starts a fresh isolated Chrome profile/process with a 1280×713 content viewport. Dimensions were checked in screenshots for the initial two blocks and asserted in every subsequent worker. The local server stays alive and accumulates records across blocks; rotation distributes increasing list sizes, but does not eliminate this factor entirely.

Controller RSS is sampled before work, after warmup, after every task and at the end. Browser memory is excluded. Agent-browser's daemon is measured; Playwright/Puppeteer use the Node worker including harness overhead. No claim about physical unique memory, total browser footprint, idle CPU or energy follows. Screenshots are timed separately outside task latency. Application pauses and waiting/polling are included in task duration. Internal command counts are controller-specific instrumentation, not comparable logical-action counts.

## Reliability finding and bounded recovery

After correcting a harness decoding error during setup, the first agent-browser block failed 24/24 workflows with an explicit Save-button-outside-viewport error. These are real native-action failures and are preserved separately. The failure happened before the first save, after completing the initial form.

The measured sustained run adds one narrowly bounded recovery: when agent-browser returns that exact outside-viewport failure, issue native `scrollintoview` and retry that click once. Failed first attempts, scrolling, retry time and recovery counts remain inside task timing. Other errors become failed tasks; no generic retries hide failures. Playwright and Puppeteer keep their built-in scrolling/actionability behavior. Thus “eventual task success” must be distinguished from “success without recovery.”

The earlier setup error was different: agent-browser printed a JSON string containing serialized JSON, and one parse left false as a truthy string. The initial setup run was discarded, decoding fixed, and readiness now requires an actual boolean. That discarded harness failure is not counted as a controller failure.


## Reproduction and limitations

The first agent-browser/Playwright blocks were preserved when a Puppeteer import-layout problem and its default 800×600 viewport were discovered. We corrected Puppeteer to 1280×713, verified its workflow, and resumed the remaining blocks. At that restart, the fixture server was seeded with the exact 48 final records and original ID/order from the validated initial blocks; partial Puppeteer records were discarded. Later blocks then continued against the same server. Setup pilots and abandoned partial runs are excluded from the final metrics. This preserves valid work while making the restart and growing dataset explicit.

A final one-task reproduction with recovery disabled confirmed the geometry: Save was partly visible at y=705.875–747.875, but its center was y=726.875, below the 713-pixel viewport. That supports the partial-visibility explanation, rather than a claim that agent-browser never auto-scrolls. Its [versioned element code](https://github.com/vercel-labs/agent-browser/blob/v0.41.2/cli/src/native/element.rs#L1050) contains auto-scroll logic. The diagnostic result is included in the raw artifact.

Use the pinned dependencies from [the original benchmark instructions](browser-controller-local-benchmark.md#reproduction), then run:

```sh
node scripts/bench/browser-controllers/sustained.mjs
```

The default runs all nine blocks fresh. `BENCH_ROOT` sets the dependency/output directory, `CHROME_PATH` overrides Chrome, and `TASKS` overrides tasks per block. The macOS native binary and `ps` memory sampler make this a macOS harness; it has not been validated on Windows. `RESUME=1` is specific to the documented restart after the initial two 24-task blocks and seeds those 48 records. To reproduce the unassisted click failure separately:

```sh
TASKS=1 BENCH_PILOT=1 PILOT_KIND=ab AB_RECOVERY=0 RESULT_PREFIX=diagnostic node scripts/bench/browser-controllers/sustained.mjs
```

This experiment answers a realistic but prescribed CRUD workflow on one local application. It does not rank autonomous LLM agents, arbitrary public websites, all OSS controllers, Windows, or Ferrite's actual CEF embedding. Native actionability/waiting semantics differ. Browser state, rendering, scrolling and persisted values were checked, but full browser conformance was not.

For Ferrite, current native agent-browser is the strongest measured speed/footprint candidate **provided the tested scrolling fallback is included and CEF compatibility is validated**. Playwright completed the same tasks without that external recovery and remains the reliability baseline. The measurements support those concrete tradeoffs rather than an unconditional winner.
