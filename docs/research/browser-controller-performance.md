# Browser controller performance for Ferrite

Expanded research 2026-11-09. Primary-source review, not a locally executed comparative benchmark. Scope: control Ferrite's existing visible CEF pages on macOS and Windows, with low CPU/memory and fast agent interactions.

## Conclusion

**Adoption correction, 2026-11-09:** Navigera has zero GitHub stars and forks at the live API check. Its self-published speed claim was given too much recommendation weight below. Treat it as experimental, not a leading production choice. The subsequent [X discovery and adoption review](browser-controller-x-discovery.md) broadens the practical shortlist with OpenCode Browse, Tencent BrowserSkill and bb-browser, and separates social claims from measured integration results. agent-browser remains a substantially more established native-controller baseline; no Ferrite speed winner has been measured.

**There is no defensible universal “fastest” winner, and the initial shortlist was too narrow.** Navigera now has the strongest *published agent-command latency claim among the wider set inspected*: 33 ms versus agent-browser's 104 ms in its own benchmark. That makes it a serious first evaluation candidate, not a proven Ferrite winner. Rod/chromedp, Chromiumoxide, PinchTab, and newer CEF-native suitecut materially change the comparison. Rust alone does not predict speed.

For Ferrite, evaluate **Navigera and agent-browser as ready-made native agent controllers**, plus **Rod or Chromiumoxide as thin library baselines**. Include suitecut's CEF integration design in the investigation, but its current missing Windows runtime packaging prevents selecting its complete runtime for Ferrite's two-platform requirement. All finalists must control Ferrite's existing visible CEF tab rather than launch another browser.
The distinction matters: a thin CDP library supplies less functionality per call than an agent-facing controller performing element resolution, visibility checks, snapshots, and retries. Compare equal successful outcomes, not just command names.

## Concrete published numbers

### agent-browser: substantial controller-memory evidence, not a cross-project speed win

The project's March 2026 changelog publishes this comparison of its former Node.js daemon and native Rust replacement:

| Metric | Former Node daemon | Rust daemon |
|---|---:|---:|
| Cold startup | 1,002 ms | 617 ms |
| Daemon memory | 143 MB | 8 MB |

These are author-reported measurements. Daemon memory excludes Chrome itself; 8 MB is not a browser footprint or a promise for today's version. Cold startup is also not the latency of every action. [Published results in the repository changelog](https://github.com/vercel-labs/agent-browser/blob/main/CHANGELOG.md) (Full Native Rust entry; verified against the raw changelog).

