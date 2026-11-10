#!/usr/bin/env python3
import multiprocessing
import time
import sys

def worker(stop_time):
    """Burns one CPU core with square roots until [stop_time]."""
    while time.time() < stop_time:
        _ = [x**0.5 for x in range(10000)]

if __name__ == '__main__':
    duration = int(sys.argv[1]) if len(sys.argv) > 1 else 15
    stop_time = time.time() + duration
    cores = multiprocessing.cpu_count()
    print(f"Spawning {cores} CPU stress workers for {duration} seconds...")
    procs = []
    for _ in range(cores):
        p = multiprocessing.Process(target=worker, args=(stop_time,))
        p.start()
        procs.append(p)
    for p in procs:
        p.join()
    print("CPU stress test complete.")
