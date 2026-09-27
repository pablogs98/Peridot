#!/usr/bin/env python3
"""Empties the artifact's bucket, creating it if it does not exist.

Experiments count objects to decide when an arm has finished, so each run has
to start from a known state.
"""
import os
import sys

import boto3
from botocore.client import Config


def main():
    s3 = boto3.client(
        "s3",
        endpoint_url=os.environ.get("S3_ENDPOINT") or os.environ.get("AWS_ENDPOINT_URL"),
        aws_access_key_id=os.environ["AWS_ACCESS_KEY_ID"],
        aws_secret_access_key=os.environ["AWS_SECRET_ACCESS_KEY"],
        region_name=os.environ.get("AWS_REGION", "us-east-1"),
        # Path-style for the same reason as in s3_wait.py.
        config=Config(signature_version="s3v4", s3={"addressing_style": "path"}),
    )
    bucket = os.environ.get("S3_BUCKET", "peridot-ae")

    try:
        s3.head_bucket(Bucket=bucket)
    except Exception:
        s3.create_bucket(Bucket=bucket)
        print(f"created s3://{bucket}")
        return 0

    deleted = 0
    for page in s3.get_paginator("list_objects_v2").paginate(Bucket=bucket):
        keys = [{"Key": o["Key"]} for o in page.get("Contents", [])]
        if keys:
            s3.delete_objects(Bucket=bucket, Delete={"Objects": keys})
            deleted += len(keys)
    print(f"emptied s3://{bucket} ({deleted} objects removed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
