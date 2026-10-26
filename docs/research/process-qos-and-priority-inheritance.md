# Process QoS and priority inheritance: Server pump vs agent subtrees

2026-10-26. Branch `perf/interactive-server-qos`. Research only, no production code changed.

The question: how to run the Server's pump thread at an interactive class, and every
agent Session with all its descendants (node, cargo, rustc, stockfish) at a lower
class. Interactive work should win under contention, and agents should still get
full speed (P-cores, unthrottled IO) when the machine is idle.

Sources are primary only. Abbreviations used below:

- `XNU` = <https://github.com/apple-oss-distributions/xnu/blob/xnu-12377.1.9> (the newest
  published tag. The test Mac runs `xnu-13432.1.9`, macOS 27.0.1 (26A434), which is not
  published yet. Where a local experiment agrees with the source, the behaviour held on 13432.)
- `PTH` = <https://github.com/apple-oss-distributions/libpthread/blob/libpthread-539>
- `SYS` = <https://github.com/apple-oss-distributions/system_cmds/blob/system_cmds-1042.100.6.0.1>
- `SDK` = `/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk/usr/include`
- `RUST` = `~/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/src/rust/library/std/src` (rustc 1.97.1)

"Local experiment" means a throwaway C program under `/tmp/qosr` run on this Mac:
macOS 27.0.1, 10 cores (4P+6E), `kern.sched: edge`. All of those processes and launchd
jobs were removed afterwards.

## Answer

### macOS

1. **Agents: spawn each agent with a UTILITY QoS clamp.** Use the public
   `posix_spawnattr_set_qos_class_np(attr, QOS_CLASS_UTILITY)`. Simpler still, run the
   agent through `/usr/sbin/taskpolicy -c utility <agent> …`. `taskpolicy` uses
   `POSIX_SPAWN_SETEXEC`, so the agent keeps the PID that Rust's `Child` holds. The facts
   behind this:
   - The clamp is task policy (`trp_qos_clamp`). `task_policy_create` copies it into every
     new task. `execve` on current XNU also builds a new task from the old one. So the clamp
     reaches the whole subtree through fork, exec and posix_spawn. There is no setter for it
     after spawn.
   - Under the clamp, every thread's effective QoS is `MIN(clamp, requested)`, even a thread
     that asked for USER_INTERACTIVE. Priority is capped at 20.
     - Local experiment: a grandchild that called `pthread_set_qos_class_self_np(USER_INTERACTIVE)`
       got rc=0 and stayed at base priority 20.
   - Only BACKGROUND and MAINTENANCE are kept off the P-cores. The Edge scheduler source says
     UTILITY "[can] be scheduled on all CPUs".
     - Local experiment at load average 100: a UTILITY thread spent 59% of its samples on the
       P-cluster. A BACKGROUND thread never ran there.
   - UTILITY's IO tier is TIER1. A tiered IO is only throttled while higher-tier IO to the
     same device is in flight or within the last 25 ms. Idle disks are not throttled.
   - Do **not** use BACKGROUND, `PRIO_DARWIN_BG` or `taskpolicy -b`. They pin to E-cores,
     set priority 4 and IO TIER2, and turn on darwinbg.
   - Do **not** rely on `nice`. XNU ignores nice for any thread that has a QoS class, which
     on modern macOS is effectively every thread.
     - Local experiment: `nice -n 10` left threads at 31.
2. **Server: run the pump thread at `QOS_CLASS_USER_INTERACTIVE`.** Call
   `pthread_set_qos_class_self_np` from inside the pump thread. `pthread_create` (and so
   `std::thread`) does **not** inherit the creator's QoS: new threads start at
   QOS_CLASS_DEFAULT (31). What the request actually yields depends on how the Server was
   started:
   - **Spawned by Ferrite.app (today).** The Server inherits the app's apptype
     (`TASK_APPTYPE_APP_DEFAULT`) but not its role. XNU squashes USER_INTERACTIVE and
     USER_INITIATED to DEFAULT (31) for an app with no role.
     - Local experiment: the live Server (pid 2505) has role 0 and all 61 of its threads at 31.
     - Even so, 31 beats clamped agents at 20, which is the separation we need. Reaching
       46/47 needs the private `setpriority(PRIO_DARWIN_ROLE, 0, PRIO_DARWIN_ROLE_UI)`
       (local experiment: 31 → 46), or spawning the Server with a daemon apptype.
     - The role is not inherited across fork, so agents stay at a 31 ceiling either way.
   - **Started by launchd as a LaunchAgent with no `ProcessType`.** This is the "Standard"
     type, and it is the worst case: launchd spawns it UTILITY-clamped.
     - Local experiment: `launchctl submit` job, spawn type `daemon (3)`. Main thread QoS
       UTILITY, priority 20, a USER_INTERACTIVE request stays at 20, children at 20.
     - The pump and every agent then share priority 20, and the clamp cannot be lifted.
       This alone would produce the reported queueing.
     - **Set `ProcessType` = `Interactive`.**
       - Local experiment: spawn type `interactive (4)`. Main thread DEFAULT 31,
         USER_INTERACTIVE gives 37, children 31.
       - Plus a UTILITY clamp on agents, that gives pump 37 against agents 20.
3. **Rust `std::process::Command` cannot set spawn attributes.** It uses `posix_spawnp`
   only when no `pre_exec` closure is set, and offers no attrs hook.
   - Inside `pre_exec`, after `fork`, `pthread_set_qos_class_self_np` fails with **EPERM**.
     A forked child's thread has no kernel QoS yet.
     - Local experiment: rc=1 (EPERM).
   - Any thread QoS would be reset at `execve` anyway (`task_set_main_thread_qos`).
   - So the clamp has to come from `posix_spawn` itself. Use the `taskpolicy` trampoline,
     a direct `libc::posix_spawnp` call, or a self-trampoline that calls `posix_spawnp`
     with `POSIX_SPAWN_SETEXEC`.

### Windows

