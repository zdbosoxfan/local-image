import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'bin'))
import li_state
import li_validation

def module(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    result = importlib.util.module_from_spec(spec); loader.exec_module(result)
    return result

dispatch = module('dispatch', ROOT / 'bin/li-dispatch')
dashboard = module('dashboard', ROOT / 'dashboard/dashboard.py')

class StateTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(); self.addCleanup(self.tmp.cleanup)
        self.dev = Path(self.tmp.name)
        self.p = patch.object(li_state, 'DEV', self.dev); self.p.start(); self.addCleanup(self.p.stop)
        self.d = patch.object(dispatch, 'DEV', self.dev); self.d.start(); self.addCleanup(self.d.stop)
        self.save({'jobs': [{'name': 'a', 'status': 'running'}, {'name': 'b', 'status': 'running'}]})
    def save(self, state): (self.dev/'jobs.json').write_text(json.dumps(state))
    def read(self): return json.loads((self.dev/'jobs.json').read_text())
    def test_concurrent_updates_do_not_lose_changes(self):
        self.save({'jobs': [{'name': str(n), 'status': 'running'} for n in range(8)]})
        env = dict(os.environ, LI_DEV_DIR=str(self.dev), PYTHONPATH=str(ROOT/'bin'))
        workers = [subprocess.Popen([sys.executable, '-c', 'from li_state import update; import sys; update(sys.argv[1],status="landed")', str(n)], env=env) for n in range(8)]
        for worker in workers: self.assertEqual(worker.wait(timeout=10), 0)
        self.assertTrue(all(j['status']=='landed' for j in self.read()['jobs']))
    def test_failed_transaction_preserves_file(self):
        before = (self.dev/'jobs.json').read_bytes()
        with self.assertRaises(ValueError):
            with li_state.transaction() as state:
                state['jobs']=[]; raise ValueError('abort')
        self.assertEqual((self.dev/'jobs.json').read_bytes(),before)
    def test_hold_reconciles_orphaned_landing_without_launch(self):
        self.save({'jobs':[{'name':'a','status':'landing','land_into':'main'}, {'name':'b','status':'queued'}]})
        (self.dev/'queue-hold').touch()
        with patch.object(dispatch,'active_units',return_value=set()), patch.object(dispatch,'command') as command:
            dispatch.tick(); command.assert_not_called()
        self.assertEqual([j['status'] for j in self.read()['jobs']],['interrupted','queued'])
    def test_running_service_wins_over_stale_report(self):
        (self.dev/'codex-logs').mkdir(); (self.dev/'codex-logs/a-job.final.md').write_text('old')
        state=self.read(); dispatch.reconcile(state,{'li-codex-a-123.service'})
        self.assertEqual(state['jobs'][0]['status'],'running')
    def test_old_report_does_not_finish_new_attempt(self):
        (self.dev/'codex-logs').mkdir(); (self.dev/'codex-logs/a-job.final.md').write_text('old')
        state=self.read(); state['jobs'][0]['attempt_started']=time.time()+1
        dispatch.reconcile(state,set()); self.assertEqual(state['jobs'][0]['status'],'interrupted')
    def test_finished_report_recognized_after_exit(self):
        (self.dev/'codex-logs').mkdir(); (self.dev/'codex-logs/a-job.final.md').write_text('done')
        state=self.read(); dispatch.reconcile(state,set()); self.assertEqual(state['jobs'][0]['status'],'finished')
    def test_failed_systemd_query_preserves_state(self):
        before=self.read()
        with patch.object(dispatch,'command',return_value=subprocess.CompletedProcess([],1,'','unavailable')):
            with self.assertRaises(RuntimeError): dispatch.tick()
        self.assertEqual(self.read(),before)
    def test_land_entrypoint_refuses_before_mutating_job_or_launching(self):
        job={'name':'a','land_into':'main','status':'finished'}
        before=job.copy()
        with patch.object(dispatch,'command') as command:
            with self.assertRaisesRegex(RuntimeError, 'held'): dispatch.launch_land(job)
            command.assert_not_called()
        self.assertEqual(job,before)
    def test_finished_jobs_remain_held_even_without_hold_file(self):
        self.save({'paused':False,'jobs':[{'name':n,'status':'finished','land_into':'main'} for n in ('a','b')]})
        before=self.read()
        with patch.object(dispatch,'active_units',return_value=set()), patch.object(dispatch,'command') as cmd:
            dispatch.tick(); cmd.assert_not_called()
        self.assertEqual(self.read(),before)
    def test_untracked_land_prevents_second_land(self):
        self.save({'jobs':[{'name':'a','status':'finished','land_into':'main'}]})
        with patch.object(dispatch,'active_units',return_value={'li-land-legacy-1.service'}), patch.object(dispatch,'command') as cmd:
            dispatch.tick(); cmd.assert_not_called()
    def test_rejected_jobs_not_automatically_sent_back(self):
        self.save({'jobs':[{'name':'a','status':'rejected','land_into':'main','rework':0}]})
        with patch.object(dispatch,'active_units',return_value=set()),patch.object(dispatch,'command') as cmd:
            dispatch.tick(); cmd.assert_not_called()
    def test_builder_refuses_without_copying_tasks_or_creating_worktrees(self):
        task=self.dev/'task.md';task.write_text('retained task')
        job={'name':'a','status':'queued','task':str(task)}
        self.save({'paused':False,'jobs':[job]})
        before=self.read();files=sorted(p.name for p in self.dev.iterdir())
        with patch.object(dispatch,'command') as command:
            with self.assertRaisesRegex(RuntimeError, 'held'): dispatch.launch_builder(job)
            command.assert_not_called()
        self.assertEqual(self.read(),before)
        self.assertEqual(sorted(p.name for p in self.dev.iterdir()),files)
        self.assertEqual(task.read_text(),'retained task')
        with patch.object(dispatch,'active_units',return_value=set()), patch.object(dispatch,'command') as command:
            dispatch.tick(); command.assert_not_called()
        self.assertEqual(self.read(),before)

    def test_coordinator_owned_jobs_never_auto_reconcile_launch_or_land(self):
        self.save({'max':3,'jobs':[
            {'name':'manual-ready','status':'queued','dispatch_mode':'coordinator'},
            {'name':'manual-finished','status':'finished','land_into':'v2','dispatch_mode':'coordinator'},
            {'name':'manual-active','status':'running','dispatch_mode':'coordinator'},
        ]})
        before=self.read()
        with patch.object(dispatch,'active_units',return_value=set()), patch.object(dispatch,'command') as command:
            dispatch.tick(); command.assert_not_called()
        self.assertEqual(self.read(),before)

class GateTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(); self.addCleanup(self.tmp.cleanup); self.path=Path(self.tmp.name)
        self.env=dict(os.environ,LI_BUILD_STATE_DIR=str(self.path/'lanes'),LI_BUILD_SLOTS='2',LI_BUILD_LANES='2',LI_LANE_IDLE='0',LI_BUILD_MIN_FREE_GB='0')
        self.fake=self.path/'fake.py'
        self.fake.write_text('import os,time,json,sys\np=sys.argv[1]\nwith open(p,"a") as f: f.write(json.dumps(["start",time.time(),os.getpid()])+"\\n")\ntime.sleep(float(sys.argv[2]))\nwith open(p,"a") as f: f.write(json.dumps(["end",time.time(),os.getpid()])+"\\n")\n')
        self.processes=[]; self.addCleanup(self.cleanup)
    def cleanup(self):
        for p in self.processes:
            if p.poll() is None: p.terminate()
        for p in self.processes:
            try:p.wait(timeout=10)
            except subprocess.TimeoutExpired:p.kill();p.wait()
    def launch(self,name,duration='0.4'):
        args=[sys.executable,str(ROOT/'bin/li-rustc-gate'),sys.executable,str(self.fake),str(self.path/'events'),duration,'--out-dir',str(self.path/name/'target/debug/deps'),'--crate-name',name]
        p=subprocess.Popen(args,env=self.env); self.processes.append(p); return p
    def events(self):
        try:return [json.loads(l) for l in (self.path/'events').read_text().splitlines()]
        except FileNotFoundError:return []
    def until(self,predicate):
        deadline=time.monotonic()+6
        while not predicate():
            if time.monotonic()>deadline:self.fail('timed out')
            time.sleep(.02)
    def test_actual_compiler_concurrency_and_completion(self):
        for n in range(8):self.launch('tree'+str(n%3))
        for p in self.processes:self.assertEqual(p.wait(timeout=15),0)
        current=peak=0
        for kind,_,_ in sorted(self.events(),key=lambda x:x[1]):
            current += 1 if kind=='start' else -1;peak=max(peak,current)
        self.assertEqual(current,0);self.assertLessEqual(peak,2);self.assertEqual(len(self.events()),16)
    def test_waiting_compiler_is_not_counted_running(self):
        self.env['LI_BUILD_SLOTS']='1';self.launch('a','1.3');self.until(lambda:len(self.events())==1)
        self.launch('b','0.1');self.until(lambda:len(list((self.path/'lanes').glob('pending.*')))>0)
        with patch.object(dashboard,'LANES',str(self.path/'lanes')):
            lanes,waiting=dashboard.lanes()
        self.assertEqual(sum(len(l.get('running',[])) for l in lanes),1)
        self.assertEqual(sum(w['compilers'] for w in waiting),1)
    def test_termination_releases_slot(self):
        self.env['LI_BUILD_SLOTS']='1';p=self.launch('a','10');self.until(lambda:len(self.events())==1)
        p.terminate();self.assertEqual(p.wait(timeout=6),143)
        q=self.launch('b','0.01');self.assertEqual(q.wait(timeout=6),0)
    def test_killed_wrapper_does_not_release_live_child_slot(self):
        self.env['LI_BUILD_SLOTS']='1';p=self.launch('a','1.4');self.until(lambda:len(self.events())==1)
        p.kill();p.wait();q=self.launch('b','0.01');self.assertEqual(q.wait(timeout=6),0)
        events=self.events();self.assertEqual([e[0] for e in events],['start','end','start','end'])
    def test_three_lane_capacity_limits_four_independent_trees(self):
        self.env.update(LI_BUILD_LANES='3',LI_BUILD_SLOTS='10')
        first=[self.launch('tree'+str(n),'1.4') for n in range(3)]
        self.until(lambda:len(self.events())==3)
        fourth=self.launch('fourth','0.05')
        for child in first+[fourth]:self.assertEqual(child.wait(timeout=10),0)
        events=sorted(self.events(),key=lambda x:x[1]);current=peak=0
        for kind,_,_ in events:
            current += 1 if kind=='start' else -1;peak=max(peak,current)
        self.assertEqual(peak,3)
        self.assertEqual(current,0)
        self.assertEqual(len(events),8)
        fourth_start=next(t for kind,t,pid in events if kind=='start' and pid not in {e[2] for e in events[:3]})
        self.assertTrue(any(kind=='end' and t<=fourth_start for kind,t,_ in events))

    def test_exit_status_preserved(self):
        p=subprocess.run([sys.executable,str(ROOT/'bin/li-rustc-gate'),sys.executable,'-c','raise SystemExit(17)','--out-dir',str(self.path/'target/debug')],env=self.env)
        self.assertEqual(p.returncode,17)

class ValidationTests(unittest.TestCase):
    def test_retry_requires_actual_named_pass(self):
        for code,out in [(0,'test result: ok. 0 passed'),(101,'error: bad argument'),(1,'test foo ... ok'),(0,'test other ... ok')]:
            self.assertFalse(li_validation.retry_passed(subprocess.CompletedProcess([],code,out,''),'foo'))
        self.assertTrue(li_validation.retry_passed(subprocess.CompletedProcess([],0,'test foo ... ok\n',''),'foo'))
    def test_streaming_keeps_complete_log_and_return_code(self):
        log=io.StringIO()
        result=li_validation.run_logged([sys.executable,'-c','import sys;print("hello",flush=True);print("error",file=sys.stderr);sys.exit(3)'],str(ROOT),os.environ,log)
        self.assertEqual(result.returncode,3);self.assertIn('hello',log.getvalue());self.assertIn('error',result.stderr);self.assertNotIn('error',result.stdout)

class DashboardTests(unittest.TestCase):
    def test_truncated_request_does_not_break_dashboard(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'requests';p.write_text('{"text":"ok"}\n{"text":')
            self.assertEqual(dashboard.json_lines(str(p),10),[{'text':'ok'}])
    def test_status_shared_between_clients(self):
        with patch.object(dashboard,'_STATUS_CACHE',None),patch.object(dashboard,'_STATUS_AT',0),patch.object(dashboard,'collect_status',return_value={'ok':True}) as collect:
            self.assertEqual(dashboard.status(),dashboard.status());collect.assert_called_once()
    def test_jobs_json_is_source_of_truth(self):
        with tempfile.TemporaryDirectory() as d,patch.object(dashboard,'DEV',d):
            (Path(d)/'jobs.json').write_text(json.dumps({'jobs':[{'name':'new','status':'queued','deps':['old']}]}))
            tasks,nxt=dashboard.tasks({});self.assertEqual(tasks[0]['title'],'new');self.assertIn('old',tasks[0]['doing'])
    def test_managed_worktree_build_uses_recorded_location(self):
        with tempfile.TemporaryDirectory() as d,patch.object(dashboard,'DEV',d):
            worktree='/managed/v2-recovery/app'
            (Path(d)/'jobs.json').write_text(json.dumps({'jobs':[{'name':'recovery-worker','status':'running','worktree':worktree}]}))
            tasks,_=dashboard.tasks({123:('cargo test --offline',worktree)})
            self.assertTrue(tasks[0]['building'])

    def test_coordinator_handoff_is_display_only_and_keeps_unrelated_interruptions(self):
        with tempfile.TemporaryDirectory() as d,patch.object(dashboard,'DEV',d):
            state={'jobs':[{'name':'old','status':'interrupted'}, {'name':'other','status':'interrupted'},
                           {'name':'planned','status':'queued','reason':'saved scope'}, {'name':'live','status':'running'}]}
            path=Path(d)/'jobs.json';path.write_text(json.dumps(state));before=path.read_bytes()
            recovery={'previous_worker':'old','branch':'codex/v2-recovery','next':['Current next step']}
            rows,nxt=dashboard.tasks({},recovery);rows={row['title']:row for row in rows}
            self.assertEqual(rows['old']['status'],'handed over')
            self.assertEqual(rows['other']['status'],'interrupted')
            self.assertEqual(rows['live']['status'],'running')
            self.assertEqual(rows['planned']['status'],'held plan')
            self.assertEqual(nxt,['Current next step']);self.assertEqual(path.read_bytes(),before)
            recovery['next']=[]
            self.assertEqual(dashboard.tasks({},recovery)[1],[])
    def test_coordinator_distinguishes_recorded_result_from_live_validation(self):
        with tempfile.TemporaryDirectory() as d,patch.object(dashboard,'RECOVERY',d):
            root=Path(d);worktree=root/'candidate';evidence=worktree/'docs/wip/v2-recovery-validation';evidence.mkdir(parents=True)
            (root/'active-worker.json').write_text(json.dumps({'kind':'coordinator','worktree':str(worktree),'branch':'codex/v2-recovery'}))
            (root/'direct-validation.json').write_text(json.dumps({'label':'check','record':'/not/read/from/here'}))
            (evidence/'check.json').write_text(json.dumps({'status':'running'}))
            self.assertIn('not active',dashboard.coordinator({})['validation']['state'])
            ps={1:('python3 docs/wip/v2-recovery-validation/run.py check test --offline',str(worktree))}
            self.assertEqual(dashboard.coordinator(ps)['validation']['state'],'running')
            (evidence/'check.json').write_text(json.dumps({'status':'finished','exit':0}))
            self.assertEqual(dashboard.coordinator({})['validation']['state'],'passed')
            (evidence/'check.json').write_text(json.dumps({'status':'finished','exit':1}))
            self.assertEqual(dashboard.coordinator({})['validation']['state'],'failed')
    def test_truncated_coordinator_record_does_not_claim_running_work(self):
        with tempfile.TemporaryDirectory() as d,patch.object(dashboard,'RECOVERY',d):
            (Path(d)/'active-worker.json').write_text('{"kind":')
            self.assertIsNone(dashboard.coordinator({}))


class CheckoutTests(unittest.TestCase):
    def test_persistent_checkout_preserves_unchanged_source_mtime(self):
        with tempfile.TemporaryDirectory() as d:
            repo=Path(d)/'repo';repo.mkdir();scratch=str(Path(d)/'scratch')
            def git(*args):
                return subprocess.run(['git','-C',str(repo),*args],capture_output=True,text=True,check=True)
            git('init','-q','-b','main');git('config','user.email','test@example.com');git('config','user.name','Test')
            (repo/'keep.rs').write_text('unchanged');(repo/'change.rs').write_text('before')
            git('add','.');git('commit','-qm','initial')
            def run(args,cwd):return subprocess.run(args,cwd=cwd,capture_output=True,text=True,check=True)
            li_validation.prepare_checkout(str(repo),scratch,'main',run)
            mtime=(Path(scratch)/'keep.rs').stat().st_mtime_ns
            (repo/'change.rs').write_text('after');git('commit','-qam','update')
            li_validation.prepare_checkout(str(repo),scratch,'main',run)
            self.assertEqual((Path(scratch)/'change.rs').read_text(),'after')
            self.assertEqual((Path(scratch)/'keep.rs').stat().st_mtime_ns,mtime)
    def test_foreign_checkout_is_not_reset(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)
            def run(args,cwd):return subprocess.CompletedProcess(args,0,str(p) if '--show-toplevel' in args else 'user-branch','')
            with self.assertRaises(RuntimeError):li_validation.prepare_checkout(str(p),str(p),'main',run)

if __name__=='__main__':unittest.main()
