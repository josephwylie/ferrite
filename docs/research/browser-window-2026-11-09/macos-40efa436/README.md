# Actual macOS provider action pass

Source `40efa43638f78390ceaa8e100cbef94b973e7cae`, signed macOS ARM64 package.
Run https://github.com/josephwylie/ferrite/actions/runs/42571010383
Job https://github.com/josephwylie/ferrite/actions/runs/42571010383/job/129008123949
Artifact `browser-window-macOS-ARM64`, ID `13653791251`.
This folder retains original essential PNGs and raw structured/log files.

The normal operator Window/Preview/pointer scenario passes. The subsequent
actual ClaudeSession performs tools/list → State → Observe → Click → Observe
through the bound Thread browser service, actual Server/window protocol and
production native worker. [Trace](claude/provider-trace.json) records the opaque
reference and receipt returned by readable Observe and used for the click;
neither was manufactured by the driver. Native controller attachment now pins
Ferrite's existing target without attempting new-target creation.

The [actual native Window PNG](claude/window-provider.png) was visually inspected:
the owning live Preview shows the button blue after the provider action.
The independent [HTTP oracle](claude/oracle.json) records exactly two page clicks,
operator then provider. [Window result](claude/window-result.json) reports success
and 7,083 ms in the scenario. A tool acknowledgement alone could not pass it.
The complete native Window uses 1024×674 physical pixels, not the later fixed
1280×713 browser benchmark viewport.

The independent Unix process-group sample is empty after ordinary app quit.
The external public Stop request cleanly ends the isolated Server and its one
provider session. This proves this narrow action/Preview/quit slice on macOS;
it does not establish full forms, screenshots from the browser tool, both-provider
parity, annotations, arbitration, sustained resources or reclamation. Raw native
lifecycle fields in the tool answer are not OS process/resource measurements.

Windows from the same source passes its normal operator probe but the provider
launch never reaches its peer: its initial operator gesture fails to turn the
page green. See ../windows-40efa436 for that separate readiness failure. Do not
report this macOS action pass as cross-platform provider acceptance.