- **Agents: `CREATE_PROCESS` with `BELOW_NORMAL_PRIORITY_CLASS`.** In Rust this is
  `CommandExt::creation_flags(0x4000)`. Microsoft documents that children of a
  BELOW_NORMAL or IDLE process inherit that class by default. Rust std, and libuv (node),
  pass no priority-class flag, so the class flows down the tree.
  - Any process can raise its own class again. A job's `JOB_OBJECT_LIMIT_PRIORITY_CLASS`
    prevents that (the raise is "silently ignored"). But the docs say the caller "must
    enable `SE_INC_BASE_PRIORITY_NAME`". UNCONFIRMED whether that also applies to a
    below-normal class.
- **Server pump: `SetThreadPriority(THREAD_PRIORITY_ABOVE_NORMAL or HIGHEST)`.** Optionally
  also `ABOVE_NORMAL_PRIORITY_CLASS` for the whole Server. Windows strictly runs the
  highest-priority ready thread first, so BELOW_NORMAL agents still use every idle CPU.
- **Avoid EcoQoS** (`PROCESS_POWER_THROTTLING_EXECUTION_SPEED`). It "always selects most
  efficient CPU frequency and schedules to efficient cores", idle or not.
  - Caveat: Windows' QoS heuristics may treat reduced thread priority as a lower QoS level.
    The docs do not say how much.

### Linux (`ferrited`)

- **Agents: `setpriority(PRIO_PROCESS, 0, +N)` in `pre_exec`.** Optionally also
  `SCHED_BATCH`. Both nice and scheduling policy are inherited across fork and preserved
  across execve.
- **Autogroup caveat.** With autogroup on, nice only matters relative to processes in the
  same session. An agent started with `setsid` would sit in its own autogroup, and its nice
  value would have no effect against the Server.
  - Either do not `setsid` agents, or also write the nice value to `/proc/self/autogroup`,
    or use a cgroup CPU weight. Any CPU cgroup overrides autogroup.
- `SCHED_IDLE` is "lower even than a +19 nice value" and would starve agents under load.

## 1. `pthread_set_qos_class_self_np` and the QoS priority table

**What it does.** It sets the requested QoS class and relative priority of the calling
thread only. `pthread_set_qos_class_np` on another thread returns EPERM.

- Allowed classes: USER_INTERACTIVE, USER_INITIATED, DEFAULT, UTILITY and BACKGROUND.
  Anything else returns EINVAL. `relpri` must be in `[QOS_MIN_RELATIVE_PRIORITY, 0]`.
  Source: `SDK/pthread/qos.h` lines 119-157, and `PTH/src/qos.c#L147-L169`.
- It enters the kernel through `__bsdthread_ctl(BSDTHREAD_CTL_SET_SELF, …)`
  (`PTH/src/qos.c#L272-L310`).

**Interaction with `pthread_setschedparam`.** The header says: "Subsequent calls to
interfaces such as pthread_setschedparam() that are incompatible or in conflict with the
QOS class system will unset the QOS class … A thread so modified is permanently opted-out
of the QOS class system and calls to this function … will fail and return EPERM."
(`SDK/pthread/qos.h` around line 125.)

- The kernel side: `bsdthread_set_self` returns EPERM for a non-workqueue thread whose
  requested QoS is UNSPECIFIED. The comment is "Threads opted out of QoS can't change QoS"
  (`XNU/bsd/pthread/pthread_workqueue.c#L2416-L2424`, with `thread_has_qos_policy` at
  `XNU/osfmk/kern/thread_policy.c#L157-L160`).
- Setting a non-QoS thread policy removes the QoS from a thread
  (`thread_policy.c#L311-L321`).
- The same EPERM check bites in a forked child; see section 3c.

**Priority per class.** The table is `thread_qos_policy_params` at
`XNU/osfmk/kern/thread_policy.c#L78-L123`. The constants come from
`XNU/osfmk/kern/sched.h#L158-L166`.

| QoS | base pri | IO tier | throughput tier | latency (timer) tier |
|---|---|---|---|---|
| USER_INTERACTIVE | 46 (`BASEPRI_BACKGROUND`), 47 when `qos_ui_is_urgent` (foreground app) | TIER0 | 0 | 0 |
| USER_INITIATED | 37 | TIER0 | 1 | 1 |
| DEFAULT (LEGACY) | 31 | TIER0 | 1 | 1 |
| UTILITY | 20 | TIER1 | 2 | 3 |
| BACKGROUND | 4 (`MAXPRI_THROTTLE`) | TIER2 ("possibly overridden by bg_iotier") | 5 | 3 |
| MAINTENANCE | 4 | TIER3 | 5 | 3 |

- The 46 → 47 bump for the urgent UI case is at `thread_policy.c#L1102-L1105`.
- The relative priority is ignored once the QoS is clamped (`thread_policy.c#L1694-L1709`).
- **Nice is ignored for QoS threads.** The QoS branch of the priority calculation carries
  the comment `/* TODO: factor in renice priority here? */` (`thread_policy.c#L1107`). Only
  the non-QoS branch adds `thread->task_priority`, which is where nice ends up.

**Ceilings that squash a requested QoS.** These are in `task_policy.c#L831-L888`.

- Apps (`APP_DEFAULT` or `APP_NONUI`) with role FOREGROUND, BACKGROUND, DEFAULT or CONTROL
  have no ceiling.
- `TASK_NONUI_APPLICATION` and an **unspecified role** ("Apps that don't have an
  application role get USER_INTERACTIVE and USER_INITIATED squashed to LEGACY",
  `#L881-L884`) are capped at LEGACY, which is 31.
- Every non-app apptype, including daemons and `APPTYPE_NONE`, is capped at USER_INITIATED,
  which is 37 (`#L887-L888`).
- The ceiling applies only to threads that already have a QoS (`thread_policy.c#L1686-L1690`).

Local experiment results for a thread that requests each class:

