"""Bounded OS resource evidence for an isolated packaged-browser run.

CPU is cumulative user+kernel process time, not wall time. RSS/WorkingSet sums
include shared pages and are not unique physical-memory measurements. Sampling
runs out of process; raw costs and missed intervals are retained, not hidden.
"""
import json
import os
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path


def _mac_start(pid):
    # macOS SDK sys/proc_info.h: PROC_PIDTBSDINFO=3, struct proc_bsdinfo.
    # Unlike ps lstart, the kernel identity includes start microseconds.
    import ctypes
    class BsdInfo(ctypes.Structure):
        _fields_ = [('ids', ctypes.c_uint32 * 12), ('names', ctypes.c_char * 48),
                    ('counts', ctypes.c_uint32 * 6), ('start_sec', ctypes.c_uint64),
                    ('start_usec', ctypes.c_uint64)]
    library = ctypes.CDLL('/usr/lib/libproc.dylib')
    library.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64,
                                    ctypes.c_void_p, ctypes.c_int]
    library.proc_pidinfo.restype = ctypes.c_int
    info = BsdInfo()
    size = ctypes.sizeof(info)
    if library.proc_pidinfo(pid, 3, 0, ctypes.byref(info), size) != size:
        return None
    return f'{info.start_sec}.{info.start_usec:06d}'


def _cpu_seconds(value):
    days, separator, clock = value.partition('-')
    if not separator:
        clock, days = days, '0'
    total = 0.0
    for component in clock.split(':'):
        total = total * 60 + float(component)
    return int(days) * 86400 + total


def _processes():
    if os.name == 'nt':
        command = r'''Get-CimInstance Win32_Process | ForEach-Object {
            [pscustomobject]@{
                pid=$_.ProcessId; parent_pid=$_.ParentProcessId;
                executable=$(if ($_.ExecutablePath) {$_.ExecutablePath} else {$_.Name});
                start_identity=$(if ($_.CreationDate) {$_.CreationDate.ToUniversalTime().ToString('o')} else {$null});
                cpu_seconds=([double]$_.KernelModeTime+[double]$_.UserModeTime)/10000000;
                resident_bytes=[double]$_.WorkingSetSize;
                state=$_.ExecutionState
            }
        } | ConvertTo-Json -Compress'''
        output = subprocess.check_output(
            ['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', command],
            text=True, timeout=15,
        )
        rows = json.loads(output.lstrip('\ufeff')) or []
        if isinstance(rows, dict):
            rows = [rows]
        result = []
        for row in rows:
            if row.get('start_identity') is None:
                # Fail closed on ownership: an inaccessible identity cannot
                # safely establish that a retained PID still names our child.
                continue
            row['pid'] = int(row['pid'])
            row['parent_pid'] = int(row['parent_pid'])
            row['resident_bytes'] = int(row['resident_bytes'])
            result.append(row)
        return result
    output = subprocess.check_output(
        ['ps', '-axww', '-o', 'pid=,ppid=,pgid=,state=,lstart=,time=,rss=,comm='],
        text=True, timeout=15,
    )
    rows = []
    for line in output.splitlines():
        fields = line.strip().split(None, 11)
        if len(fields) != 12:
            continue
        pid, parent, group, state = fields[:4]
        start_identity = ' '.join(fields[4:9])
        if sys.platform == 'darwin':
            start_identity = _mac_start(int(pid))
            if start_identity is None:
                continue
        rows.append({
            'pid': int(pid), 'parent_pid': int(parent), 'process_group': int(group),
            'state': state, 'start_identity': start_identity,
            'cpu_seconds': _cpu_seconds(fields[9]),
            'resident_bytes': int(fields[10]) * 1024, 'executable': fields[11],
        })
    return rows


def _identity(row):
    return f"{row['pid']}:{row['start_identity']}:{row['executable']}"


def _role(row, parent_role):
    name = Path(row['executable']).name.casefold()
    if name in ('agent-browser', 'agent-browser.exe'):
        return 'controller'
    if name in ('claude', 'claude.exe', 'codex', 'codex.exe') or 'browser-smoke-peer' in name:
        return 'provider'
    if ('helper' in name or any(brand in name for brand in ('chromium', 'chrome', 'crashpad', 'msedge'))):
        return 'browser'
    if parent_role in ('app', 'browser') and name in ('ferrite', 'ferrite.exe'):
        return 'browser'
    return parent_role


