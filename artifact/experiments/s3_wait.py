#!/usr/bin/env python3
"""Blocks until a bucket prefix has finished receiving objects.

The Figure 6 comparison is only fair if both arms are timed to the same
finish line: every tensor durably in the object store. The Peridot arm
reaches it on its own, because the `batch` context drains its uploads during
shutdown and the runtime does not exit before that finishes. The Wasmtime arm
does not -- its batcher is a separate process that keeps working after the
guest exits -- so the script waits for it here.

Two ways to decide it is done:

  * the expected object count is reached, or
  * the count stops changing for `--quiet-for` seconds.

The second matters because tensor_batcher names objects `batch_<u16>`, and a
u16 drawn at random collides often enough at these batch counts (~12% for 128
objects) that the exact expected count is sometimes never reached. Waiting for
quiescence measures the same instant without depending on every key being
distinct.

Usage: s3_wait.py <prefix> <expected_count> [timeout_seconds] [quiet_for]
Exits 0 when settled, 1 on timeout. Prints elapsed seconds.
"""
import os
import sys
import time

import boto3
from botocore.client import Config


def count(s3, bucket, prefix):
    paginator = s3.get_paginator("list_objects_v2")
    return sum(page.get("KeyCount", 0)
               for page in paginator.paginate(Bucket=bucket, Prefix=prefix))


def main():
    if len(sys.argv) < 3:
        print("usage: s3_wait.py <prefix> <expected> [timeout_s] [quiet_for_s]",
              file=sys.stderr)
        return 2

    prefix = sys.argv[1]
    expected = int(sys.argv[2])
    timeout = float(sys.argv[3]) if len(sys.argv) > 3 else 300.0
    quiet_for = float(sys.argv[4]) if len(sys.argv) > 4 else 3.0

    s3 = boto3.client(
        "s3",
        endpoint_url=os.environ.get("S3_ENDPOINT") or os.environ.get("AWS_ENDPOINT_URL"),
        aws_access_key_id=os.environ["AWS_ACCESS_KEY_ID"],
        aws_secret_access_key=os.environ["AWS_SECRET_ACCESS_KEY"],
        region_name=os.environ.get("AWS_REGION", "us-east-1"),
        # Self-hosted S3-compatible stores are reached by hostname, which the
        # default virtual-hosted addressing turns into <bucket>.<host>.
        config=Config(signature_version="s3v4", s3={"addressing_style": "path"},
                      retries={"max_attempts": 3}),
    )
    bucket = os.environ.get("S3_BUCKET", "peridot-ae")

    # The caller passes the moment it started waiting in WAIT_T0, so that
    # interpreter and boto3 start-up count against the arm being timed rather
    # than disappearing from the measurement.
    start = float(os.environ.get("WAIT_T0") or time.time())
    seen = 0
    last_change = start

    while time.time() - start < timeout:
        now_seen = count(s3, bucket, prefix)
        if now_seen >= expected:
            print(f"{time.time() - start:.3f}")
            return 0
        if now_seen != seen:
            seen = now_seen
            last_change = time.time()
        elif seen > 0 and time.time() - last_change >= quiet_for:
            # Settled below the expected count: almost always key collisions.
            print(f"{time.time() - start - quiet_for:.3f}")
            print(f"settled at {seen}/{expected} objects under {prefix} "
                  f"(likely key collisions)", file=sys.stderr)
            return 0
        time.sleep(0.1)

    print(f"{time.time() - start:.3f}")
    print(f"timeout: saw {seen}/{expected} objects under {prefix}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
