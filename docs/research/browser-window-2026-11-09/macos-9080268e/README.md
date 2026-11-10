# Actual macOS operator Preview pass and native attachment red

Source `9080268eb3f34092a5b91b6c84001a6713126ccc`, signed macOS ARM64 package.
Run https://github.com/josephwylie/ferrite/actions/runs/42569281894
Job https://github.com/josephwylie/ferrite/actions/runs/42569281894/job/129002661820
Artifact `browser-window-macOS-ARM64`. This directory retains original
before/after PNGs and raw structured/log files; the complete artifact also
includes redundant current-frame and provider-before/after screenshots.

The actual native GPUI Window, production CockpitView, Server, owning Preview
and CEF page passed the operator scenario. The original [before](window-before.png)
and [after](window-after.png) PNGs were visually inspected: the displayed button
changes red to green. The independent [HTTP oracle](oracle.json) recorded exactly
one click. Native readback retained eleven frames and the in-window scenario
took 2,868 ms. This is not a controlled cold startup measurement. Images are
1024×674 physical pixels, not the later 1280×713 browser benchmark viewport.
[Provenance](provenance.json) retains the signed executable SHA-256 and OS.

The actual ClaudeSession capability discovery and State passed. Its readable
[Observe](claude/provider-trace.json) failed with the same native attachment
error as Windows: `CDP error (Target.createTarget): Browser command is outside the embedded target capability`.
The endpoint correctly refused new-target creation. No provider click or blue
Preview result occurred. [Peer result](claude/provider-result.json),
[advertised definitions](claude/provider-definitions.json) and
[host wire](claude/provider-host.jsonl) retain the actual boundary result.

The independent OS process-group sample was empty after each ordinary app exit;
no native controller survived. Each external public Stop request then stopped
its isolated Server, with zero ended sessions for the operator scenario and
one for the provider scenario. Neither generated cleanup-error.txt.

This confirms the operator Preview and narrow quit/cleanup behavior on macOS.
It does not establish provider control, annotations, both-provider parity,
sustained fixture correctness or resource acceptance. Together with the Windows
folder from the same commit, it proves the same actual native attachment red on
both supported platforms before the targeted pin-binding fix.
