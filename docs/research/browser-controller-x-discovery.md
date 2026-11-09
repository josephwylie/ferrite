# Browser controllers: X discovery and credibility check

Research date: 2026-11-09. Supplements [the performance survey](browser-controller-performance.md). This pass prioritizes X discovery, then verifies architecture against the actual project source. It does not establish a new speed winner.

## What changed

**OpenCode Browse, Tencent BrowserSkill, Playwriter and Vibium were meaningful omissions.** Browse is particularly relevant to Ferrite's shared-browser interaction design. Navigera's benchmark can be informative, but promoting it above established projects without checking adoption was premature: its repository currently has zero stars. Stars measure attention rather than correctness, but a zero-adoption project requires substantially more validation before becoming a product dependency.

## X evidence and access limitations

The user requested Scrape Creators for X research. The parent used the configured API credential without exposing it, completing ten searches and three direct-post fetches. I inspected the resulting JSON in `/tmp/ferrite-browser-x-research/` (ephemeral local evidence, not a committed dependency). This corroborated the official Browse launch, Hung Vinh comparison, Stagehand v4 launch and Tencent BrowserSkill announcement. The initial discovery also included direct reading in X before the user's change of method. Ordinary web fetches returned 403; Grok trend summaries and third-party mirrors are **not technical evidence**. Search results are neither exhaustive nor always relevant: the Vibium query returned no posts, and “Playwriter” also returned Playwright results.
| Direct X source | Observation | Evidential weight |
|---|---|---|
| [OpenCode Browse launch](https://x.com/opencode/status/2118895128152621153) | Official November 7 launch; parent observed about 5,389 likes and 307,097 views | Strong discovery signal, not a benchmark. Architecture verified below. |
| [thdxr beta announcement](https://x.com/thdxr/status/2118878460391137310) | Any-agent browser access | First-party product announcement. |
| [thdxr remote-machine detail](https://x.com/thdxr/status/2118973784162238748) | Agent and browser can run on different machines | Useful architectural lead; not local performance evidence. |
| [Browse user comparison](https://x.com/hungv47/status/2119042998680043665) | Reports Browse 40% faster, 81% less **uncached** input and 33% fewer steps than agent-browser with GPT-6 Astra medium | Anecdote without inspected controlled harness, repetition count or equal-output checks. Measures agent workflow, not bare controller speed. |
| [Dan Kornas on fastbrowse](https://x.com/DanKornas/status/2118537846797779218) | November 6 author post; parent observed 34 likes | Discovery lead. Canonical repository and benchmark not yet verified; guessed repository URL returned 404. Do not recommend on this evidence alone. |
| [Obscura post](https://x.com/itsharmanjot/status/2095876108067508231) | September 4 post; parent observed about 2,548 likes | Discovery signal for a different engine. Current source now includes rendering, unlike older descriptions. |

The parent also inspected the attached comparison image: agent-browser versus Browse shows 15 versus 10 steps, uncached input 9,467 versus 1,764 tokens, cached input 391,936 versus 404,352, and total processed tokens 408,641 versus 412,817. Thus the “81% less input” headline applies only to **uncached** input; overall processed tokens were slightly higher for Browse. The image has no timing row, so the 40% speed claim remains an unverified statement in the post.

Additional Scrape Creators evidence:

- [Tencent's BrowserSkill announcement](https://x.com/TencentAI_News/status/2100143086429217278) describes local tab borrowing, existing logins and human handoff. The launch described CLI-only use, but current repository docs now include MCP; use current docs for integration decisions.
- [Stagehand v4 launch](https://x.com/Stagehanddev/status/2086849338089857082) confirms improved context management, self-healing actions and iframe support; it does not provide a speed benchmark.
- [Browser-use CLI versus Playwright CLI](https://x.com/s_streichsbier/status/2036065004043292763) reports **best runs** of 83.3 s and 102.4 s on an agent-driven dashboard task. Best-run anecdotes are neither median performance nor controlled Ferrite evidence.
- [PinchTab mention](https://x.com/heynavtoor/status/2028922003365986705) was recovered through Scrape Creators and describes its small Go binary and HTTP interface. Its promotional language is not measured performance evidence.

## New credible alternatives

### OpenCode Browse

The actual project is **[anomalyco/browse](https://github.com/anomalyco/browse)**, not OpenCode's main repository. Its official [product page](https://opencode.ai/product/browse) describes an MCP server and Chromium extension that use Playwright to control existing tabs, preserve logins, open background tab groups, and hand control back to the user for login/2FA/payment. It works with any MCP client. Its installer supports Windows; npm installation requires Node 22.19+, while packaged binaries are also offered.

This is a strong reference for Ferrite's shared-user/agent tab model. However, a Chromium extension plus relay cannot be presumed to work inside Ferrite's off-screen CEF embedding: extension/debugger APIs, tab groups and handoff UI need adaptation. The X speed claim is promising but insufficient to establish lowest local CPU/RSS or control latency. Its newly launched standalone repository had **338 stars** at API lookup; do not attribute OpenCode's much larger repository count to Browse.

### Tencent BrowserSkill

[BrowserSkill](https://github.com/Tencent/BrowserSkill) is a particularly relevant addition: Rust CLI/daemon plus Chromium extension, locally controlled by an existing shell-capable agent, with current MCP integration as well. Official support includes macOS Apple Silicon/Intel and Windows x64. Its shared-control workflow synchronizes observations and yields to manual input, with explicit existing-tab borrowing and human handoff. This deserves comparison for Ferrite's collaboration requirements, not merely its adoption.

The extension supports Chrome/Edge 125+; CEF embedding remains unverified. No normalized latency/RSS benchmark found here establishes it as fastest. Native Rust daemon plus corporate maintainer does not remove the extension/API compatibility requirement. [Source and platform table](https://github.com/Tencent/BrowserSkill)

### Playwriter

[Playwriter](https://github.com/remorses/playwriter) provides an existing-Chrome extension, CLI/MCP and persistent JavaScript sessions using Playwright. Sessions isolate code state while browser tabs are shared; tabs can be grouped per agent. This closely matches the requested collaboration shape. Its README also exposes a CDP relay, but connecting Playwright to that relay is not proof that its extension-based backend can attach directly to Ferrite's CEF. [Current releases](https://github.com/remorses/playwriter/releases) document lifecycle and multi-session work, which are relevant implementation evidence beyond a demo.

The README's “100× more efficient” video claim does not establish 100× faster browser control. No normalized Ferrite benchmark was found. Evaluate its session and observation design; do not adopt its headline as a performance result.

### Vibium

[Vibium](https://github.com/VibiumDev/vibium) is a Go binary with CLI/MCP and JavaScript, Python and Java clients. It advertises a roughly 10 MB binary with no runtime dependencies and agent actions such as element mapping, references, screenshots, waits and form input. **Binary size is not resident memory.** Its [protocol explanation](https://github.com/VibiumDev/vibium/blob/main/docs/explanation/webdriver-bidi.md) identifies WebDriver BiDi, with Chrome support through ChromeDriver. This is a serious lightweight contender, but adds a protocol/driver compatibility question for CEF. It is not just another CDP client that can be assumed to attach unchanged.

### bb-browser

The [author's X explanation](https://x.com/yan5xu/status/2033230604423352821), fetched through Scrape Creators, links directly to [epiral/bb-browser](https://github.com/epiral/bb-browser). This Node CLI/MCP uses a daemon and CDP, with browser actions plus site-specific JavaScript adapters that return structured data. It can reuse OpenClaw's channel instead of its usual bridge. Its current README lists 103 commands across 36 platforms; this is an adapter catalog, not universal website coverage.

This is another credible omission, with **6,259 stars** at API lookup. Its interesting optimization is avoiding repeated visual navigation by calling site functionality inside the existing page. Such workflow shortcuts may save far more time than small CDP-call differences, but depend on site-specific integrations and do not replace Ferrite's general preview/annotation controls. No equal-task CEF performance evidence was found. [Repository API](https://api.github.com/repos/epiral/bb-browser)

### Skyvern and Steel

[Skyvern](https://github.com/Skyvern-AI/skyvern) is an established Python AI workflow system, with vision/model reasoning and Playwright-compatible APIs. Its success and task-duration figures include that agent layer. It is broader than the control tool Ferrite needs.

[Steel](https://github.com/steel-dev/steel-browser) manages browser processes and sessions through Puppeteer/CDP, with API/UI, extensions and cloud/local deployment. It is credible infrastructure, but its normal deployment owns a browser environment; it is not evidence of the lightest helper for Ferrite's existing visible CEF surface.

### Obscura: do not repeat stale “no rendering” claims

The current [official repository](https://github.com/h4ckf0r0day/obscura) advertises native screenshots, screencasts and PDF output. It is a Rust headless engine using V8 and CDP, with macOS and Windows release instructions. Thus it is inaccurate to dismiss today's Obscura as incapable of rendering. However, screenshots/screencasts do not prove complete desktop-browser compatibility, input behavior, accessibility or a GPUI embedding API. Its engine benchmarks answer a different question than fastest control of Ferrite's current CEF. Evaluate browser conformance and interactive rendering separately before reconsidering the engine decision.

## Adoption snapshot

Read directly from GitHub's public REST repository API on 2026-11-09 (`stargazers_count`). Counts move continually, and do not establish production deployments, speed, security, or compatibility. Each link identifies the exact counted repository.

| Project | Stars |
|---|---:|
| [browser-use](https://api.github.com/repos/browser-use/browser-use) | 120,343 |
| [agent-browser](https://api.github.com/repos/vercel-labs/agent-browser) | 47,538 |
| [Obscura](https://api.github.com/repos/h4ckf0r0day/obscura) | 30,575 |
| [Stagehand](https://api.github.com/repos/browserbase/stagehand) | 25,985 |
| [Skyvern](https://api.github.com/repos/Skyvern-AI/skyvern) | 23,366 |
| [BrowserOS](https://api.github.com/repos/browseros-ai/BrowserOS) | 14,284 |
| [PinchTab](https://api.github.com/repos/pinchtab/pinchtab) | 10,563 |
| [Tencent BrowserSkill](https://api.github.com/repos/Tencent/BrowserSkill) | 10,188 |
| [Steel](https://api.github.com/repos/steel-dev/steel-browser) | 7,940 |
| [bb-browser](https://api.github.com/repos/epiral/bb-browser) | 6,259 |
| [Playwriter](https://api.github.com/repos/remorses/playwriter) | 4,064 |
| [Vibium](https://api.github.com/repos/VibiumDev/vibium) | 2,957 |
| [OpenCode Browse](https://api.github.com/repos/anomalyco/browse) | 338 |
| [Navigera](https://api.github.com/repos/andrey-usa/navigera) | 0 |

## Decision implication

Keep **agent-browser as a credible native baseline**, and add **OpenCode Browse, Tencent BrowserSkill and Playwriter** for shared-browser/session design, **Vibium** for a small native controller with a different protocol, and **Stagehand** for observation/batching efficiency. PinchTab remains interesting subject to its Windows limitations. Navigera belongs in an experimental benchmark lane, not at the top of a product recommendation solely because its own benchmark looks fast. None of the X posts settles a Windows-and-macOS Ferrite CEF ranking.
