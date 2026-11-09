# Local browser-controller measurements

Measured 2026-11-09 on Apple M4, macOS 27.2, Node 25.9.0, Chrome 157.0.8103.38. This is an executed experiment, not vendor marketing. Raw per-iteration data is in [the results JSON](browser-controller-local-benchmark-results.json); the [harness](../../scripts/bench/browser-controllers/bench.mjs) and pinned npm lockfile are preserved.

## Result

Follow-up: the [sustained task benchmark](browser-controller-sustained-benchmark.md) exercises 72 complete form/create/edit workflows per controller. It preserves agent-browser's lead on that workload, but finds a smaller practical speed gap and a Save-button viewport failure requiring explicit scroll recovery. Use that report alongside this microbenchmark when evaluating Ferrite integration.

**Agent-browser 0.41.2, invoked as its native binary, was the fastest tested option for this successful observe–fill–click–verify sequence.** Separate native commands and native batch were effectively tied at about **44 ms median**, versus **97 ms for persistent Playwright** and **229 ms for persistent Puppeteer**. This is roughly 2.2× and 5.2× faster respectively on this fixture, not a universal claim about all workloads.

The installed 0.35.2 was slower. Critically, the latest npm wrapper was also much slower than calling its packaged native binary directly. A Ferrite integration should not accidentally add a Node launcher to every command.

| Tested path | Combined median, ms | Combined p95, ms | Forward / reverse median, ms | Resident controller memory, MiB |
|---|---:|---:|---:|---:|
| agent-browser 0.41.2 native, separate commands | 44.5 | 75.0 | 30.1 / 45.3 | 19.8–20.0 |
| agent-browser 0.41.2 native, one batch | 44.1 | 73.8 | 33.9 / 45.3 | 20.0–20.0 |
| agent-browser 0.35.2 installed native | 68.9 | 97.4 | 71.2 / 66.5 | 11.2–11.3 |
| Playwright 1.65.0, persistent Node API | 96.9 | 134.9 | 88.0 / 115.5 | 151.9–155.8 |
| agent-browser 0.41.2 through npm JS wrapper | 196.3 | 303.3 | 184.9 / 248.3 | 19.8–21.1* |
| Puppeteer 25.16.0, persistent Node API | 229.2 | 291.6 | 221.7 / 237.9 | 93.3–93.5 |

Every row has **60 measured successful repetitions and zero failed iterations**, following five discarded warmups per pass. Percentiles use the nearest-rank definition. Results combine two passes, one forward and one reverse order. The different pass medians show system variability; do not interpret the 0.4 ms native-versus-batch difference as meaningful.

Memory is a post-workload `ps` RSS sample, not peak memory or physical unique memory. Agent-browser figures are its persistent daemon only; transient CLI processes are excluded. Playwright/Puppeteer figures include the Node benchmark worker and imported controller library. The AB orchestration worker used roughly 60–70 MiB and is excluded because it is the harness rather than the controller. Therefore these figures describe deployed persistent processes but are **not a rigorously baseline-subtracted library allocation comparison**. *The npm-wrapper row additionally launches transient Node processes; their memory is not included in its daemon figure. Chrome memory is excluded from every row. No idle-CPU or energy claim is made.

## What was held equal

All controllers attached to the same separately launched Chrome process within each pass, using an isolated temporary profile, the same single page and a local HTTP fixture. The real user profile was never used. Chrome ran headless with a 1280×800 window, normal GPU/sandbox defaults, and no network-dependent page content. Each pass restarted the isolated browser. The fixture includes 200 noninteractive cards, a labeled input, Save button and status output.

Each measured iteration:

1. Reads the same JavaScript-generated JSON projection of the input, button and status, checking that expected content is present.
2. Fills the input with a unique value using the controller's native fill API.
3. Clicks Save using the controller's native click API.
4. Reads and verifies the exact expected saved value.

Reset is outside the timed region. The batch path performs those same four commands in one CLI batch and checks both the initial observation and final saved value. It represents a task whose steps are known in advance; it does not imply that an agent can batch decisions that depend on observations it has not read.

Playwright and Puppeteer use persistent connections and locators; they do not pay Node launch cost per action. Agent-browser pays native CLI launch/IPC costs per command or batch, while retaining its daemon. Each benchmark daemon was terminated before the next contender. This is a comparison of practical invocation paths, not an isolated CDP transport benchmark.

## Limits that matter for Ferrite

The observation is a **small, normalized DOM projection**, not a general accessibility snapshot, screenshot or full arbitrary-page understanding. It deliberately avoids comparing unequal native snapshot outputs. No LLM ran, and no claims about token costs or autonomous task success follow. Native actionability and stability checks differ by library even though all returned correct results on this static fixture. Rich SPAs, delayed rendering, iframes, shadow DOM, screenshots, retries and navigation can change the ranking.

This measures macOS Chrome, **not Windows and not Ferrite's embedded CEF**. Ferrite currently has no general CDP attach bridge in its CEF adapter, so an honest same-CEF comparison was unavailable. BrowserSkill, Browse, Playwriter, PinchTab, Navigera and other candidates were not measured here. Thus the justified answer is “fastest of these measured paths,” not “fastest OSS browser controller in existence.”

The practical recommendation from this measurement is **current native agent-browser as the first Ferrite integration candidate**, with persistent direct APIs or batching where useful. Verify the same visible CEF target and both platforms before committing to production. The benchmark gives a concrete basis for choosing that candidate; it does not eliminate the integration gate.

## Reproduction

The preserved harness uses macOS `ps` for memory and defaults to the installed system Chrome location; it is not advertised as a portable Windows benchmark. `CHROME_PATH`, `BENCH_ROOT`, `AB_NATIVE`, `CDP_PORT` and `FIXTURE_PORT` can override relevant paths. The default ports must be free. `AB_INSTALLED` names the older installed native binary used as a historical baseline.

```sh
mkdir -p /tmp/ferrite-controller-bench
cp scripts/bench/browser-controllers/package*.json /tmp/ferrite-controller-bench/
npm ci --prefix /tmp/ferrite-controller-bench
cp scripts/bench/browser-controllers/bench.mjs /tmp/ferrite-controller-bench/
AB_INSTALLED="$(command -v agent-browser)" BENCH_PASS=forward node /tmp/ferrite-controller-bench/bench.mjs
AB_INSTALLED="$(command -v agent-browser)" BENCH_PASS=reverse node /tmp/ferrite-controller-bench/bench.mjs
```

Record the installed baseline version on reruns: it will no longer represent 0.35.2 if the global package has been upgraded. The npm lock pins agent-browser 0.41.2, Playwright 1.65.0 and Puppeteer 25.16.0. Raw current experiment data retains all samples and errors but omits process command lines and user-specific session paths.
