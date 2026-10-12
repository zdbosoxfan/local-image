"""Small validation helpers shared by the merge worker and regression tests."""
import re
import subprocess
import time
import threading

def run_logged(cmd, cwd, env, log):
    log.write(time.strftime('%H:%M:%S ') + '$ ' + ' '.join(cmd) + '\n'); log.flush()
    output, errors = [], []
    log_lock = threading.Lock()
    def drain(stream, parts):
        for line in stream:
            with log_lock:
                log.write(line); log.flush()
            parts.append(line)
    with subprocess.Popen(cmd, cwd=cwd, env=env, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, errors='replace') as process:
        stderr = threading.Thread(target=drain, args=(process.stderr, errors))
        stderr.start()
        drain(process.stdout, output)
        code = process.wait()
        stderr.join()
    log.write(time.strftime('%H:%M:%S ') + f'command exited {code}\n'); log.flush()
    return subprocess.CompletedProcess(cmd, code, ''.join(output), ''.join(errors))


def retry_passed(result, name):
    # No tests selected, process crashes, compiler errors, and bad CLI syntax are not passing evidence.
    return result.returncode == 0 and re.search(r'^test ' + re.escape(name) + r' \.\.\. ok\s*$', result.stdout + result.stderr, re.M) is not None

def prepare_checkout(repo, scratch, branch, run):
    """Reuse the queue-owned checkout so untouched source mtimes remain stable across lands."""
    from pathlib import Path
    path = Path(scratch)
    if path.exists():
        top = run(['git', 'rev-parse', '--show-toplevel'], scratch).stdout.strip()
        current = run(['git', 'branch', '--show-current'], scratch).stdout.strip()
        if Path(top).resolve() != path.resolve() or current != '_land-scratch':
            raise RuntimeError('Refusing to reset a checkout not owned by the merge queue: ' + scratch)
        # This checkout is exclusively owned by the worker under .land.lock. Reset only updates changed paths.
        run(['git', 'reset', '--hard', branch], scratch)
    else:
        run(['git', 'worktree', 'add', '-q', '-B', '_land-scratch', scratch, branch], repo)