| process | USER_INTERACTIVE gives | USER_INITIATED gives |
|---|---|---|
| child of Ferrite.app (apptype inherited, role 0) | 31 | 31 |
| same, after `setpriority(PRIO_DARWIN_ROLE, 0, 2 /*UI*/)` | 46 | not measured |
| same, after `PRIO_DARWIN_ROLE` 1 (UI_FOCAL) | 47 | not measured |
| spawned with `posix_spawnattr_setprocesstype_np(DAEMON_STANDARD or DAEMON_INTERACTIVE)` | 37 | 37 |
| under `taskpolicy -a` (APP_DEFAULT plus role UI) | 46 | 37 |

## 2. Default QoS of an ordinary process, and thread inheritance

**Main thread.** At exec or spawn the kernel calls
`task_set_main_thread_qos(new_task, main_thread)` (`XNU/bsd/kern/kern_exec.c#L4857-L4861`
for spawn, `#L5590-L5594` for execve). That uses `task_compute_main_thread_qos`
(`XNU/osfmk/kern/task_policy.c#L2212-L2250`), which picks by apptype:

| apptype | main-thread QoS |
|---|---|
| `APP_DEFAULT` | USER_INTERACTIVE |
| `DAEMON_INTERACTIVE`, `DAEMON_STANDARD`, `DAEMON_ADAPTIVE`, `DRIVER`, `APP_NONUI` | LEGACY |
| `DAEMON_BACKGROUND` | BACKGROUND |
| `NONE` | UNSPECIFIED |

A clamp, if present, takes the `MIN`.

**Apptype is inherited.** `task_policy_create` runs at every task creation and calls
`task_set_requested_apptype(task, parent_task->requested_policy.trp_apptype, true)`
(`task_policy.c#L734-L748`). Every task is created by fork, and execve too (next
paragraph), so a CLI started from a GUI app is an "app" as far as the kernel is concerned.

- Local experiment: `pbi_flags` shows `PROC_FLAG_APPLICATION` (`0x1000000`, from
  `XNU/bsd/sys/proc_info_private.h#L118`) on:
  - this shell;
  - the Ferrite Server (pid 2505);
  - a `claude` agent (pid 13417).
- Their main threads report a requested QoS of USER_INTERACTIVE, but run at base
  priority 31 (role-less ceiling).
- The **role** is not part of `task_policy_create`, so it is not inherited at fork. It is
  carried across exec only (`proc_inherit_task_role`, `task_policy.c#L2198-L2206`).
  - Local experiment: a parent at role 2 (UI) forked a child; the child had role 0, and
    USER_INTERACTIVE gave it 31.

**Threads made with `pthread_create` do not inherit the creator's QoS.**

- `_pthread_attr_default.qosclass` is a compile-time LEGACY (DEFAULT) value
  (`PTH/src/pthread.c#L100-L111`). `pthread_attr_init` copies that default (`#L337-L341`).
  `_pthread_create` passes `attrs->qosclass` to the kernel as `PTHREAD_START_QOSCLASS`
  (`#L1547-L1550`).
- Rust's `std::thread` uses `pthread_attr_init` (`RUST/sys/thread/unix.rs#L49`), so Rust
  threads start at DEFAULT.
- Apple's guide does not say whether threads inherit
  ([Energy Efficiency Guide for Mac Apps, "Prioritize Work at the Task Level"](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/PrioritizeWorkAtTheTaskLevel.html)).
  It does say "In an app, the main thread runs at a QoS level of user-interactive".
- Local experiment: the main thread is USER_INTERACTIVE, but `pthread_create(NULL attr)`
  gives a DEFAULT child, and that stays true after the main thread sets USER_INTERACTIVE.
  Inside a clamped process the DEFAULT child runs at the clamp (20).
- **Consequence:** the pump must call `pthread_set_qos_class_self_np` itself, on its own
  thread.

## 3. `posix_spawnattr_set_qos_class_np`, clamp inheritance, escape

### 3a. What it does

- **Header.** "The QOS class specified at the time of process spawn determines both the
  initial requested QOS class of the main thread in the new process, and the
  interpretation by the system of all QOS class values requested by threads in the
  process." Only UTILITY or BACKGROUND are accepted, otherwise EINVAL
  (`SDK/pthread/spawn.h#L37-L66`).
  - Local experiment: USER_INTERACTIVE → rc=22.
- **libpthread.** It maps the class to `posix_spawnattr_set_qos_clamp_np(attr,
  POSIX_SPAWN_PROC_CLAMP_UTILITY | _BACKGROUND | _MAINTENANCE)` (`PTH/src/qos.c#L633-L645`).
  `MAINTENANCE` is also accepted there, beyond what the header lists.
- **Kernel, at spawn.** `exec_handle_spawnattr_policy` maps the clamp to
  `THREAD_QOS_UTILITY` / `_BACKGROUND` / `_MAINTENANCE` and calls
  `proc_set_task_spawnpolicy` (`XNU/bsd/kern/kern_exec.c#L2586-L2652`). That sets
  `task->requested_policy.trp_qos_clamp = qos_clamp` (`XNU/osfmk/kern/task_policy.c#L2175-L2177`).
  - It runs whenever spawn attributes are present, including the `POSIX_SPAWN_SETEXEC`
    path (`kern_exec.c#L4820-L4825`; there is no `spawn_no_exec` condition).

What the clamp does to the task:

- **Thread QoS.** "A task clamp will result in an effective QoS even when requested is
  UNSPECIFIED". Otherwise `next.thep_qos = MIN(task_effective.tep_qos_clamp, next.thep_qos)`
  (`thread_policy.c#L1670-L1678`).
- **Task priority.** The task's priority and max priority are capped at
  `qos_pri[clamp]`, 20 for UTILITY (`task_policy.c#L1270-L1276`). The thread priority is
  capped by `thread->max_priority` (`thread_policy.c#L1138-L1140`).
- **IO, latency and throughput tiers** are raised to at least the clamp's tier
  (`task_policy.c#L1017-L1018`, `#L1044-L1045`, `#L1066-L1067`).
