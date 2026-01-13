#!/usr/bin/env python3
"""
Real-world performance benchmarks for walsync

Measures what users care about:
1. Sync latency (commit → Tigris)
2. Restore performance (Tigris → usable DB)
3. Multi-DB throughput
4. Network failure recovery

Requirements:
- walsync binary (cargo build --release)
- Tigris credentials in .env
- pip install psutil boto3

Usage:
    python bench/realworld.py                    # Run all benchmarks
    python bench/realworld.py --test sync        # Only sync latency
    python bench/realworld.py --test restore     # Only restore
    python bench/realworld.py --json             # JSON output
"""

import argparse
import json
import os
import sys
import time
import sqlite3
import subprocess
import tempfile
import shutil
from pathlib import Path
from dataclasses import dataclass, asdict, field
from typing import Optional, List
import threading
import statistics

try:
    import psutil
except ImportError:
    print("Please install psutil: pip install psutil")
    sys.exit(1)

try:
    import boto3
except ImportError:
    print("Please install boto3: pip install boto3")
    sys.exit(1)


@dataclass
class SyncLatencyResult:
    """Results for sync latency benchmark."""
    name: str = "sync_latency"
    num_commits: int = 0
    p50_ms: float = 0.0
    p95_ms: float = 0.0
    p99_ms: float = 0.0
    mean_ms: float = 0.0
    max_ms: float = 0.0
    samples: List[float] = field(default_factory=list)


@dataclass
class RestoreResult:
    """Results for restore benchmark."""
    name: str = "restore"
    db_size_mb: float = 0.0
    download_time_ms: float = 0.0
    restore_time_ms: float = 0.0
    total_time_ms: float = 0.0
    throughput_mbps: float = 0.0


@dataclass
class MultiDbResult:
    """Results for multi-DB throughput benchmark."""
    name: str = "multi_db_throughput"
    num_databases: int = 0
    writes_per_sec: float = 0.0
    sync_lag_p95_ms: float = 0.0
    cpu_percent: float = 0.0
    memory_mb: float = 0.0


@dataclass
class NetworkRecoveryResult:
    """Results for network recovery benchmark."""
    name: str = "network_recovery"
    outage_duration_sec: int = 0
    catchup_time_ms: float = 0.0
    writes_during_outage: int = 0
    writes_lost: int = 0


def create_s3_client(endpoint: Optional[str] = None):
    """Create S3 client with optional endpoint."""
    session = boto3.Session(
        aws_access_key_id=os.environ.get("AWS_ACCESS_KEY_ID"),
        aws_secret_access_key=os.environ.get("AWS_SECRET_ACCESS_KEY"),
        region_name=os.environ.get("AWS_REGION", "auto"),
    )

    config_kwargs = {}
    if endpoint:
        config_kwargs["endpoint_url"] = endpoint

    return session.client("s3", **config_kwargs)


