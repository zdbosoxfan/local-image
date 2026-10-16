#!/usr/bin/env python3
"""Local Image project dashboard: http://127.0.0.1:8765

Live view of build lanes (from li-rustc-gate), Codex / DeepSeek / Claude tasks, the bug backlog, what's next, and a
request box. Requests are appended to requests.jsonl (the coordinator watches it). Local only (binds 127.0.0.1).
"""
import threading
from datetime import datetime
import fcntl, html, json, os, re, subprocess, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'bin'))
from li_config import absolute, build_state_dir, dev_dir, recovery_dir, repository, validate_state_dir

REPO = str(repository(required=False) or '')
DEV = str(dev_dir())
LANES = str(build_state_dir())
RECOVERY = str(recovery_dir() or '')
RUNTIME = os.path.join(DEV, 'dashboard')
REQ = os.path.join(RUNTIME, 'requests.jsonl')
BUGS = os.path.join(RUNTIME, 'bugs.jsonl')
HIST = os.path.join(RUNTIME, 'lane-history.json')


def read(p, default=""):
    try:
        with open(p, encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError:
        return default


def ago(t):
    s = int(time.time() - t)
    return f"{s}s" if s < 90 else f"{s // 60}m" if s < 5400 else f"{s // 3600}h"


def short(path):
    return (path.replace(REPO, "main") if REPO else path).replace("/target", "")


def procs():
    out = {}
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            cmd = read(f"/proc/{pid}/cmdline").replace("\0", " ")
            cwd = os.readlink(f"/proc/{pid}/cwd")
        except OSError:
            continue
        out[int(pid)] = (cmd, cwd)
    return out


def marker_live(path):
    try:
        with open(path) as stream:
            try:
                fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
                return False
            except BlockingIOError:
                return True
    except OSError:
        return False


def lanes():
    # Monitoring never deletes scheduler files or writes estimated history.
    res, waiting = [], {}
    if not os.path.isdir(LANES): return res, []
    now = time.time()
    for name in os.listdir(LANES):
        path = os.path.join(LANES, name)
        if name.startswith('pending.') and marker_live(path):
            parts = read(path).splitlines()
            if len(parts) >= 2:
                try: since = float(parts[1])
                except ValueError: continue
                previous = waiting.get(parts[0], (since, 0))
                waiting[parts[0]] = (min(since, previous[0]), previous[1] + 1)
        if not name.isdigit(): continue
        key = read(path + '/key').splitlines()
        running = []
        try: files = os.listdir(path)
        except OSError: continue
        for filename in files:
            if not filename.startswith('run.') or not marker_live(path + '/' + filename): continue
            parts = read(path + '/' + filename).split()
            if len(parts) == 2:
                try: running.append((parts[0], max(0, int(now-float(parts[1])))))
                except ValueError: pass
        if not key or not running:
            res.append({'lane': name, 'idle': True}); continue
        done = [line.split() for line in read(path + '/done').splitlines() if line.strip()]
        res.append({'lane': name, 'build': short(key[0]), 'since': ago(float(key[1])) if len(key)>1 else '?',
                    'running': sorted(running, key=lambda row: -row[1]), 'done': len(done), 'pct': -1,
                    'est': 0, 'idle': False, 'slowest': []})
    return sorted(res, key=lambda row: int(row['lane'])), [
        {'build': short(key), 'waiting': ago(since), 'compilers': count}
        for key, (since, count) in sorted(waiting.items(), key=lambda row: row[1][0])]


def json_file(path, fallback):
    try: return json.loads(read(path))
    except (ValueError, OSError): return fallback


def json_lines(path, limit):
    rows = []
    for line in read(path).splitlines():
        try: rows.append(json.loads(line))
        except ValueError: continue
    return rows[-limit:]


def last_line(path):
    try:
        with open(path, "rb") as f:
            f.seek(0, 2)
            f.seek(max(0, f.tell() - 6000))
            lines = [l.strip() for l in f.read().decode("utf-8", "replace").splitlines() if l.strip()]
    except OSError:
        return ""
    for l in reversed(lines):
        if not l.startswith(("tokens used", "---", "```")) and not re.fullmatch(r"[\d,]+", l):
            return l[:220]
    return ""


def coordinator(ps):
    if not RECOVERY: return None
    active = json_file(os.path.join(RECOVERY, 'active-worker.json'), {})
    if not isinstance(active, dict) or active.get('kind') != 'coordinator': return None
    worktree = active.get('worktree', '')
    if not isinstance(worktree, str): return None
    try: updated = ago(datetime.fromisoformat(active.get('updated_at', '')).timestamp())
    except (ValueError, TypeError): updated = 'unknown'
    handle = json_file(os.path.join(RECOVERY, 'direct-validation.json'), {})
    label = handle.get('label', '') if isinstance(handle, dict) else ''
    validation = {'label': label, 'state': 'unavailable'}
    if isinstance(label, str) and re.fullmatch(r'[a-z0-9-]+', label) and worktree:
        # Read only the expected evidence path under the state dir, not an arbitrary record filename.
        record = json_file(os.path.join(RECOVERY, 'validation', label + '.json'), {})
        if record.get('status') == 'finished':
            validation['state'] = 'passed' if record.get('exit') == 0 else 'failed'
        elif record.get('status') == 'running':
            live = any(cwd == worktree and 'run.py ' + label + ' ' in cmd for cmd, cwd in ps.values())
            validation['state'] = 'running' if live else 'unfinished record; worker not active'
    findings = json_file(os.path.join(RECOVERY, 'coordinator-findings.json'), {}).get('findings', {})
    previous = active.get('previous_worker') or {}
    owner = active.get('parallel_owner') or {}
    return {'name': active.get('name', 'Recovery coordinator'), 'branch': active.get('branch', ''),
            'worktree': worktree, 'updated': updated, 'activity': active.get('activity', 'See the coordinator progress report.'),
            'owner': owner.get('task', ''), 'owner_status': owner.get('status', ''), 'scope': owner.get('scope', []),
            'previous_worker': previous.get('name') if previous.get('status') == 'stopped' else None,
            'validation': validation, 'committed_findings': sum(bool(v.get('commit')) for v in findings.values()),
            'next': active.get('next', []), 'release_status': active.get('release_status', 'final Linux release still pending')}


def tasks(ps, recovery=None):
    state = json_file(os.path.join(DEV, 'jobs.json'), {'jobs': []})
    jobs = state.get('jobs', [])
    landed = {job['name'] for job in jobs if job['status'] == 'landed'}
    logs = {}
    directory = os.path.join(DEV, 'codex-logs')
    try:
        for entry in os.scandir(directory):
            if entry.name.endswith('.log'):
                try: logs[entry.name] = (entry.path, entry.stat().st_mtime)
                except OSError: pass
    except OSError: pass
    out = []
    for job in jobs:
        name = job['name']
        candidates = [value for key, value in logs.items() if key.startswith(name + '-')]
        logfile, modified = max(candidates, key=lambda value: value[1], default=('', 0))
        path = job.get('worktree') or ''
        dependencies = [dep for dep in job.get('deps', []) if dep not in landed]
        reason = job.get('reason', '')
        if job['status'] == 'queued' and dependencies: reason = 'Waiting for: ' + ', '.join(dependencies)
        if job['status'] == 'landing':
            logfile = os.path.join(DEV, 'land-logs', name + '.log')
            try: modified = os.path.getmtime(logfile)
            except OSError: modified = 0
        display_status = job['status']
        if recovery and name == recovery.get('previous_worker') and display_status in ('interrupted', 'died', 'hold', 'finished'):
            display_status = 'handed over'
            reason = 'Work preserved; the recovery coordinator continues in ' + recovery.get('branch', '')
        elif recovery and display_status == 'queued':
            display_status = 'held plan'
            reason = 'Managed by the recovery coordinator. Original scope: ' + job.get('reason', reason)
        out.append({'who': job.get('model', 'Codex'), 'title': name, 'status': display_status,
                    'doing': reason, 'alive': job['status'] in ('running', 'landing'),
                    'building': bool(path) and any(cwd == path and 'cargo' in cmd for cmd, cwd in ps.values()),
                    'activity': last_line(logfile) if logfile else '', 'log_age': ago(modified) if modified else '',
                    'changed': ''})
    next_jobs = [j['name'] + (': ' + j.get('reason', '') if j.get('reason') else '') for j in jobs if j['status'] == 'queued']
    order = {'interrupted': 0, 'died': 0, 'launch_failed': 0, 'rejected': 0,
             'running': 1, 'landing': 1, 'queued': 2, 'integrated': 3, 'hold': 3, 'finished': 4, 'validated': 4, 'landed': 5}
    out.sort(key=lambda row: order.get(row['status'], 3))
    if recovery is not None: next_jobs = recovery.get('next', [])
    return out, next_jobs


def deepseek(ps):
    agents = []
    for cmd, cwd in ps.values():
        if "ds_agent.py" in cmd:
            m = re.search(r"worktrees/(\S+)", cmd)
            agents.append(m.group(1) if m else short(cwd))
    log = read(DEV + "/ds-jobs/dispatch.log").splitlines()[-8:]
    batch = read(DEV + "/ds-jobs/batch-status.txt").strip()
    return {"running": sorted(set(agents)), "recent": log, "batch": batch}


def backlog():
    if not REPO: return []
    s = read(REPO + "/docs/BACKLOG.md")
    m = s.split("## Known issues", 1)
    items = re.findall(r"^- (.+?)(?=\n- |\n\n|\Z)", m[1] if len(m) > 1 else "", re.S | re.M)
    return [re.sub(r"\s+", " ", i)[:240] for i in items]


def machine():
    mem = dict(re.findall(r"(\w+):\s+(\d+)", read("/proc/meminfo")))
    load = read("/proc/loadavg").split()[:3]
    try:
        st = os.statvfs(REPO or DEV)
        disk = st.f_bavail * st.f_frsize / 1e12
    except OSError:
        disk = 0
    return {"free_gb": int(mem.get("MemAvailable", 0)) // 1048576, "total_gb": int(mem.get("MemTotal", 0)) // 1048576,
            "load": " ".join(load), "disk_tb": round(disk, 2)}


def collect_status():
    ps = procs()
    ln, waits = lanes()
    recovery = coordinator(ps)
    t, nxt = tasks(ps, recovery)
    reqs = json_lines(REQ, 10)
    bugs = json_lines(BUGS, 15)
    state = json_file(os.path.join(DEV, 'jobs.json'), {})
    return {"time": time.strftime("%H:%M:%S"), "machine": machine(), "lanes": ln, "queue": waits, "tasks": t,
            "coordinator": recovery, "deepseek": deepseek(ps), "backlog": backlog(), "next": nxt, "requests": reqs, "bugs": bugs,
            "scheduler": {"paused": True, "policy": "coordinator-only; automatic dispatch and landing held",
                          "hold_file": os.path.exists(os.path.join(DEV, 'queue-hold')),
                          "max_builders": state.get('max', 3),
                          "build_lanes": state.get('build_lanes', 3),
                          "compiler_slots": state.get('compiler_slots', 10),
                          "model_slots": state.get('max_model_subagents', 8),
                          "active_workers": sum(j.get('status') in ('running', 'starting', 'landing') for j in state.get('jobs', [])),
                          "interrupted": sum(j.get('status') in ('interrupted', 'died', 'launch_failed') for j in t)}}


_STATUS_LOCK = threading.Lock()
_STATUS_CACHE = None
_STATUS_AT = 0

def status():
    global _STATUS_CACHE, _STATUS_AT
    if _STATUS_CACHE is not None and time.monotonic() - _STATUS_AT < 3:
        return _STATUS_CACHE
    if not _STATUS_LOCK.acquire(blocking=False):
        if _STATUS_CACHE is not None: return _STATUS_CACHE
        _STATUS_LOCK.acquire()
    try:
        if _STATUS_CACHE is None or time.monotonic() - _STATUS_AT >= 3:
            _STATUS_CACHE = collect_status()
            _STATUS_AT = time.monotonic()
        return _STATUS_CACHE
    finally:
        _STATUS_LOCK.release()


PAGE = r"""<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Local Image — build board</title><style>
:root{--bg:#16171a;--card:#1f2125;--line:#2c2f35;--text:#e6e6e6;--dim:#9aa0a6;--acc:#7aa2ff;--ok:#5ccf8a;--warn:#e8b14a;--bad:#ef6b6b}
@media (prefers-color-scheme: light){:root{--bg:#f4f5f7;--card:#fff;--line:#dfe2e6;--text:#1d1f22;--dim:#5f6670;--acc:#2f63e0;--ok:#1b9a55;--warn:#a96d00;--bad:#c83a3a}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font:13px/1.45 system-ui,sans-serif}
header{display:flex;gap:16px;align-items:baseline;padding:10px 16px;border-bottom:1px solid var(--line);position:sticky;top:0;background:var(--bg)}
h1{font-size:15px;margin:0}header .m{color:var(--dim)}
main{display:grid;grid-template-columns:repeat(auto-fit,minmax(360px,1fr));gap:12px;padding:12px 16px}
section{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:10px 12px;min-width:0}
h2{font-size:12px;text-transform:uppercase;letter-spacing:.06em;color:var(--dim);margin:0 0 8px}
.row{padding:6px 0;border-top:1px solid var(--line)}.row:first-of-type{border-top:0}
.t{font-weight:600}.d{color:var(--dim);font-size:12px;overflow-wrap:anywhere}
.bar{height:6px;background:var(--line);border-radius:3px;overflow:hidden;margin:4px 0}.bar i{display:block;height:100%;background:var(--acc)}
.pill{display:inline-block;font-size:11px;padding:0 6px;border-radius:9px;border:1px solid var(--line);color:var(--dim);margin-left:4px}
.on{color:var(--ok);border-color:var(--ok)}.wait{color:var(--warn);border-color:var(--warn)}.off{color:var(--bad);border-color:var(--bad)}
code{font:11.5px ui-monospace,monospace}ul{margin:0;padding-left:18px}li{margin:3px 0}
textarea{width:100%;min-height:70px;background:var(--bg);color:var(--text);border:1px solid var(--line);border-radius:6px;padding:6px;font:inherit}
button{margin-top:6px;background:var(--acc);color:#fff;border:0;border-radius:6px;padding:5px 12px;cursor:pointer}
.wide{grid-column:1/-1}
.tab{background:var(--bg);color:var(--dim);border:1px solid var(--line);border-radius:6px;padding:4px 8px;cursor:pointer}.tab.on{color:var(--text);border-color:var(--acc)}
</style></head><body>
<header><h1>Local Image — build board</h1><span class="m" id="mach"></span><span class="m" id="time"></span></header>
<main>
<section class="wide"><h2>Job queue</h2><div id="scheduler"></div></section>
<section class="wide" id="recovery-section" style="display:none"><h2>V2 recovery</h2><div id="recovery"></div></section>
<section><h2>Compiling now (build lanes)</h2><div id="lanes"></div><h2 style="margin-top:10px">Waiting for a lane</h2><div id="queue"></div></section>
<section><h2>Requests for review</h2><p class="d">Requests are saved for the coordinator to review. Submitting here does not start work or change the job queue.</p>
<div style="display:flex;gap:4px;margin-bottom:8px"><button class="tab" data-k="feature" onclick="tab('feature')">Request a Feature</button><button class="tab" data-k="bug" onclick="tab('bug')">Submit a Bug</button><button class="tab" data-k="direction" onclick="tab('direction')">Change Direction</button></div>
<div id="f-bug" style="display:none"><input id="bt" placeholder="What's wrong? (short title)" style="width:100%;margin-bottom:6px;background:var(--bg);color:var(--text);border:1px solid var(--line);border-radius:6px;padding:6px;font:inherit">
<div style="display:flex;gap:6px;margin-bottom:6px"><select id="bw" style="flex:1;background:var(--bg);color:var(--text);border:1px solid var(--line);border-radius:6px;padding:6px;font:inherit"><option>Library</option><option>Develop</option><option>Compositing</option><option>AI / Generate</option><option>Smart Sort / People</option><option>Install / start-up</option><option>Other</option></select>
<select id="bs" style="flex:1;background:var(--bg);color:var(--text);border:1px solid var(--line);border-radius:6px;padding:6px;font:inherit"><option value="annoying">Annoying</option><option value="broken">Broken</option><option value="crash">Crash / data loss</option></select></div></div>
<textarea id="req" placeholder=""></textarea>
<button id="go" onclick="send()">Send</button><div id="reqs" style="margin-top:8px"></div></section>
<section class="wide"><p class="d">Coordinator-only workflow: legacy automatic launch, landing and rework are held.</p><h2>Worker history and held scopes</h2><p class="d">Older worker reports are retained below. Current recovery progress is shown above.</p><div id="tasks"></div></section>
<section><h2>DeepSeek</h2><div id="ds"></div></section>
<section><h2>Coming up next</h2><ol id="next"></ol></section>
<section><h2>Bug backlog (final bug-fixing run)</h2><ul id="backlog"></ul></section>
</main><script>
const e=s=>String(s).replace(/[&<>"]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
let loading=false; async function load(){if(loading)return;loading=true;try{const response=await fetch('/api/status');if(!response.ok)throw new Error('status unavailable');const s=await response.json();
document.getElementById('time').textContent='updated '+s.time;
document.getElementById('scheduler').textContent=(s.scheduler.paused?'Automatic queue held. ':'Automatic queue active. ')+s.scheduler.active_workers+' worker(s) active · '+s.scheduler.max_builders+' coding worker limit · '+s.scheduler.build_lanes+' build lanes / '+s.scheduler.compiler_slots+' compiler slots · '+s.scheduler.model_slots+' configured model slots · '+s.scheduler.interrupted+' unresolved interruption(s)';
const c=s.coordinator;document.getElementById('recovery-section').style.display=c?'':'none';
if(c)document.getElementById('recovery').innerHTML=`<div class="row"><span class="t">${e(c.name)}</span><span class="pill">${e(c.branch)}</span><div class="d">Last reported ${e(c.updated)} ago: ${e(c.activity)}</div><div class="d">${e(c.owner)} · ${e(c.owner_status)} · ${e(c.scope.join(', '))}</div><div class="d">Latest validation: <b>${e(c.validation.label)}</b> — ${e(c.validation.state)}</div><div class="d">${c.committed_findings} findings recorded with commits · ${e(c.release_status)} · one Cargo command at a time</div><div class="d">Candidate: <code>${e(c.worktree)}</code></div></div>`;
const m=s.machine;document.getElementById('mach').textContent=`RAM free ${m.free_gb}/${m.total_gb} GB · load ${m.load} · disk free ${m.disk_tb} TB`;
document.getElementById('lanes').innerHTML=s.lanes.map(l=>l.idle&&!l.build?`<div class="row"><span class="t">Lane ${l.lane}</span> <span class="pill">free</span></div>`:
`<div class="row"><span class="t">Lane ${l.lane}: ${e(l.build)}</span><span class="pill ${l.idle?'':'on'}">${l.idle?'finishing / tests':'compiling'}</span>
${l.pct>=0?`<div class="bar"><i style="width:${l.pct}%"></i></div>`:''}<div class="d">${l.done} compiler invocations completed · ${l.pct>=0?`~${l.pct}% of ~${l.est}`:'elapsed time shown; no guessed percentage'} · this build: ${l.since}</div>
<div class="d">${l.running.map(r=>`<code>${e(r[0])}</code> ${r[1]}s`).join(' · ')||''}</div>
${l.slowest.length?`<div class="d">slowest: ${e(l.slowest.join(', '))}</div>`:''}</div>`).join('')||'<div class="d">No builds yet.</div>';
document.getElementById('queue').innerHTML=s.queue.map((q,i)=>`<div class="row">${i+1}. ${e(q.build)} <span class="pill wait">waiting ${q.waiting}</span></div>`).join('')||'<div class="d">Nothing waiting.</div>';
document.getElementById('tasks').innerHTML=s.tasks.map(t=>`<div class="row"><span class="t">${e(t.who)} · ${e(t.title)}</span>
<span class="pill ${['running','landing','landed','finished','validated'].includes(t.status)?'on':['queued','hold','integrated','held plan','handed over'].includes(t.status)?'wait':'off'}">${e(t.status)}</span>${t.building?'<span class="pill wait">building</span>':''}
${t.changed!==''?`<span class="pill">${t.changed} files changed</span>`:''}
<div class="d">${e(t.doing)}</div>${t.activity?`<div class="d">worker report (${e(t.log_age)} ago): <code>${e(t.activity)}</code></div>`:''}</div>`).join('');
const d=s.deepseek;document.getElementById('ds').innerHTML=`<div class="d">${d.running.length} job(s) running${d.batch?' · '+e(d.batch):''}</div>`+
(d.running.length?`<ul>${d.running.map(r=>`<li><code>${e(r)}</code></li>`).join('')}</ul>`:'')+
(d.recent.length?`<div class="d" style="margin-top:6px">${d.recent.map(e).join('<br>')}</div>`:'');
document.getElementById('next').innerHTML=s.next.map(n=>`<li>${e(n)}</li>`).join('');
document.getElementById('backlog').innerHTML=s.backlog.map(b=>`<li class="d">${e(b)}</li>`).join('');
document.getElementById('reqs').innerHTML=s.requests.slice().reverse().map(r=>`<div class="row d"><span class="pill ${r.status==='done'?'on':'wait'}">${e(r.status||'queued')}</span> <span class="pill">${e({feature:'Feature',bug:'Bug',direction:'Direction'}[r.kind]||(String(r.text).startsWith('BUG ')?'Bug':'Request'))}</span> ${e(r.time)} — ${e(r.text)}</div>`).join('');
}catch(err){document.getElementById('time').textContent='dashboard offline'}finally{loading=false}}
let K='feature';const PH={feature:'Describe the feature or change you want (Ctrl+Enter to send)',bug:'Steps to reproduce, what you expected, what happened (Ctrl+Enter to send)',direction:'A change of plan or priority: stop, pause, start, reorder… (Ctrl+Enter to send)'};
const BTN={feature:'Request feature',bug:'Submit bug',direction:'Change direction'};
function tab(k){K=k;document.querySelectorAll('.tab').forEach(b=>b.classList.toggle('on',b.dataset.k===k));document.getElementById('f-bug').style.display=k==='bug'?'':'none';document.getElementById('req').placeholder=PH[k];document.getElementById('go').textContent=BTN[k]}
async function send(){const t=document.getElementById('req');
 if(K==='bug'){const bt=document.getElementById('bt');if(!bt.value.trim()&&!t.value.trim())return;
  await fetch('/api/bug',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({title:bt.value,where:document.getElementById('bw').value,severity:document.getElementById('bs').value,details:t.value})});bt.value='';}
 else{if(!t.value.trim())return;await fetch('/api/request',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({text:t.value,kind:K})});}
 t.value='';load()}
tab('feature');
document.getElementById('req').addEventListener('keydown',ev=>{if(ev.key==='Enter'&&ev.ctrlKey)send()});
load();setInterval(load,3000);
</script></body></html>"""


def append_record(path, value):
    path = absolute(path, 'dashboard record')
    validate_state_dir(path)
    fd = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'a', encoding='utf-8') as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        stream.write(json.dumps(value) + '\n')
        stream.flush()
        os.fsync(stream.fileno())


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, code, body, ctype):
        b = body.encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def do_GET(self):
        if self.path.startswith("/api/status"):
            try:
                self._send(200, json.dumps(status()), "application/json")
            except Exception as ex:  # keep the page alive
                self._send(500, json.dumps({"error": str(ex)}), "application/json")
        else:
            self._send(200, PAGE, "text/html; charset=utf-8")

    def do_POST(self):
        if self.path == "/api/request":
            n = int(self.headers.get("Content-Length", "0") or 0)
            try:
                body = json.loads(self.rfile.read(min(n, 20000)) or b"{}")
                text, kind = str(body.get("text", "")).strip(), str(body.get("kind", "feature"))[:20]
            except ValueError:
                text, kind = "", "feature"
            if text:
                append_record(REQ, {"time": time.strftime("%Y-%m-%d %H:%M"), "kind": kind, "text": text[:4000], "status": "queued"})
            self._send(200, "{}", "application/json")
        elif self.path == "/api/bug":
            n = int(self.headers.get("Content-Length", "0") or 0)
            try:
                b = json.loads(self.rfile.read(min(n, 40000)) or b"{}")
            except ValueError:
                b = {}
            title, details = str(b.get("title", "")).strip()[:300], str(b.get("details", "")).strip()[:8000]
            if title or details:
                now = time.strftime("%Y-%m-%d %H:%M")
                rec = {"time": now, "title": title or details[:80], "where": str(b.get("where", ""))[:60],
                       "severity": str(b.get("severity", ""))[:20], "details": details, "status": "reported"}
                append_record(BUGS, rec)
                # Also a request line, so the coordinator's watcher is notified at once.
                append_record(REQ, {"time": now, "kind": "bug", "text": f"BUG [{rec['where']} / {rec['severity']}] {rec['title']}" + (f" — {details}" if details else ""), "status": "queued"})
            self._send(200, "{}", "application/json")
        else:
            self._send(404, "{}", "application/json")


if __name__ == "__main__":
    REPO = str(repository())
    validate_state_dir(DEV, Path(REPO))
    runtime = validate_state_dir(RUNTIME, Path(REPO))
    runtime.mkdir(parents=True, exist_ok=True)
    ThreadingHTTPServer(("127.0.0.1", 8765), H).serve_forever()
