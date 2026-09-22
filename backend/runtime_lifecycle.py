"""Lifetime of the local background service, renewed by open desktop windows."""
import time

last_heartbeat = time.monotonic()
shutdown_requested = False


def heartbeat():
    global last_heartbeat
    last_heartbeat = time.monotonic()