- **BACKGROUND and MAINTENANCE clamps also set darwinbg** (`wants_darwinbg`, which leads to
  `tep_lowpri_cpu`, `task_policy.c#L946-L950` and `#L963-L967`). That means priority 4
  plus background sockets.

### 3b. Inheritance across fork, exec and posix_spawn: yes, the whole subtree

- `task_policy_create` copies `trp_qos_clamp` from the parent task (`task_policy.c#L746`).
  It also copies `trp_int_darwinbg`, `trp_ext_darwinbg`, the IO tiers and the apptype.
- It is called from `task_create_internal` whenever there is a parent task
  (`XNU/osfmk/kern/task.c#L1970-L1973`).
- fork goes through `cloneproc`/`fork_create_child`. **execve also creates a fresh task
  from the old one**: "For execve case, create a new proc, task and thread … the new proc
  would take place of the old proc" with `cloneproc(old_task, NULL, p, CLONEPROC_EXEC)`
  (`kern_exec.c#L4180-L4189` for spawn-as-exec and `#L5423-L5433` for execve). So the
  copied clamp survives exec as well.

Local experiment (`cs ut spawn /bin/sh -c '…'`):

- `sh` forks and execs `q1`: main thread UTILITY at 20; a new pthread at DEFAULT runs at 20.
- `sh` → `cs none spawn ./q1` (a plain `posix_spawn` with no attrs): the grandchild is
  still at 20.
- A background `sleep` is at `20T`.
- With the clamp applied via `POSIX_SPAWN_SETEXEC` (`cs ut exec ./q1`, and the
  fork-then-SETEXEC case `sh -c 'exec ./cs bg exec ./q1'`), the same results hold
  (20 and 4).

**No runtime setter.** The only writers of `trp_qos_clamp` in this XNU are spawn
(`#L2176`) and inheritance (`#L746`); there is no `TASK_POLICY_QOS_CLAMP` path from
user space (grep over `bsd/` and `osfmk/`). A clamped process cannot remove its own
clamp, and an unclamped process cannot add one to itself afterwards. Use spawn time.

### 3c. Can a descendant raise itself above the clamp? No

The effective QoS is the `MIN` with the clamp (`thread_policy.c#L1670-L1678`), and the
priority is capped by the task's max priority.

- Local experiment: inside the clamp, `pthread_set_qos_class_self_np(USER_INTERACTIVE)`
  returned 0, the TSD read back "UI", and `base_pri` stayed 20.
- Exceptions in source:
  - **Turnstile promotion above a darwinbg clamp** (`thread_policy.c#L1657-L1666`). This
    is not possible with BACKGROUND/MAINTENANCE clamps, which set `bg_clamps_turnstiles`.
  - **Transient promotions** (`user_promotion_basepri`, `thread_policy.c#L1127` and
    `#L1147-L1150`). These are priority-inheritance boosts while a higher-priority thread
    waits on a lock or IPC that this thread holds. That is intended: a clamped agent the
    Server is blocked on gets boosted for the duration.

### 3d. Rust `Command`, `pre_exec`, and what survives exec

- `posix_spawn` is skipped when there is a uid/gid change, a PATH-lookup env case,
  **any `pre_exec` closure**, groups, or chroot (`RUST/sys/process/unix/unix.rs#L449-L468`).
  On Apple targets it is also skipped for `setsid` (`#L749-L757`).
- There is no API for adding `posix_spawnattr_*` calls, so a clamp cannot be set through
  `Command`.
- **Thread QoS in `pre_exec` does not work.**
  - The forked child's thread is created with `default_thread_requested_policy`
    (`XNU/osfmk/kern/thread.c#L1381`), so its kernel QoS is UNSPECIFIED. fork does not call
    `task_set_main_thread_qos`.
  - `pthread_set_qos_class_self_np` then hits the EPERM check (section 1).
  - Local experiment: in a forked child the TSD still read USER_INTERACTIVE (copied memory),
    and setting UTILITY returned EPERM.
  - Even if it worked, exec recomputes the main-thread QoS from the apptype and clamp
    (`kern_exec.c#L5594`).
  - Local experiment: after fork + exec, `q1`'s main thread was back at USER_INTERACTIVE/31.
- **Task-level policy set in `pre_exec` survives exec**, because `task_policy_create`
  copies it:
  - darwinbg via `setpriority(PRIO_DARWIN_PROCESS, 0, PRIO_DARWIN_BG)`. Local experiment:
    the exec'd image showed `PROC_FLAG_DARWINBG`.
  - nice. `p_nice` is in the `p_forkcopy` struct group (`XNU/bsd/sys/proc_internal.h#L376-L390`),
    copied at `XNU/bsd/kern/kern_fork.c#L1026`, and reapplied by `resetpriority` (`#L504-L506`).
    `cloneproc(…CLONEPROC_EXEC)` uses the same path. Local experiment: `nice -n 7 sh -c 'ps …'`
    showed NI 7.
  - IO policy.
