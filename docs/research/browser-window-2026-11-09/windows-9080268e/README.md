# Actual Windows operator Preview pass and controller attachment red

Source `9080268eb3f34092a5b91b6c84001a6713126ccc`, Windows x64 hosted runner.
Run https://github.com/josephwylie/ferrite/actions/runs/42569281894
Job https://github.com/josephwylie/ferrite/actions/runs/42569281894/job/129002662101
Artifact `browser-window-Windows-X64`, ID `13652444741`.
This directory retains the relevant original PNGs and raw structured/log
files; the complete artifact additionally includes redundant current-frame
and provider-before/after screenshots.

The staged application's actual native GPUI Window, production CockpitView,
Server, owning Preview and CEF page passed the operator scenario. The original
[before](window-before.png) and [after](window-after.png) PNGs were visually
inspected: the displayed fixture button changes red to green. The independent
[HTTP oracle](oracle.json) recorded exactly one click. Native window readback
retained ten frames; the in-window scenario took 3,678 ms (not a controlled cold
startup measurement). Images are 1028×780 physical pixels at scale 1; the
browser viewport is not the later 1280×713 benchmark viewport.

Both staged CEF bootstrap and naked Rust executable now reserve 8,388,608 bytes
and commit 4,096 bytes, verified from the actual [PE headers](pe-headers.json).
The failure-only naked startup comparison reached its native Window and stayed
alive after settling. This verifies the bounded larger-stack fix against the
prior 1 MiB main-thread overflow; it does not establish a minimum reserve.

The subsequent actual ClaudeSession capability discovery and browser State
passed. [Observe](claude/provider-trace.json) failed with:
`CDP error (Target.createTarget): Browser command is outside the embedded target capability`.
The native controller attempted an operation the embedded endpoint correctly
refused. No provider click or blue Preview result occurred. The peer received
actual advertised browser, plan and wrap-up definitions before calling State;
its unique probe marker and PID tie the response to the isolated CLI process.
[Peer result](claude/provider-result.json), [definitions](claude/provider-definitions.json)
and [host wire](claude/provider-host.jsonl) retain this new attachment red.

Both applications exited with no native controller in the independent OS
survivor samples. The external public Stop request then stopped each isolated
Server; server-process.log records normal shutdown with zero sessions for the
operator scenario and one ended session for the provider scenario. Neither
produced cleanup-error.txt. These are narrow quit/cleanup checks, not full
resource or lifecycle acceptance.

This is a Windows operator Preview pass and an actual provider attachment
failure. It does not establish provider control, screenshots from that control,
annotations, both-provider parity, sustained fixture correctness or resources.
The macOS job from this run was still in progress when this evidence was saved.
