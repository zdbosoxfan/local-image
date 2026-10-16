#!/usr/bin/env python3
"""Repeat prebuilt Rust test binaries under CPU and concurrent GPU-test load.

Build first with Rust 1.98.1, --offline and li-rustc-gate. Example:
  python3 scripts/stress-native-tests.py --binary target/debug/deps/<test-binary> \
    --runs 20 --parallel 4 --cpu-workers 16 --output target/crash-hunt/stress \
    -- tests_gpu --test-threads=16 --nocapture

Only children created here are stopped. Each run gets isolated preferences/model
directories, a timestamped log, PID/exit status and (on a native crash) coredumpctl
info and GDB backtraces. Test filters select GPU work on the real adapter; CPU
workers use bounded memory. Exit status includes crashes *after* a passing Rust
test summary. No dependency installs, GUI windows or ComfyUI jobs are needed.
"""

import argparse
import concurrent.futures
import datetime
import hashlib
import json
import os
from pathlib import Path
import resource
import re
import shutil
import signal
import subprocess
import sys
import threading
import time


def burn_cpu():
    os.nice(10)
    block = b"crash-hunt" * 65536
    while True:
        hashlib.sha256(block).digest()


def stop_child(child):
    if child.poll() is None:
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()


def resident_bytes(pid):
    """Sum RSS for this process and descendants, matching li-test-runner's cap."""
    total = 0
    pending = [pid]
    seen = set()
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        seen.add(current)
        try:
            status = Path(f"/proc/{current}/status").read_text()
            rss = re.search(r"^VmRSS:\s+(\d+) kB", status, re.MULTILINE)
            total += int(rss[1]) * 1024 if rss else 0
            for task in Path(f"/proc/{current}/task").iterdir():
                pending.extend(map(int, (task / "children").read_text().split()))
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            pass
    return total