Its actual benchmark harness runs commands in a Vercel Sandbox, separates browser/daemon RSS, and times navigation, screenshots, evaluation, snapshots, clicks, input, and sequences. The “agent-loop” is three scripted commands, **not an LLM-driven task**. [Harness](https://github.com/vercel-labs/agent-browser/blob/4b0e9f01fa8d214d39bbfe5391257640634fb9a0/benchmarks/bench.ts), [scenarios](https://github.com/vercel-labs/agent-browser/blob/4b0e9f01fa8d214d39bbfe5391257640634fb9a0/benchmarks/scenarios.ts)

The benchmark README explicitly says command latency is largely CDP round trips, so daemon differences are usually small. Its defaults now install the latest Rust release versus a Rust branch; reproducing the historical Node comparison requires changing the version. It also names 0.23.0 as the Rust-only boundary, whereas the changelog labels the change 0.20.0. Pin source revisions and binaries instead of trusting an unqualified release-era claim. [Benchmark methodology](https://github.com/vercel-labs/agent-browser/blob/4b0e9f01fa8d214d39bbfe5391257640634fb9a0/benchmarks/README.md)

### Raw library comparison: winners vary by operation

Ferrous-browser publishes a comparison on one Linux host, using Chrome for Testing 131.0.6778.204. Selected reported medians:

| Operation | Chromiumoxide | Ferrous-browser | Puppeteer | Playwright |
|---|---:|---:|---:|---:|
| Evaluate `document.title` | **0.18 ms** | 0.22 ms | 0.45 ms | 0.79 ms |
| Navigate to `about:blank` | **4.3 ms** | 6.2 ms | 5.1 ms | 4.7 ms |
| PNG screenshot | 38 ms | **37 ms** | 41 ms | 50 ms |
| Launch browser | 134 ms | 357 ms | 162 ms | **93 ms** |

This is a competing project's self-published benchmark, not independent validation. No agent-browser or Chrome DevTools MCP result is included. The tested engine, OS, headless configuration, and launch workflow differ from Ferrite. The results support only narrow claims about those operations in that setup. [Results and declared methodology](https://github.com/theoxfaber/ferrous-browser/blob/99b76295f2dbc91cdd90077434c2dda84972f6c1/README.md#benchmarks)

Source audit found reasons not to accept the table as a blanket ranking:

- Node harnesses time page creation **and closure** together; the Chromiumoxide harness excludes closure. Do not compare their published new-page row.
- The Chromiumoxide selector-wait test imposes a user-written 50 ms polling interval. That delay is not an inherent CDP or Rust limit.
- Screenshot dimensions are not explicitly normalized across the inspected harnesses; screenshots are blank-page captures, not complex development previews.
- Browser launch excludes starting the host Node/Rust program, and explicit flags disable GPU and sandbox. This is not Ferrite startup or its intended graphics configuration.

[Playwright harness](https://github.com/theoxfaber/ferrous-browser/blob/99b76295f2dbc91cdd90077434c2dda84972f6c1/bench/playwright/bench.js), [Puppeteer harness](https://github.com/theoxfaber/ferrous-browser/blob/99b76295f2dbc91cdd90077434c2dda84972f6c1/bench/puppeteer/bench.js), [Chromiumoxide harness](https://github.com/theoxfaber/ferrous-browser/blob/99b76295f2dbc91cdd90077434c2dda84972f6c1/bench/chromiumoxide/src/bench.rs)

## Wider survey: controllers are not interchangeable with agent frameworks or engines

“Attach” below means the project documents connecting to an existing browser; it is **not verified Ferrite CEF compatibility**. Cross-platform client code also does not establish tested Windows/macOS CEF integration. No common apples-to-apples benchmark covers this whole field.

### Direct controllers and agent-facing control surfaces

| Project | Runtime / existing-browser path | Performance evidence and Ferrite fit |
|---|---|---|
| [Navigera](https://docs.rs/crate/navigera/0.10.0) | Rust library, CLI and MCP; CDP browser control | Published 33 ms agent-command result deserves evaluation; benchmark conditions below. Newer than the established automation libraries; validate both OS builds and CEF targets. |
| [keon/browser-control](https://github.com/keon/browser-control) | Rust CLI, element refs, event watcher and explicit CDP endpoint | Another relevant native agent controller; no comparable primary latency results found. Validate Windows packaging and CEF attachment rather than infer them from Rust. |
| [Playwriter](https://github.com/remorses/playwriter) | Node CLI/MCP, Playwright API through a Chrome extension | Reuses running browser tabs and logins. Its extension and Chrome tab-group workflow require adaptation to CEF; no comparable speed winner evidence found. |
| [agent-browser](https://github.com/vercel-labs/agent-browser) | Native Rust daemon; CLI/MCP; [`--cdp`](https://agent-browser.dev/cdp-mode) attach | Ready-made snapshots and element references. Historical 8 MB daemon RSS is useful evidence, not total browser memory. Current direct competitor results exist; it cannot be called fastest by default. |
| [PinchTab](https://github.com/pinchtab/pinchtab) | Go binary; HTTP control plane, CLI/MCP; attach/bridge modes | Published agent-token savings, not proof of lowest action latency. macOS/Linux primary tested platforms; Windows is explicitly limited/best-effort. Existing daemon/profile lifecycle needs adaptation. |
| [Chromiumoxide](https://github.com/mattsse/chromiumoxide) | Async Rust CDP library; connect to existing WebSocket | Low raw overhead in one published harness; Ferrite supplies agent snapshots, reference lifecycle and waits. A baseline, not a complete ready-made agent tool. |
| [Ferrous-browser](https://github.com/theoxfaber/ferrous-browser) | Rust CDP library | Useful reproducible harness, with mismatches described above. Its own table does not win every operation or establish CEF integration. |
| [headless_chrome](https://github.com/rust-headless-chrome/rust-headless-chrome) | Synchronous Rust CDP library; [`Browser::connect`](https://docs.rs/headless_chrome/latest/headless_chrome/browser/struct.Browser.html) | macOS/Windows/Linux browser support; useful embedded Rust candidate. No comparable winner evidence found; API feature coverage differs from Puppeteer. |
| [chromedp](https://github.com/chromedp/chromedp) | Go direct CDP, no external driver; remote allocator | Serious low-overhead native controller. Navigera reports roughly 8 MB driver RSS for it. Ferrite would still need agent tooling; Go helper is an extra process rather than a Rust dependency. |
| [Rod](https://github.com/go-rod/rod) | Go CDP library; [`ControlURL`](https://pkg.go.dev/github.com/go-rod/rod#Browser.ControlURL) | Automatic waiting, concurrent control and frame/shadow support. Navigera's raw evaluation result puts Rod slightly ahead of Navigera; language-based rankings are unjustified. |
| [Puppeteer](https://pptr.dev/api/puppeteer.connect) | Node library; existing-browser WebSocket attach | Mature actions/targets API; ferrous benchmark above provides narrow timings. Node RSS is not necessarily large enough to dominate browser RSS. |
| [Playwright library](https://playwright.dev/docs/api/class-browsertype#browser-type-connect-over-cdp) | Node core with multiple language clients; CDP attach | Mature waits and locators; docs explicitly describe CDP attach as lower fidelity than its own protocol. Must test CEF; benchmark semantics matter more than API spelling. |
| [Playwright CLI](https://github.com/microsoft/playwright-cli) | Node CLI plus agent skills; `attach --cdp` | Distinct agent interface worth comparing: files/skills avoid automatically feeding full snapshots and schemas into model context. Token-efficiency claim is not a latency benchmark. |
| [Playwright MCP](https://github.com/microsoft/playwright-mcp) | Node persistent MCP server; `--cdp-endpoint` | Structured accessibility tools, mature ecosystem. Persistent transport can avoid per-command CLI launch; no universal comparison proves it slowest. |
| [Chrome DevTools MCP](https://github.com/ChromeDevTools/chrome-devtools-mcp) | Node/Puppeteer MCP server; existing-browser options | Strong diagnostics/performance tools; supports Chrome/Chrome for Testing, not guaranteed CEF. Raw controller and model tool-selection costs must be separated. |
| [browser-use Browser Harness](https://github.com/browser-use/browser-harness) | Python direct CDP harness, CLI/skills/MCP | Lets an existing coding agent drive a browser; does not inherently require a second autonomous model loop. Chrome-oriented connection and helper workflow require Ferrite adaptation. |

Native Go and Rust libraries can be built for Windows/macOS; that is a plausible integration route, **not a claim that their authors certify Ferrite's CEF embedding**. The same distinction applies to Node/Python controllers. PinchTab's documented Windows qualification is a concrete exception to treating all platforms equally.

### Higher-level AI automation

| Project | What it adds | Why its headline speed is not the same metric |
|---|---|---|
| [browser-use library / cloud](https://github.com/browser-use/browser-use) | Python autonomous Agent loop and hosted services, alongside direct-control products | Full task success depends on model choice, observation and reasoning. Its [`cdp_url` example](https://github.com/browser-use/browser-use/blob/main/examples/browser/using_cdp.py) demonstrates an existing-browser route. Compare its direct control separately from its autonomous Agent. |
| [Stagehand v3](https://github.com/browserbase/stagehand/blob/main/packages/docs/v3/references/stagehand.mdx) | Direct CDP plus AI `act`, `extract`, `observe`, reusable actions | Official v3 claim of 44.11% faster iframe/shadow interaction is against its own v2, not other controllers. Documented `cdpUrl` exists; model calls and caching affect task duration. [Official release article](https://dev.browserbase.com/blog/stagehand-v3) |
| [Stagehand v4](https://www.browserbase.com/stagehand) | Current architecture moves execution into a browser extension; TypeScript/Python/Go SDKs | Official roughly 2× claim concerns a 50-action crawl, not a common Ferrite benchmark. [`localBrowser.connect`](https://github.com/browserbase/stagehand/blob/main/packages/docs/v4/configuration/browser.mdx) uses/discovers or loads its extension, with shared-filesystem constraints. CEF extension compatibility is an additional requirement; v3 and v4 cannot be conflated. |

### Integrated browsers and replacement engines

| Project | Classification | Ferrite implication |
|---|---|---|
| [suitecut](https://github.com/skyaara/suitecut) | CEF browser runtime plus SDK/control system | Especially relevant architecture, because it already uses CEF. Published speed results below compare different browser runtimes. Current README says Windows runtime packaging is not implemented; separate C++ SDK is not a drop-in Ferrite adapter. |
| [BrowserOS / neo](https://github.com/browseros-ai/BrowserOS) | Complete Chromium-fork browser with agent facilities, MCP and native components | Windows/macOS product, but replacing Ferrite's embedded CEF with a separate full browser is a different decision. Its AGPL license also requires deliberate integration review. Not evidence of a faster attachable controller. |
| [Lightpanda](https://github.com/lightpanda-io/browser) | New Zig **headless engine**, not Rust and not a controller for CEF | Its author benchmark reports roughly 9× crawl speed and 16× lower memory than Chrome. That changes the engine and browser capabilities; it does not supply Ferrite's visible rendered preview. Native macOS/Linux; Windows through WSL2. [Benchmark](https://github.com/lightpanda-io/demo/blob/main/BENCHMARKS.md#crawler-benchmark) |

## Additional performance claims audited

### Navigera: important broader evidence, still a vendor benchmark

Version 0.10.0 reports about **33 ms per agent command versus 104 ms for agent-browser**, and roughly **7 MB driver memory versus chromedp's 8 MB**. The same project's low-level comparison reports **Rod evaluation around 0.44 ms versus Navigera 0.46 ms**. These are different tiers of measurement: no inference that a 0.44 ms library call replaces a full 33 ms agent command is valid. [Versioned results](https://docs.rs/crate/navigera/0.10.0)

The audited ladder uses deterministic local fixtures (500 cards / 1,000 rows), subprocesses for CLI tools and persistent stdio for MCP, and forces standard Chrome for Navigera and agent-browser. It includes extracted-data correctness gates and publishes run identifiers/artifacts. However, snapshot payloads and action semantics differ: agent-browser uses its full default snapshot rather than interactive-only `-i`; tests disable sandboxing. Consequently this is useful evidence of complete CLI workflow cost in that harness, not a fair isolation of CDP transport or proof of Ferrite CEF performance. Pin versions: the documentation website and current crate README have different generations of results. [Pinned methodology and harness](https://github.com/andrey-usa/navigera/tree/19432ed411018b9b62cd8d26560193700050fde6/bench/ladder)

### suitecut: an actual CEF comparison with engine and platform confounders

Its November 4 M4 benchmark reports aggregate tasks around **1.1 s for suitecut versus 1.4 s for agent-browser**, but snapshot latency around **79 ms versus agent-browser's faster 61 ms**. Summed process RSS is approximately **2.816 GB suitecut, 3.633 GB agent-browser, and 2.587 GB browser-use**: even its own results do not establish one universal winner. [Published report](https://suitecut.aakashreddy.com/benchmarks/)

The harness compares CEF 156 against Chrome 156, uses cold sessions, three rounds and live public websites, and does not run an LLM. Summing process RSS can count shared pages repeatedly. Thus its total memory is not controller-only memory, its task time mixes browser/runtime behavior with control, and its three-round internet experiment is not a robust p95 comparison. Relevant design evidence, but not permission to replace Ferrite's engine or ignore its missing Windows runtime package. [Benchmark source](https://github.com/skyaara/suitecut/tree/main/benchmarks), [platform status](https://github.com/skyaara/suitecut)

### PinchTab: model cost, not controller latency

Its April 20 benchmark uses local fixtures and an LLM runner, recording actual provider token usage. A 24-step Sonnet 4.6 task reports approximately **20.3% lower model cost and 25.3% fewer tokens than agent-browser**, with only two repetitions; Haiku task sets use three or five. Combined actions/snapshots and observation design change the agent loop. Useful evidence for agent operating cost, but it does not establish faster CDP actions, lower browser memory, or robust end-to-end latency. [Methodology and raw-run links](https://github.com/pinchtab/pinchtab/blob/main/docs/deep-dive/benchmark.md)

## Embedded-browser constraints

CEF exposes DevTools command and observation APIs, but that does not prove complete compatibility with a controller expecting Chrome's browser-wide discovery and target APIs. [CEF host interface](https://github.com/chromiumembedded/cef/blob/master/include/cef_browser.h)

Ferrite's current engine is sealed off-screen visuals, not a general browser; the existing spike measured rendering/input and footprint, not controller latency. No controller attach bridge was found in the local CEF adapter search. A fair CEF comparison first needs that integration. [Existing ADR](../adr/0013-inline-web-visuals.md), [CEF adapter](../../crates/ferrite-pane/src/visual/web/cef.rs)

The proposed controller must attach to the very tab shown in GPUI, preserve conversation tab ownership, share login state as agreed, and never launch a second browser. Default agent-browser named sessions each have their own daemon/browser, so its default lifecycle cannot simply become Ferrite's lifecycle. [agent-browser lifecycle](https://github.com/vercel-labs/agent-browser#how-it-works)

## What would settle “fastest”

Run Navigera and agent-browser against the same CEF build and local fixture on both macOS and Windows, with Rod or Chromiumoxide as a thin-library baseline. Include PinchTab only after validating its Windows path, and suitecut's SDK only if it can reuse Ferrite's runtime. Match viewport, screenshot format, loaded page, actionability checks, snapshot contents and completion condition. Measure warm action p50/p95, a full inspect/click/type/verify sequence, cold attach time, controller RSS/idle CPU, and whole-process-tree memory separately. Record failures and retries. Then measure model-facing token volume and complete agent task duration separately from controller latency.

**Decision now: Navigera is a credible speed challenger; agent-browser is a credible ready-made native option; neither is proven fastest inside Ferrite.** Thin Go libraries belong in the comparison alongside Rust. Avoid selecting a headless replacement engine, autonomous AI framework, or entire browser based on a benchmark that answers a different question. The existing evidence is sufficient to form a broader evaluation set, not to manufacture a single winner.