def benchmark_sync_latency(
    bucket: str,
    endpoint: Optional[str] = None,
    num_commits: int = 50,
    db_size_kb: int = 100,
) -> SyncLatencyResult:
    """
    Measure latency from SQLite commit to S3 object appearing.

    Strategy:
    1. Create test database
    2. Start walsync watch in background
    3. For each commit:
       - Record time before commit
       - Execute INSERT + COMMIT
       - Poll S3 until new WAL segment appears
       - Record latency
    4. Return p50/p95/p99/max
    """
    print(f"\nBenchmarking sync latency ({num_commits} commits)...")

    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir = Path(tmpdir)
        db_path = tmpdir / "test.db"

        # Create initial database
        conn = sqlite3.connect(str(db_path))
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("CREATE TABLE data (id INTEGER PRIMARY KEY, value BLOB, ts REAL)")

        # Insert initial data
        chunk = b"x" * 1024
        for i in range(db_size_kb):
            conn.execute("INSERT INTO data (value, ts) VALUES (?, ?)", (chunk, time.time()))
        conn.commit()
        conn.close()

        # Start walsync
        walsync_bin = Path(__file__).parent.parent / "target" / "release" / "walsync"
        cmd = [str(walsync_bin), "watch", str(db_path), "-b", bucket]
        if endpoint:
            cmd += ["--endpoint", endpoint]

        proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        time.sleep(2)  # Let walsync initialize

        # Setup S3 client to poll for new objects
        s3_client = create_s3_client(endpoint)
        bucket_name = bucket.replace("s3://", "").split("/")[0]
        prefix = "/".join(bucket.replace("s3://", "").split("/")[1:])
        if prefix:
            prefix += "/"
        prefix += "test/wal/"

        latencies = []

        # Measure latencies
        conn = sqlite3.connect(str(db_path))
        for i in range(num_commits):
            # Get current S3 object list
            before_objs = set()
            try:
                resp = s3_client.list_objects_v2(Bucket=bucket_name, Prefix=prefix)
                before_objs = {obj["Key"] for obj in resp.get("Contents", [])}
            except:
                pass

            # Commit
            start = time.time()
            conn.execute("INSERT INTO data (value, ts) VALUES (?, ?)", (chunk, start))
            conn.commit()

            # Poll S3 until new object appears
            max_wait = 10  # 10 seconds max
            poll_start = time.time()
            found = False

            while time.time() - poll_start < max_wait:
                try:
                    resp = s3_client.list_objects_v2(Bucket=bucket_name, Prefix=prefix)
                    after_objs = {obj["Key"] for obj in resp.get("Contents", [])}

                    if len(after_objs) > len(before_objs):
                        latency = (time.time() - start) * 1000
                        latencies.append(latency)
                        found = True
                        break
                except:
                    pass

                time.sleep(0.05)  # Poll every 50ms

            if not found:
                print(f"  Warning: commit {i} timed out waiting for S3")

        conn.close()
        proc.terminate()
        proc.wait()

        if not latencies:
            return SyncLatencyResult(num_commits=num_commits)

        latencies.sort()
        return SyncLatencyResult(
            num_commits=len(latencies),
            p50_ms=latencies[int(len(latencies) * 0.5)],
            p95_ms=latencies[int(len(latencies) * 0.95)],
            p99_ms=latencies[int(len(latencies) * 0.99)] if len(latencies) >= 100 else latencies[-1],
            mean_ms=statistics.mean(latencies),
            max_ms=max(latencies),
            samples=latencies[:10],  # Only keep first 10 for output
        )


def benchmark_restore(
    bucket: str,
    endpoint: Optional[str] = None,
    db_sizes_mb: List[int] = [1, 10, 100],
) -> List[RestoreResult]:
    """
    Measure restore performance for different database sizes.

    Strategy:
    1. Create test database of given size
    2. Snapshot to S3
    3. Measure time to restore
    4. Calculate throughput (MB/s)
    """
    print(f"\nBenchmarking restore performance (sizes: {db_sizes_mb}MB)...")
    results = []

    walsync_bin = Path(__file__).parent.parent / "target" / "release" / "walsync"

    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir = Path(tmpdir)

        for size_mb in db_sizes_mb:
            print(f"  Testing {size_mb}MB database...")

            db_path = tmpdir / f"test_{size_mb}mb.db"

            # Create database
            conn = sqlite3.connect(str(db_path))
            conn.execute("PRAGMA journal_mode=WAL")
            conn.execute("CREATE TABLE data (id INTEGER PRIMARY KEY, value BLOB)")

            # Insert data to reach target size
            chunk = b"x" * (1024 * 10)  # 10KB chunks
            num_rows = (size_mb * 1024) // 10

            for i in range(num_rows):
                conn.execute("INSERT INTO data (value) VALUES (?)", (chunk,))
                if i % 100 == 0:
                    conn.commit()

            conn.commit()
            conn.close()

            actual_size_mb = db_path.stat().st_size / (1024 * 1024)

            # Snapshot to S3
            cmd = [str(walsync_bin), "snapshot", str(db_path), "-b", bucket]
            if endpoint:
                cmd += ["--endpoint", endpoint]

            result = subprocess.run(cmd, capture_output=True, text=True)
            if result.returncode != 0:
                print(f"    Snapshot failed: {result.stderr}")
                print(f"    Command: {' '.join(cmd)}")
                raise RuntimeError(f"Snapshot failed with exit code {result.returncode}")

            # Measure restore time
            restore_path = tmpdir / f"restored_{size_mb}mb.db"

            cmd = [str(walsync_bin), "restore", f"test_{size_mb}mb", "-o", str(restore_path), "-b", bucket]
            if endpoint:
                cmd += ["--endpoint", endpoint]

            start = time.time()
            subprocess.run(cmd, check=True, capture_output=True)
            restore_time = (time.time() - start) * 1000

            throughput = (actual_size_mb / (restore_time / 1000)) if restore_time > 0 else 0

            results.append(RestoreResult(
                db_size_mb=actual_size_mb,
                download_time_ms=restore_time,  # We don't separate download vs restore yet
                restore_time_ms=0,
                total_time_ms=restore_time,
                throughput_mbps=throughput,
            ))

            # Cleanup
            db_path.unlink()
            if restore_path.exists():
                restore_path.unlink()

    return results


