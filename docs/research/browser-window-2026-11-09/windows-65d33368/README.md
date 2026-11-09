# Actual Windows native-window startup failure

Source commit: `65d33368e301beb467d1825ff059cbea47a5a297`.
Packaged workflow: https://github.com/josephwylie/ferrite/actions/runs/42567297438
Windows job: https://github.com/josephwylie/ferrite/actions/runs/42567297438/job/128996481803
Artifact: `browser-window-Windows-X64` (retained raw files alongside this note).

The staged CEF bootstrap reached the actual native Window and production
CockpitView (`window-start.json`, scale 1), then exited with `0xC00000FD`.
No frame, pointer result or provider acceptance result was produced. The
failure-only naked executable comparison also reached the native Window,
then exited with the same code. Its app.log explicitly reports that the
main thread overflowed its stack. Both PE headers reserve 1,048,576 bytes
and commit 4,096 bytes. This establishes a stack overflow; it does not
establish that a particular larger reserve fixes it or prove browser use.
The naked comparison reports Visuals unavailable because it lacks the
packaged Windows sandbox bootstrap.

The external runner's public Stop request shut the isolated Server down
cleanly after each application exit. The raw server-process.log files end
with “stopping, as a window asked” and “quitting: 0 sessions ended”; neither
launch generated cleanup-error.txt. OS sampling retained only the Server
process after the packaged app exited. No CEF/controller rendering or
resource acceptance is claimed from this failed run.

The macOS job in this run failed before compilation: the sccache release
download returned HTTP 504 on all three attempts. It supplies no new macOS
feature evidence. Earlier actual macOS operator Preview evidence remains
in ../macos-0abd8f1b.
