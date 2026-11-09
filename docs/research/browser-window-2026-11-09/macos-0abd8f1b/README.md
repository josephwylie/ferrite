# First actual packaged window evidence

Source `0abd8f1b158d652ade3c3dfd0608b5deb4a89d50`, macOS ARM64 hosted
runner, [run 42564574885](https://github.com/josephwylie/ferrite/actions/runs/42564574885),
job `128988463458`, artifact `13651397350`. The complete artifact retains logs;
this directory retains 116 KiB of original PNGs and structured results.

The ordinary production native GPUI window and actual Server/Preview/CEF path
passed the operator slice. [Before](window-before.png) and [after](window-after.png)
show the same fixture button changing red to green after window pointer input.
[Oracle](oracle.json) independently recorded exactly one HTTP click.
[Window result](window-result.json) records ten native window readback frames
and success. The hosted window image is 1024×674 physical pixels; this is not
the controlled 1280×713 browser viewport required for later benchmarks.

The subsequent actual Claude adapter → bound Thread browser service → actual
window operation failed, as expected before #194 adapter implementation:
[provider result](claude-observe-result.json) contains `isError: true` and
`Browser agent control is not available in this build`. The peer's unique
probe marker and PID tie this response to the intended deterministic external
CLI, not a model. [Host protocol](claude-provider-host.jsonl) and
[provider window result](claude-window-result.json) retain that feature red.

This establishes real packaged operator rendering/input on macOS and the
actual provider-boundary red. It does not establish Windows support, provider
control success, annotations, browser screenshot agreement, resource budgets or
complete #196 acceptance. The logs also exposed an owner-cleanup race: stopping
the Server while the ordinary window was still alive let that window reconnect
and restart it. The next harness revision moves the public-protocol stop to
the external owner after window exit; clean shutdown needs that subsequent run.