def benchmark_multi_db_throughput(
    bucket: str,
    endpoint: Optional[str] = None,
    num_databases: int = 10,
    writes_per_sec: int = 100,
    duration_sec: int = 10,
) -> MultiDbResult:
    """
    Measure throughput with multiple databases writing concurrently.

    Strategy:
    1. Create N databases
    2. Start walsync watching all of them
    3. Spawn threads that write to each DB at target rate
    4. Measure sync lag, CPU, memory
    """
    print(f"\nBenchmarking multi-DB throughput ({num_databases} DBs, {writes_per_sec} writes/sec)...")

    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir = Path(tmpdir)

        # Create databases
        databases = []
        for i in range(num_databases):
            db_path = tmpdir / f"test_{i}.db"
            conn = sqlite3.connect(str(db_path))
            conn.execute("PRAGMA journal_mode=WAL")
            conn.execute("CREATE TABLE data (id INTEGER PRIMARY KEY, value BLOB, ts REAL)")
            conn.commit()
            conn.close()
            databases.append(db_path)

        # Start walsync
        walsync_bin = Path(__file__).parent.parent / "target" / "release" / "walsync"
        cmd = [str(walsync_bin), "watch"] + [str(db) for db in databases] + ["-b", bucket]
        if endpoint:
            cmd += ["--endpoint", endpoint]

        proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        time.sleep(2)

        # Writer threads
        stop_flag = threading.Event()
        chunk = b"x" * 1024

        def writer(db_path: Path):
            conn = sqlite3.connect(str(db_path))
            interval = 1.0 / (writes_per_sec / num_databases)

            while not stop_flag.is_set():
                start = time.time()
                conn.execute("INSERT INTO data (value, ts) VALUES (?, ?)", (chunk, start))
                conn.commit()

                elapsed = time.time() - start
                sleep_time = max(0, interval - elapsed)
                time.sleep(sleep_time)

            conn.close()

        # Start writers
        threads = [threading.Thread(target=writer, args=(db,)) for db in databases]
        for t in threads:
            t.start()

        # Monitor walsync process
        time.sleep(1)
        walsync_proc = psutil.Process(proc.pid)

        mem_samples = []
        cpu_samples = []

        for _ in range(duration_sec):
            mem_samples.append(walsync_proc.memory_info().rss / (1024 * 1024))
            cpu_samples.append(walsync_proc.cpu_percent(interval=1))

        # Stop writers
        stop_flag.set()
        for t in threads:
            t.join()

        proc.terminate()
        proc.wait()

        return MultiDbResult(
            num_databases=num_databases,
            writes_per_sec=writes_per_sec,
            sync_lag_p95_ms=0,  # TODO: measure actual sync lag
            cpu_percent=statistics.mean(cpu_samples) if cpu_samples else 0,
            memory_mb=statistics.mean(mem_samples) if mem_samples else 0,
        )


