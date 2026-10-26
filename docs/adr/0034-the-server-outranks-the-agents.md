---
status: accepted
date: 2026-10-26
---

# The Server outranks the agents

The facts this rests on, with their sources (XNU, libpthread, system_cmds,
launchd.plist(5), Microsoft Learn, man7) and the local experiments that
confirmed them, are in `docs/research/process-qos-and-priority-inheritance.md`.

## Context

Every act of a window waits for the Server's reply (ADR 0025), so the
Server's pump thread is on the operator's critical path. The provider CLIs
are the Server's children, and everything the agents run (builds, test
runners, engines) descends from them. All of it ran at the same class as
the pump. On the operator's machine (10 cores, load average 100–130) the
window's main thread ran at priority 54, the Server at 31, and its pump at
27 (timeshare decay for its own CPU use), level with or below the agents'
processes. A revive replay took 5.3 s of CPU and 31.7 s of wall time, and
every window act queued for a CPU behind the agents.

There are two ways a Server starts. Either the window spawns it, or launchd
does as a LaunchAgent. The LaunchAgent had no `ProcessType`, which
launchd.plist(5) calls Standard: "light resource limits … throttling its
CPU usage and I/O bandwidth". Observed, that is a UTILITY clamp on the
Server itself and on everything it starts, which nothing inside can lift.
The plist left `ProcessType` out to avoid Background's throttling, and
that throttled the Server anyway.

## Decision

The agents run one class below the Server, and the Server's threads that
the operator waits on ask for the class above default. Under contention
the operator's work wins. When nothing contends, the agents still have the
whole machine: P-cores, unthrottled disk.

- **One constructor for an agent's child: `spawn::agent(program)`.** It is
  used by Sessions (through `providers::launched`) and by the helper CLIs
  the Server runs for them: the titler and other one-shots, command lists,
  the Codex model catalog. It also sets the agents' environment and, on
  Windows, no console window. Version probes, `--version` discovery and the
  operator's own provider upgrade stay at the Server's class: they are the
  operator's, and a probe's `NotFound` names a missing CLI.
  - **macOS:** `/usr/sbin/taskpolicy -c utility <program> …`, a UTILITY
    QoS clamp. Only `posix_spawn` can set a clamp, and std's `Command` has
    no hook into it. In a `pre_exec` QoS calls fail with EPERM, and exec
    resets thread QoS. `taskpolicy` spawns with `POSIX_SPAWN_SETEXEC`, so
    the pid, pipes and exit status stay the program's. The clamp is task
    policy, copied into every task forked or exec'd beneath it, and a
    descendant cannot raise itself above it. UTILITY, not BACKGROUND:
    BACKGROUND (and `PRIO_DARWIN_BG`, `taskpolicy -b`) run at priority 4,
    only on E-cores, with darwinbg's disk throttling, even on an idle
    machine. UTILITY threads run on every core. Their IO is throttled only
    while more important IO to the same disk is in flight. `nice` does
    nothing here: XNU ignores it for threads that have a QoS class.
  - **Windows:** `BELOW_NORMAL_PRIORITY_CLASS` at creation. Children of a
    below-normal process inherit the class, so the `.cmd` shim's CLI and
    what it starts do too. Not EcoQoS: it picks efficient cores and
    frequencies even when the machine is idle.
  - **Linux** (`ferrited`, ADR 0026): `nice(10)` before exec. Nice is
    inherited across fork and exec. Autogroup weighs nice only within a
    session, and agents share the Server's session. Nothing more (cgroups,
    `SCHED_BATCH`) until a machine shows the need.
- **`spawn::serve_the_operator()` on the pump and on each window
  connection's reader and writer threads.** On macOS it requests
  USER_INITIATED: the pump's work is replies the operator waits for, not
  frames. A thread does not inherit its creator's QoS there, so each of
  these threads asks for its own. On Windows the threads run at
  `THREAD_PRIORITY_ABOVE_NORMAL`. On Linux an unprivileged process cannot
  raise itself, and the agents' nice does the work.
- **The LaunchAgent's `ProcessType` is `Interactive`** (ADR 0025 amended).
  The man page reserves it for a job an app's responsiveness depends on;
  every window act waits on this one. Adaptive needs XPC.

What each class comes to on macOS:

| Server started by | Pump | Agents and their descendants |
|---|---|---|
| the window (a detached child) | 31: XNU squashes a requested class above default to 31 for a process with no UI role | 20 |
| launchd, `Interactive` | 37 | 20 |
| launchd, no `ProcessType` (before) | 20, clamped | 20 |

Measured with a release Server on an empty Store. Its Session was a stub
CLI that streams a reply and starts 30 short-lived busy-loop processes
(3× the cores), as a build does. The round trip of a no-op command went
from p50/p90/p99 0.2/1.0/34.7 ms and 0.2/2.1/61.7 ms (max 146 and 224 ms)
to 0.1/0.2/2.5 ms and 0.2/1.1/13.0 ms (max 17 and 125 ms), in two
alternating rounds with the machine's own load at 30–80. The stub's
processes ran at 20 instead of 31, and the pump stayed at 31 instead of
decaying to 27.

## Consequences

- An agent's build on a busy machine is slower than before: it yields to
  the window and to anything else at default class. Alone on the machine
  it is not.
- On macOS every agent process has a `taskpolicy` exec in front of it. A
  missing program is `taskpolicy` exiting 66, not a spawn error, so
  callers that name a missing CLI keep their version probe in front of the
  spawn.
- A Server started by the window cannot rank its pump above 31 on macOS
  without the private `PRIO_DARWIN_ROLE`. The separation from the agents
  comes from their clamp, not from the pump's request.
- A loaded LaunchAgent keeps its old plist until logout (ADR 0025), so a
  Server launchd already runs stays clamped until the next login.