class ResourceSampler:
    """One sampler per isolated run; roots maps role names to actual PIDs.

    start(); add_root('app', child.pid); phase('quiet'); stop(). Phases are
    caller-declared intervals: the helper does not infer that a task succeeded
    or that a page was idle. stop() retains raw evidence and returns its report.
    """
    def __init__(self, output, roots, interval=0.5):
        if interval <= 0:
            raise ValueError('Resource sampling interval must be positive')
        self.output = Path(output)
        self.interval = float(interval)
        self.roots = {int(pid): {'role': role, 'identity': None} for role, pid in roots.items()}
        self.owned = {}
        self.previous = {}
        self.samples = []
        self.events = []
        self.current_phase = 'baseline'
        self.phase_id = 0
        self.lock = threading.RLock()
        self.stopping = threading.Event()
        self.thread = None
        self.started = time.monotonic()
        self.stopped = False

    def add_root(self, role, pid):
        with self.lock:
            self.roots[int(pid)] = {'role': role, 'identity': None}
            self.events.append({'event': 'root', 'role': role, 'pid': int(pid),
                                'monotonic': time.monotonic()})
        self.sample()

    def phase(self, name):
        with self.lock:
            self.current_phase = str(name)
            self.phase_id += 1
            self.events.append({'event': 'phase', 'phase': self.current_phase,
                                'monotonic': time.monotonic()})
        return self.sample()

    def start(self):
        if self.thread is not None:
            raise RuntimeError('Resource sampler already started')
        self.phase('baseline')
        self.thread = threading.Thread(target=self._run, name='browser-resource-sampler', daemon=True)
        self.thread.start()
        return self

    def _run(self):
        while not self.stopping.wait(self.interval):
            self.sample()

    def sample(self):
        with self.lock:
            if self.stopped:
                return None
            started = time.monotonic()
            sample = {'monotonic': started, 'utc': datetime.now(timezone.utc).isoformat(),
                      'phase': self.current_phase, 'phase_id': self.phase_id, 'processes': []}
            try:
                rows = _processes()
                current = {row['pid']: row for row in rows}
                selected = {}
                # Already-owned identities remain owned after their parent
                # exits/reparents. A reused PID cannot inherit the old role.
                for row in rows:
                    identity = _identity(row)
                    if identity in self.owned:
                        selected[row['pid']] = self.owned[identity]['role']
                for pid, root in self.roots.items():
                    row = current.get(pid)
                    if row is None:
                        continue
                    identity = _identity(row)
                    if root['identity'] is None:
                        root['identity'] = identity
                    if root['identity'] == identity:
                        selected[pid] = root['role']
                while True:
                    added = False
                    for row in rows:
                        if row['pid'] not in selected and row['parent_pid'] in selected:
                            selected[row['pid']] = _role(row, selected[row['parent_pid']])
                            added = True
                    if not added:
                        break
                previous_time = self.samples[-1]['monotonic'] if self.samples else None
                for pid, role in sorted(selected.items()):
                    row = dict(current[pid])
                    identity = _identity(row)
                    old = self.previous.get(identity)
                    row.update({'identity': identity, 'role': role,
                                'first_seen_monotonic': self.owned.get(identity, {}).get('first_seen_monotonic', started)})
                    if old is not None:
                        elapsed = started - old['monotonic']
                        delta = row['cpu_seconds'] - old['cpu_seconds']
                        row['cpu_delta_seconds'] = delta if delta >= 0 else None
                        row['cpu_interval_seconds'] = elapsed
                        row['cpu_percent_one_core'] = delta / elapsed * 100 if elapsed > 0 and delta >= 0 else None
                        row['cpu_delta_crosses_phase'] = old['phase_id'] != sample['phase_id']
                    else:
                        row['cpu_delta_seconds'] = None
                        row['cpu_interval_seconds'] = None
                        row['cpu_percent_one_core'] = None
                        row['cpu_delta_crosses_phase'] = False
                    self.previous[identity] = {'monotonic': started, 'cpu_seconds': row['cpu_seconds'], 'phase_id': sample['phase_id']}
                    self.owned[identity] = {'role': role, 'pid': pid,
                                            'first_seen_monotonic': row['first_seen_monotonic'],
                                            'last_seen_monotonic': started, 'executable': row['executable']}
                    sample['processes'].append(row)
                sample['interval_since_previous_seconds'] = None if previous_time is None else started - previous_time
                sample['sampling_gap_seconds'] = None if previous_time is None else max(0, started - previous_time - self.interval)
            except Exception as error:
                sample['error'] = f'{type(error).__name__}: {error}'
            sample['sampling_duration_ms'] = (time.monotonic() - started) * 1000
            self.samples.append(sample)
            return sample

    def stop(self):
        self.stopping.set()
        if self.thread is not None:
            self.thread.join(timeout=20)
            if self.thread.is_alive():
                raise RuntimeError('OS resource sampler did not stop within its bounded query timeout')
        self.sample()
        with self.lock:
            self.stopped = True
            report = self._report()
            self.output.mkdir(parents=True, exist_ok=True)
            raw = {'platform': sys.platform, 'declared_interval_seconds': self.interval,
                   'started_monotonic': self.started, 'events': self.events,
                   'owned_identities': self.owned, 'samples': self.samples}
            (self.output / 'resources-raw.json').write_text(json.dumps(raw, indent=2), encoding='utf-8')
            (self.output / 'resources-report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
            return report

    def _report(self):
        phases = {}
        for sample in self.samples:
            phase = phases.setdefault(sample['phase'], {'samples': 0, 'errors': 0, 'roles': {}})
            phase['samples'] += 1
            phase['errors'] += int('error' in sample)
            phase.setdefault('first_sample_monotonic', sample['monotonic'])
            phase['last_sample_monotonic'] = sample['monotonic']
            segment = phase.setdefault('segments', {}).setdefault(sample['phase_id'], {
                'first_sample_monotonic': sample['monotonic'], 'last_sample_monotonic': sample['monotonic']})
            segment['last_sample_monotonic'] = sample['monotonic']
            by_role = {}
            for row in sample['processes']:
                role = by_role.setdefault(row['role'], {'resident_bytes': 0, 'cpu_delta_seconds': 0, 'process_count': 0})
                role['resident_bytes'] += row['resident_bytes']
                role['process_count'] += 1
                if not row['cpu_delta_crosses_phase']:
                    role['cpu_delta_seconds'] += row['cpu_delta_seconds'] or 0
            for role, value in by_role.items():
                summary = phase['roles'].setdefault(role, {'peak_resident_bytes_sum': 0, 'cpu_delta_seconds': 0, 'peak_process_count': 0})
                summary['peak_resident_bytes_sum'] = max(summary['peak_resident_bytes_sum'], value['resident_bytes'])
                summary['peak_process_count'] = max(summary['peak_process_count'], value['process_count'])
                summary['cpu_delta_seconds'] += value['cpu_delta_seconds']
        for phase in phases.values():
            duration = sum(segment['last_sample_monotonic'] - segment['first_sample_monotonic']
                           for segment in phase['segments'].values())
            phase['sampled_span_seconds'] = duration
            for role in phase['roles'].values():
                role['cpu_percent_one_core_over_sampled_span'] = role['cpu_delta_seconds'] / duration * 100 if duration > 0 else None
        return {'platform': sys.platform,
                'memory_metric': 'WorkingSetSize bytes' if os.name == 'nt' else 'ps RSS KiB converted to bytes',
                'cpu_metric': 'cumulative process user+kernel seconds; percentages use one logical core',
                'identity_metric': ('PID + CreationDate + executable' if os.name == 'nt' else
                                    'PID + kernel start seconds/microseconds + executable' if sys.platform == 'darwin' else
                                    'PID + ps lstart (one-second resolution) + executable'),
                'limitations': ['Memory sums include shared pages.', 'Processes born and exited between samples are not observed.',
                                'CPU deltas crossing phase boundaries are retained raw and excluded from phase CPU summaries.',
                                'First-seen CPU counters are retained raw; CPU before the first observation is excluded from delta summaries.',
                                'Processes with inaccessible start identity are excluded, not guessed from PID.',
                                'Non-macOS Unix ps start timestamps have one-second resolution; same-PID same-executable reuse within that second is not distinguishable.'],
                'declared_interval_seconds': self.interval, 'sample_count': len(self.samples),
                'sampling_cost_ms': [sample['sampling_duration_ms'] for sample in self.samples],
                'sampling_gaps_seconds': [sample.get('sampling_gap_seconds') for sample in self.samples],
                'phases': phases}