- **Async-signal-safety.** `setpriority` is not on the POSIX async-signal-safe list
  ([signal-safety(7)](https://man7.org/linux/man-pages/man7/signal-safety.7.html); the macOS
  `sigaction(2)` list does not include it either). It is a plain syscall wrapper in practice,
  but treat that as UNCONFIRMED. The Rust doc requires `pre_exec` closures to be
  async-signal-safe (`RUST/os/unix/process.rs#L89`).
- **Ways that do work for the clamp:**
  - **`/usr/sbin/taskpolicy -c utility prog args…`.** `taskpolicy` sets
    `posix_spawnattr_set_qos_clamp_np` and calls `posix_spawnp` with
    `spawn_flags = POSIX_SPAWN_SETEXEC` (`SYS/taskpolicy/taskpolicy.c#L69`, `#L251-L263`,
    `#L292`). It replaces itself with the target, so the PID, the pipes and `Child::wait`
    are unchanged.
  - **A direct `libc::posix_spawnp`** with `posix_spawnattr_set_qos_class_np`. This loses
    std's `Child` conveniences.
  - **A tiny self-trampoline subcommand** (`ferrite exec-clamped -- prog…`) doing
    `posix_spawnp(SETEXEC)` with the attr.

## 4. nice, `PRIO_DARWIN_BG`, `taskpolicy(8)`

**nice (`setpriority(PRIO_PROCESS)`).**

- `donice` stores `p_nice` and calls `resetpriority` (`XNU/bsd/kern/kern_resource.c#L693-L727`).
  That becomes `task_importance(task, -p_nice)` (`XNU/bsd/kern/kern_synch.c#L450-L453`),
  and `task->importance` is added to the task priority (`task_policy.c#L1268-L1269`,
  "factor in 'nice' value").
- It only reaches **non-QoS** threads, via `thread->task_priority` (`thread_policy.c#L1109-L1120`).
  QoS threads ignore it (`#L1107`).
- Lowering nice needs privilege (`kern_resource.c#L714-L716`, EACCES).
- It is inherited across fork and exec (section 3d).
- Local experiment: `nice -n 10 sleep` showed NI 10 at PRI 31, and `q1` threads under
  nice 10 stayed at 31. On macOS today nice is effectively a no-op for app-, daemon- or
  clamp-typed processes, which is every process we spawn.

**`PRIO_DARWIN_BG`.**

- `setpriority(PRIO_DARWIN_PROCESS, pid, PRIO_DARWIN_BG)` sets the `TASK_POLICY_DARWIN_BG`
  task policy (`kern_resource.c#L1216-L1259`). A caller needs the same uid (or root) plus
  the MAC check.
- `PRIO_DARWIN_THREAD` does the same for one thread (`#L1378-L1382`).
- Its effects:
  - `tep_lowpri_cpu` sets priority and max priority to 4 (`task_policy.c#L963-L967` and
    `#L1250-L1252`).
  - The IO tier rises to `trp_bg_iotier`, default TIER2 (`#L1005-L1007`, `#L248`).
  - Sockets are backgrounded.
  - It is inherited (`trp_int_darwinbg` and `trp_ext_darwinbg` are copied at `#L738-L739`).
- Local experiment: `taskpolicy -b ./q1` gave base_pri 4 on every thread, even after a
  USER_INTERACTIVE request.
- P/E effect: priority ≤ 4 is the BACKGROUND bucket, which is E-only (section 5).

**`taskpolicy(8)`** (man page on this Mac; source `SYS/taskpolicy/taskpolicy.c`):

| flag | effect |
|---|---|
| `-c utility\|background\|maintenance` | `posix_spawnattr_set_qos_clamp_np` (`#L101-L106`, `#L261-L263`) |
| `-b` / `-B` | `setpriority(PRIO_DARWIN_PROCESS, pid, PRIO_DARWIN_BG / 0)` on itself (pid 0) before the SETEXEC spawn, or on `-p pid` (`#L202-L213`) |
| `-d policy` | `setiopolicy_np(IOPOL_TYPE_DISK, IOPOL_SCOPE_PROCESS, …)` (`#L216-L220`) |
| `-g policy` | same with `IOPOL_SCOPE_DARWIN_BG` (`#L223-L227`) |
| `-t` / `-l` | `task_policy_set(TASK_OVERRIDE_QOS_POLICY)` for the throughput/latency tier (`#L230-L245`) |
| `-a` | `posix_spawnattr_setprocesstype_np(APP_DEFAULT)` plus `posix_spawnattr_set_darwin_role_np(PRIO_DARWIN_ROLE_UI)` (`#L275-L281`) |

The man page states: "All children of the specified program also inherit these policies."
The binary is signed with `com.apple.private.task_policy` (`taskpolicy-entitlements.plist`).
`-c` worked unprivileged in the local experiments.

## 5. Apple Silicon P vs E

- **This Mac runs the Edge scheduler** (`sysctl kern.sched` = `edge`). Its comment
  (`XNU/osfmk/kern/sched_clutch.c#L5993-L5999`): "The Edge scheduler supports per-QoS
  recommendations for thread groups. This enables lower QoS buckets (such as UT) to be
  scheduled on all CPUs on the system. The only restriction is for BG/Maintenance QoS
  classes for which the performance controller would never recommend execution on the
  P-cores."
- **Per-cluster placement comes from the performance controller (CLPC).** Each
  (thread group, bucket) pair has a preferred cluster set by CLPC
  (`sched_edge_thread_preferred_cluster`, `#L1489-L1502`). CLPC is not open source, so the
  exact UTILITY placement policy is UNCONFIRMED.
- **The older AMP scheduler, which is not used here,** put UTILITY and BACKGROUND on
  E-cores by default unless runtime policy said otherwise. See `recommended_pset_type`
  (`XNU/osfmk/kern/processor.c#L1843-L1880`) and the comment in
  `sched_amp_common.c#L466-L470`.
- **Apple's guidance:** "On Apple silicon, a task's QoS class influences whether the system
  runs that task. For example, the system is more likely to run background tasks on lower
  performance cores", and "transition to APIs that set QoS classes" instead of
  `setpriority`/`pthread_setschedparam`
  ([Tuning your code's performance for Apple silicon](https://developer.apple.com/documentation/apple-silicon/tuning-your-code-s-performance-for-apple-silicon),
  read via its JSON form).

**IO.** UTILITY is TIER1; BACKGROUND is TIER2 plus darwinbg's bg tier; MAINTENANCE is TIER3
(table in section 1; thread IO tier computation `thread_policy.c#L1760-L1780`).

- Throttling is conditional. `throttle_io_will_be_throttled_internal` returns
  `THROTTLE_DISENGAGED` unless a higher tier has IO in flight, or started a throttle window
  within `throttle_windows_msecs[level]`, on the same device
  (`XNU/bsd/miscfs/specfs/spec_vnops.c#L1845-L1880`).
- Live sysctls: `debug.lowpri_throttle_tier1_window_msecs: 25`,
  `debug.lowpri_throttle_tier2_window_msecs: 100`.

**Local experiment: which cluster a CPU-bound thread ran on.** A burn loop sampled
`TPIDR_EL0` (CPU number in bits 0-11, cluster id in bits 12-19, per
`XNU/osfmk/arm64/machine_machdep.h#L37-L50`). The load average was ~100 from other agents.
Cluster 0 is CPUs 0-5 (6 E-cores) and cluster 1 is CPUs 6-9 (4 P-cores).

| thread QoS | P-cluster share | iters/s |
|---|---|---|
| BACKGROUND | 0% (only CPUs 0-5) | 40 |
| UTILITY | 59% | 152 |
| DEFAULT | 52% | 526 |
| process under a UTILITY clamp | 32% | 353 |

**Conclusions:**

- **Confirmed:** BACKGROUND stays on E-cores and throttles IO (TIER2 plus darwinbg).
- **Confirmed:** UTILITY can use P-cores.
- **Confirmed:** UTILITY IO is TIER1 and only throttled behind higher-tier IO.

## 6. Does a UTILITY clamp slow agents on an idle machine?

**CPU priority** only orders runnable threads. 20 vs 31 matters only when threads compete
for a CPU.

- Under the Edge scheduler, UTILITY is not barred from P-cores (section 5). On an idle
  machine, nothing in the open source keeps a UTILITY thread off an idle P-core.
  UNCONFIRMED, because CLPC's per-bucket recommendation and DVFS (frequency) choice are
  closed source.
- No idle-machine measurement was possible (load average stayed 50-100 during the
  session). The earlier single-thread timings (7.6 s default vs 9.6 s UTILITY vs 14.9 s
  darwinbg) were taken under that load and say nothing about the idle case.

**Timer coalescing does apply regardless of load.** UTILITY's latency QoS is TIER_3
(table, plus `task_policy.c#L1044-L1045` for clamps). `tcoal_qos_adjust` uses
`latency_qos_scale[3]` / `latency_qos_abstime_max[3]` (`XNU/osfmk/kern/timer_call.c#L1829-L1845`).
Live sysctls: `kern.timer_coalesce_tier3_scale: -2`, `kern.timer_coalesce_tier3_ns_max: 75000000`.

- With a negative shift, leeway = `MIN((deadline-now) << 2, 75 ms)` (`timer_call.c#L1930-L1934`).
- So a 10 ms sleep may fire up to ~40 ms late, against `tier1_scale: 2` / 5 ms max for
  DEFAULT. This affects timer-driven loops (polling and backoff in node), not CPU-bound
  compiles.

**Throughput QoS** (TIER_2 for UTILITY) is only reported to the performance controller, as
the urgency parameter (`XNU/osfmk/kern/sched_prim.c#L6255-L6260`). Its effect is
UNCONFIRMED (CLPC is closed).

## 7. Windows priority classes and thread priority

**Inheritance of the class.** From
[CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw),
`dwCreationFlags`: "If none of the priority class flags is specified, the priority class
defaults to NORMAL_PRIORITY_CLASS unless the priority class of the creating process is
IDLE_PRIORITY_CLASS or BELOW_NORMAL_PRIORITY_CLASS. In this case, the child process
receives the default priority class of the calling process."

- Also: "If the calling process is IDLE_PRIORITY_CLASS or BELOW_NORMAL_PRIORITY_CLASS, the
  new process will inherit this class"
  ([Scheduling Priorities](https://learn.microsoft.com/en-us/windows/win32/procthread/scheduling-priorities)).
- IDLE: "The idle-priority class is inherited by child processes"
  ([SetPriorityClass](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setpriorityclass)).
- ABOVE_NORMAL and HIGH are **not** inherited.
- The intermediate spawners pass no class flag:
  - Rust std: `flags = self.flags | CREATE_UNICODE_ENVIRONMENT` (`RUST/sys/process/windows.rs#L324`).
  - libuv/node: `process_flags = CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT`,
    plus window and detach flags
    ([libuv src/win/process.c L1097-L1129](https://raw.githubusercontent.com/libuv/libuv/v1.x/src/win/process.c),
    v1.x at time of reading).
  - So BELOW_NORMAL flows down the agent tree unless some descendant sets a class
    explicitly.

**Base priorities.** From the table in Scheduling Priorities:

| class | NORMAL thread | ABOVE_NORMAL thread | HIGHEST thread |
|---|---|---|---|
| BELOW_NORMAL | 6 | 7 | 8 |
| NORMAL | 8 | 9 | 10 |
| ABOVE_NORMAL | 10 | 11 | 12 |

So a NORMAL-class Server with a HIGHEST pump (10) is above every thread of a BELOW_NORMAL
agent tree (≤ 8 short of TIME_CRITICAL).

**Scheduling and boosts.**

- Strict priority: "The system assigns time slices in a round-robin fashion to all threads
  with the highest priority…" (Scheduling Priorities).
- On using these levels: "A typical strategy is to use THREAD_PRIORITY_ABOVE_NORMAL or
  THREAD_PRIORITY_HIGHEST for the process's input thread". HIGH_PRIORITY_CLASS is "with
  care", and REALTIME "almost never".
- Dynamic boosts apply only to base priorities 0-15. The foreground NORMAL process gets
  boosted ([Priority Boosts](https://learn.microsoft.com/en-us/windows/win32/procthread/priority-boosts)).
- Lock-holder inversion is handled by AutoBoost
  ([Priority Inversion](https://learn.microsoft.com/en-us/windows/win32/procthread/priority-inversion)).
- How Windows keeps low-priority threads from starving is UNCONFIRMED (not on the pages read).

**`PROCESS_MODE_BACKGROUND_BEGIN`.** It lowers "resource scheduling priorities" (CPU, IO,
memory) and applies only to the current process (SetPriorityClass). Its threads "will never
be starved". It is set per process, and inheritance by children is UNCONFIRMED. That makes
it a poor fit for a whole tree.

## 8. EcoQoS and job objects

**EcoQoS.** From
[SetProcessInformation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation):
"When a process opts into enabling PROCESS_POWER_THROTTLING_EXECUTION_SPEED, the process
will be classified as EcoQoS. The system will try to increase power efficiency through
strategies such as reducing CPU frequency or using more power efficient cores … EcoQoS
should not be used for performance critical or foreground user experiences."

- [Quality of Service](https://learn.microsoft.com/en-us/windows/win32/procthread/quality-of-service):
  Eco "Always selects most efficient CPU frequency and schedules to efficient cores".
  It **hurts throughput even when idle**, so it is not suitable.
- Inheritance by child processes is UNCONFIRMED; neither page says.
- The same page: "Processes which directly own a window (or are descendants of window
  owning processes) are assigned a QoS level according to their visibility and focus
  state". When minimized or fully occluded that level is Low, which on battery means the
  most efficient frequency and efficient cores.
  - So the Server and agents (if Windows counts them as descendants of the Ferrite window)
    may already lose speed on battery when Ferrite is hidden. UNCONFIRMED how "descendant"
    is determined for a `DETACHED_PROCESS` / `CREATE_BREAKAWAY_FROM_JOB` Server.
- Heuristic: "threads running with reduced thread priority can imply a lower QoS level".
  BELOW_NORMAL agents may therefore be steered toward efficiency on hybrid CPUs. The
  magnitude is UNCONFIRMED.

**Job objects (apply to the whole tree).**

- Children join automatically: "by default any child processes it creates using
  CreateProcess are also associated with the job", unless the job sets
  `JOB_OBJECT_LIMIT_BREAKAWAY_OK` or `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`
  ([Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)).
- Limits hold: "If a process associated with a job attempts to increase its … process
  priority from the limit established by the job, the function calls succeed but are
  silently ignored." (same page)
- `JOB_OBJECT_LIMIT_PRIORITY_CLASS`: "Causes all processes associated with the job to use
  the same priority class … If the job is nested, the effective priority class is the
  lowest priority class in the job chain." For `PriorityClass`, the docs add "The calling
  process must enable the SE_INC_BASE_PRIORITY_NAME privilege."
  ([JOBOBJECT_BASIC_LIMIT_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_limit_information)).
  - Standard users may not hold that privilege. Whether it is enforced for BELOW_NORMAL is
    UNCONFIRMED; test it on Windows.
- `SchedulingClass` (0-9, default 5; above 5 needs the same privilege) is an alternative
  limit (same page).
- CPU rate control is a poor fit:
  - `JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP` and `MIN_MAX_RATE` are hard limits: "no threads
    associated with the job will run until the next interval". That throttles even when
    the machine is idle.
  - `WEIGHT_BASED` (1-9, default 5) gives "share of processor time … relative to other
    workloads"
    ([JOBOBJECT_CPU_RATE_CONTROL_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_cpu_rate_control_information)).
    Whether weight-based control ever throttles an idle machine is UNCONFIRMED from the docs.
- Ferrite already puts each agent in a `SessionJob` with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
  (`crates/ferrite-core/src/providers/job.rs` lines 64-81). The priority-class limit would
  go in the same `SetInformationJobObject` call.

## 9. Linux

From [setpriority(2)](https://man7.org/linux/man-pages/man2/setpriority.2.html):

- "A child created by fork(2) inherits its parent's nice value. The nice value is preserved
  across execve(2)."
- On Linux, nice is per-thread. New threads inherit the creator's value, so set it in the
  single-threaded `pre_exec` child.

From [sched(7)](https://man7.org/linux/man-pages/man7/sched.7.html):

- "each unit of difference in the nice values of two processes results in a factor of 1.25
  in the degree to which the scheduler favors the higher priority process".
- RLIMIT_NICE bounds how far an unprivileged process may raise priority (lower its nice
  value). Ferrite should lower agents rather than raise the Server.
- "Child processes inherit the scheduling policy and parameters across a fork(2). The
  scheduling policy and parameters are preserved across execve(2)."
- **SCHED_BATCH** is nice-based, but "the scheduler will always assume that the thread is
  CPU-intensive … a small scheduling penalty with respect to wakeup behavior". That suits
  build trees.
- **SCHED_IDLE** is "lower even than a +19 nice value" and is not wanted here.
- **Autogroup:**
  - "A new autogroup is created when a new session is created via setsid(2) … A new process
    created by fork(2) inherits its parent's autogroup membership."
  - "Under group scheduling, a thread's nice value has an effect for scheduling decisions
    only relative to other threads in the same task group … if autogrouping is enabled …
    employing setpriority(2) or nice(1) on a process has an effect only for scheduling
    relative to other processes executed in the same session".
  - Workaround: write a nice value to `/proc/<pid>/autogroup` (`echo 10 > /proc/self/autogroup`).
  - "The use of the cgroups(7) CPU controller to place processes in cgroups other than the
    root CPU cgroup overrides the effect of autogrouping."
  - The default is on (`/proc/sys/kernel/sched_autogroup_enabled` = 1) when the kernel has
    `CONFIG_SCHED_AUTOGROUP`.

From [ioprio_set(2)](https://man7.org/linux/man-pages/man2/ioprio_set.2.html): "If no I/O
scheduler has been set for a thread, then by default the I/O priority will follow the CPU
nice value". The page ties this to schedulers that support priorities; its example is CFQ.

What this means for Ferrite:

- `Server::start_detached` (`crates/ferrite-core/src/server/agent.rs` lines 60-99) runs
  `setsid` in `pre_exec`, so the Server is in its own autogroup.
- Agents spawned from it without `setsid` share that autogroup, and nice on agents works
  relative to the Server.
- If agents ever get their own session, also set the agent autogroup's nice value. Under a
  systemd unit with CPU cgroups, autogroup is overridden anyway, and the unit's own cgroup
  CPU weight decides.

## 10. LaunchAgent `ProcessType`: what Standard vs Interactive does

**`launchd.plist(5)` on this Mac:**

- Unspecified ProcessType means "the system will apply light resource limits to the job,
  throttling its CPU usage and I/O bandwidth".
- "Standard jobs are equivalent to no ProcessType being set".
- "Interactive jobs run with the same resource limitations as apps, that is to say, none …
  this key should only be used if an app's ability to be responsive depends on it, and
  cannot be made Adaptive".
- `LegacyTimers` "may have no effect if the job's ProcessType is not set to Interactive"
  (timers of launchd jobs are coalesced by default).

launchd's source is not published, so how it maps ProcessType to spawn attributes is
**UNCONFIRMED from source**. What is observable:

- `launchctl print` reports `spawn type = daemon (3)` for a job without ProcessType and
  `interactive (4)` for `Interactive`. Other jobs on this Mac show `adaptive (6)`,
  `background (5)` and `app (1)`.
- Those numbers equal the `POSIX_SPAWN_PROC_TYPE_DAEMON_*` constants shifted right by 8
  (`XNU/bsd/sys/spawn_internal.h#L366-L369`: STANDARD 0x300, INTERACTIVE 0x400,
  BACKGROUND 0x500, ADAPTIVE 0x600). The correspondence is an inference.

**What XNU does with those apptypes** (`exec_handle_spawnattr_policy`, `kern_exec.c#L2593-L2621`,
then `proc_set_task_spawnpolicy`):

- `DAEMON_STANDARD`:
  - main thread LEGACY (`task_policy.c#L2223-L2228`);
  - USER_INTERACTIVE squashed to USER_INITIATED (`#L887-L888`);
  - IO tier raised to `proc_standard_daemon_tier` = TIER1 (`#L1009-L1011`, `#L243`);
  - importance donor, not a receiver (`#L2101-L2105`).
- `DAEMON_INTERACTIVE`: the same ceiling and main thread, **no** IO tier, an importance donor
  (`#L2086-L2098`).
- The apptype is inherited by children (section 2).

**Observed** (local experiment, temporary jobs under `/tmp`, removed afterwards):

| launchd job | spawn type | main thread | after a USER_INTERACTIVE request | USER_INITIATED | child `sleep` | `PROC_FLAG_APPLICATION` |
|---|---|---|---|---|---|---|
| `launchctl submit` (no ProcessType) | `daemon (3)` | QoS UTILITY, pri 20 | 20 | 20 | 20T | 0 |
| plist with `ProcessType` `Interactive` | `interactive (4)` | DEFAULT, 31 | 37 | 37 | 31T | 0 |

Existing Standard agents on this Mac also run every thread at 20: the qmd node daemon,
`gamecontrolleragentd` and `SafariBookmarksSyncAgent`. Interactive ones such as AltTab and
CommCenter show 31/37.

The Standard behaviour matches exactly what a **UTILITY QoS clamp** produces:

- the main thread reports UTILITY, which is `task_compute_main_thread_qos`'s
  `MIN(clamp, LEGACY)`;
- no request escapes 20;
- descendants are also at 20.

A plain `DAEMON_STANDARD` apptype would give 31 and 37 instead; local experiment:
`posix_spawnattr_setprocesstype_np(0x300)` alone gave main thread 31 and USER_INTERACTIVE
37. So launchd must apply something beyond the apptype for Standard jobs. That it is
exactly `posix_spawnattr_set_qos_clamp_np(UTILITY)` is UNCONFIRMED, but the observable
behaviour is identical, and per section 3 such a clamp is inherited and cannot be lifted
by the process.

**What this means for Ferrite:**

- As a Standard LaunchAgent, the Server's pump and every agent it spawns run at ≤ 20.
  Nothing in the Server can raise them. Set `ProcessType` = `Interactive` in the
  LaunchAgent plist, which `ferrite-core/src/server/agent.rs` labels
  `com.github.josephwylie.ferrite.server`.
- Agents then need their own UTILITY clamp at spawn. Without it they inherit
  `DAEMON_INTERACTIVE` and run at 31 like the Server.

**Can a process change its own role or clamp later?**

- **Clamp:** no; spawn time only (section 3b).
- **Apptype:** no setter outside spawn. `task_set_requested_apptype` is called only from
  `task_policy_create` and `proc_set_task_spawnpolicy`, and
  `posix_spawnattr_setprocesstype_np` is exported from `libsystem_kernel` but has no SDK
  header.
- **Role:** yes, for app-typed processes.
  - `setpriority(PRIO_DARWIN_ROLE, pid, role)` requires same uid (or a privilege) and
    `PROC_FLAG_APPLICATION`, otherwise ENOTSUP (`XNU/bsd/kern/kern_resource.c#L831-L871`).
  - `PRIO_DARWIN_ROLE` is defined in `XNU/bsd/sys/resource_private.h#L144-L153` and is not
    in the SDK, so it is private SPI.
  - Mapping (`task_policy.c#L1976-L2005`): UI_FOCAL → FOREGROUND, UI → DEFAULT_APPLICATION,
    UI_NON_FOCAL → BACKGROUND_APPLICATION, NON_UI → NONUI (31 ceiling).
  - Local experiment:
    - the GUI-spawned case (`PROC_FLAG_APPLICATION` set) could set role 2 and reach 46;
    - a `DAEMON_*`-typed process got rc −1 (ENOTSUP) and stayed at 37;
    - the role is not inherited by forked children (section 2).

So the two Server launch paths differ:

| Server started by | pump with USER_INTERACTIVE | public-API ceiling | with private role SPI | agents if unclamped |
|---|---|---|---|---|
| Ferrite.app (`Command`, `setsid`) | 31 | 31 | 46/47 | 31 |
| LaunchAgent, no ProcessType | 20 | 20 (immovable) | n/a (ENOTSUP) | 20 |
| LaunchAgent, `Interactive` | 37 | 37 | n/a (ENOTSUP) | 31 |

In every row a UTILITY clamp on agents puts them at 20 or below, with P-cores still
available.