def benchmark_network_recovery(
    bucket: str,
    endpoint: Optional[str] = None,
    outage_duration_sec: int = 5,
) -> NetworkRecoveryResult:
    """
    Measure recovery time after network outage.

    Strategy:
    1. Start walsync + database
    2. Write continuously
    3. Kill walsync (simulating network failure)
    4. Continue writing for N seconds
    5. Restart walsync
    6. Measure time to catch up
    """
    print(f"\nBenchmarking network recovery ({outage_duration_sec}s outage)...")

    # TODO: Implement network recovery benchmark
    # This requires more sophisticated orchestration

    return NetworkRecoveryResult(
        outage_duration_sec=outage_duration_sec,
        catchup_time_ms=0,
        writes_during_outage=0,
        writes_lost=0,
    )


def main():
    parser = argparse.ArgumentParser(
        description="Real-world performance benchmarks for walsync",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--test",
        choices=["sync", "restore", "multi-db", "network"],
        help="Run specific test only",
    )
    parser.add_argument(
        "--bucket",
        type=str,
        default=os.environ.get("WALSYNC_TEST_BUCKET", "s3://walsync-bench"),
        help="S3 bucket for testing",
    )
    parser.add_argument(
        "--endpoint",
        type=str,
        default=os.environ.get("AWS_ENDPOINT_URL_S3"),
        help="S3 endpoint URL",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output results as JSON",
    )

    args = parser.parse_args()

    results = {}

    # Run benchmarks
    if not args.test or args.test == "sync":
        results["sync_latency"] = asdict(benchmark_sync_latency(args.bucket, args.endpoint))

    if not args.test or args.test == "restore":
        restore_results = benchmark_restore(args.bucket, args.endpoint)
        results["restore"] = [asdict(r) for r in restore_results]

    if not args.test or args.test == "multi-db":
        results["multi_db"] = asdict(benchmark_multi_db_throughput(args.bucket, args.endpoint))

    if not args.test or args.test == "network":
        results["network_recovery"] = asdict(benchmark_network_recovery(args.bucket, args.endpoint))

    # Output results
    if args.json:
        print(json.dumps(results, indent=2))
    else:
        print("\n" + "=" * 80)
        print("REAL-WORLD PERFORMANCE RESULTS")
        print("=" * 80)

        if "sync_latency" in results:
            r = results["sync_latency"]
            print(f"\nSync Latency (commit → S3):")
            print(f"  Commits:  {r['num_commits']}")
            print(f"  p50:      {r['p50_ms']:.1f} ms")
            print(f"  p95:      {r['p95_ms']:.1f} ms")
            print(f"  p99:      {r['p99_ms']:.1f} ms")
            print(f"  Mean:     {r['mean_ms']:.1f} ms")
            print(f"  Max:      {r['max_ms']:.1f} ms")

        if "restore" in results:
            print(f"\nRestore Performance:")
            print(f"  {'Size (MB)':>10} {'Time (ms)':>12} {'Throughput (MB/s)':>20}")
            print(f"  {'-' * 10} {'-' * 12} {'-' * 20}")
            for r in results["restore"]:
                print(f"  {r['db_size_mb']:>10.1f} {r['total_time_ms']:>12.0f} {r['throughput_mbps']:>20.2f}")

        if "multi_db" in results:
            r = results["multi_db"]
            print(f"\nMulti-DB Throughput:")
            print(f"  Databases:    {r['num_databases']}")
            print(f"  Writes/sec:   {r['writes_per_sec']:.0f}")
            print(f"  CPU:          {r['cpu_percent']:.1f}%")
            print(f"  Memory:       {r['memory_mb']:.1f} MB")

        if "network_recovery" in results:
            r = results["network_recovery"]
            print(f"\nNetwork Recovery:")
            print(f"  Outage:       {r['outage_duration_sec']}s")
            print(f"  Catchup time: {r['catchup_time_ms']:.0f} ms")


if __name__ == "__main__":
    main()