def main():
    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f"signal {signum}")

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, action="append", required=True)
    parser.add_argument("--runs", type=int, default=10, help="runs per binary")
    parser.add_argument("--parallel", type=int, default=4)
    parser.add_argument("--cpu-workers", type=int, default=0)
    parser.add_argument("--timeout", type=float, default=600, help="hang watchdog, not a performance assertion")
    parser.add_argument("--max-rss-gib", type=float, default=12, help="per-process-tree memory watchdog")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("test_args", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.runs < 1 or args.parallel < 1 or args.cpu_workers < 0 or args.timeout <= 0 or args.max_rss_gib <= 0:
        parser.error("runs, parallel and timeout must be positive; cpu-workers must be nonnegative")
    binaries = [path.resolve(strict=True) for path in args.binary]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    test_args = args.test_args
    if test_args[:1] == ["--"]:
        test_args = test_args[1:]
    # Cargo may replace a binary while a long batch is running. Pin its bytes
    # once so every iteration and collected core uses the same executable.
    snapshots = []
    metadata = []
    for index, binary in enumerate(binaries):
        snapshot = output / f"binary-{index}"
        shutil.copy2(binary, snapshot)
        with snapshot.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        listed = subprocess.run([str(snapshot), *test_args, "--list"], capture_output=True, text=True, check=True).stdout
        if not re.search(r"[1-9][0-9]* tests?,", listed):
            parser.error(f"no tests selected in {binary}")
        metadata.append({"original": str(binary), "snapshot": str(snapshot), "sha256": digest, "list": listed})
        snapshots.append(snapshot)
    (output / "binaries.json").write_text(json.dumps(metadata, indent=2) + "\n")
    binaries = snapshots
    # The system coredump collector needs a nonzero soft limit; never alter the
    # user's system configuration or the hard limit.
    _, hard = resource.getrlimit(resource.RLIMIT_CORE)
    resource.setrlimit(resource.RLIMIT_CORE, (hard, hard))
    env = os.environ.copy()
    env.update(RUST_BACKTRACE="full", LIGHTCRAFT_GPU="1", LOCAL_IMAGE_GPU_BACKEND="vulkan",
               WGPU_BACKEND="vulkan", WGPU_POWER_PREF="high",
               WGPU_ADAPTER_NAME="NVIDIA GeForce RTX 5090", LC_REQUIRE_GPU="1",
               LOCAL_IMAGE_REQUIRE_GPU="1")
    lock = threading.Lock()
    children = set()
    cancelled = threading.Event()

    def run(binary_index, iteration):
        if cancelled.is_set():
            return {"cancelled": True}
        binary = binaries[binary_index]
        run_dir = output / f"{binary_index}-{iteration:03d}"
        run_dir.mkdir()
        run_env = env.copy()
        for name, directory in [("LOCAL_IMAGE_DATA_DIR", "local-image"), ("LOCAL_IMAGE_MODELS_DIR", "models"),
                                ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                                ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state")]:
            run_env[name] = str(run_dir / directory)
        command = [str(binary), *test_args]
        (run_dir / "tmp").mkdir()
        run_env["TMPDIR"] = str(run_dir / "tmp")
        started = time.monotonic()
        with (run_dir / "tests.log").open("w") as log:
            log.write(f"{datetime.datetime.now(datetime.timezone.utc).isoformat()} {command!r}\n")
            log.flush()
            child = subprocess.Popen(command, env=run_env, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, text=True, errors="replace", start_new_session=True)
            with lock:
                children.add(child)
            timed_out = threading.Event()
            memory_exceeded = threading.Event()
            monitor_done = threading.Event()

            def watchdog():
                while not monitor_done.wait(0.5):
                    if time.monotonic() - started >= args.timeout:
                        timed_out.set()
                    if resident_bytes(child.pid) > args.max_rss_gib * 1024**3:
                        memory_exceeded.set()
                    if timed_out.is_set() or memory_exceeded.is_set():
                        stop_child(child)
                        return

            monitor = threading.Thread(target=watchdog)
            monitor.start()
            try:
                for line in child.stdout:
                    log.write(f"{datetime.datetime.now(datetime.timezone.utc).isoformat()} {line}")
                    log.flush()
                child.wait()
            finally:
                monitor_done.set()
                stop_child(child)
                monitor.join()
                child.stdout.close()
                with lock:
                    children.discard(child)
        result = {"binary": str(binary), "iteration": iteration, "pid": child.pid, "exit": child.returncode,
                  "timeout": timed_out.is_set(), "memory_exceeded": memory_exceeded.is_set(),
                  "seconds": round(time.monotonic() - started, 3)}
        (run_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        with lock:
            print(json.dumps(result), flush=True)
        if child.returncode in (-signal.SIGABRT, -signal.SIGSEGV, -signal.SIGBUS, -signal.SIGILL):
            for name, command in [
                ("core-info.txt", ["coredumpctl", "info", str(child.pid), "--no-pager"]),
                ("core-gdb.txt", ["coredumpctl", "debug", str(child.pid),
                                  "--debugger-arguments=-batch -iex 'set debuginfod enabled off' -ex 'info threads' -ex 'thread apply all bt 40'"]),
            ]:
                with (run_dir / name).open("w") as log:
                    try:
                        subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=120, check=False)
                    except (OSError, subprocess.TimeoutExpired) as error:
                        log.write(str(error) + "\n")
        return result

    stress = []
    results = []
    try:
        for _ in range(args.cpu_workers):
            stress.append(subprocess.Popen([sys.executable, __file__, "--burn-cpu"], start_new_session=True))
        (output / "invocation.json").write_text(json.dumps({"argv": sys.argv, "cpu_pids": [p.pid for p in stress],
                                                          "load_start": os.getloadavg()}, indent=2) + "\n")
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.parallel) as pool:
            futures = [pool.submit(run, b, i) for i in range(args.runs) for b in range(len(binaries))]
            try:
                results = [future.result() for future in concurrent.futures.as_completed(futures)]
            except BaseException:
                cancelled.set()
                with lock:
                    for child in list(children):
                        stop_child(child)
                raise
    finally:
        for child in stress:
            stop_child(child)
        (output / "summary.json").write_text(json.dumps({"runs": results, "load_end": os.getloadavg()}, indent=2) + "\n")
    return int(any(result.get("exit") != 0 for result in results))


if __name__ == "__main__":
    if sys.argv[1:] == ["--burn-cpu"]:
        burn_cpu()
    else:
        sys.exit(main())
