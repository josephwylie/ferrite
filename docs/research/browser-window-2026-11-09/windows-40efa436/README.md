# Windows provider launch blocked by initial operator readiness

Source `40efa43638f78390ceaa8e100cbef94b973e7cae`.
Run https://github.com/josephwylie/ferrite/actions/runs/42571010383
Job https://github.com/josephwylie/ferrite/actions/runs/42571010383/job/129008123805
Artifact `browser-window-Windows-X64`, ID `13653008924`.

The normal actual Window/Preview/render/pointer probe passes. In the subsequent
provider-mode launch, the driver dispatches its initial GPUI pointer gesture,
but the actual Preview stays red. The [window result](claude/window-result.json)
records 245 frames and a 60-second timeout with `provider_started: false`.
There is no provider result or tool trace: no Claude browser action was started.
The retained [current native PNG](claude/window-current.png) was visually inspected
and still shows the red fixture button. The external click count was not retained
on failure by this harness revision, so this evidence alone cannot distinguish
missing page activation from missing repaint after activation.

This is a readiness/input failure before the provider boundary, not a failed
Observe or a regression of the pin-binding fix. The production stale-input guard
may correctly discard a queued gesture if the first painted frame precedes the
main-document acknowledgement. The next harness revision requires the public
service's loaded live document to be stable across frames before injecting its
single gesture; it does not retry the input or relax stale-input protection.
It also retains the independent HTTP click count on failure.

The macOS job from this exact source passed the actual provider action/blue
Preview/two-click oracle slice. Windows still requires its own provider pass.
