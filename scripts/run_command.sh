#!/bin/sh

if [ "$#" -lt 1 ]; then
  echo "Usage: $0 <command...>"
  exit 1
fi

runs=30
times_file=$(mktemp)
trap "rm -f $times_file" EXIT

TIME_CMD=$(command -v time)

echo "Running: $*"
echo "-----------------------------"

for i in $(seq 1 $runs); do
  t=$($TIME_CMD -f "%e" "$@" 2>&1 >/dev/null)

  if [ -f output.bin ]; then
    size_bytes=$(stat -c %s output.bin 2>/dev/null)
    [ -z "$size_bytes" ] && size_bytes=$(stat -f %z output.bin 2>/dev/null)
  else
    size_bytes="(file not found)"
  fi

  echo "Run $i: $t seconds | output.bin size: $size_bytes bytes"
  echo "$t" >> "$times_file"
done

# Calculate average
avg=$(awk '{ total += $1 } END { print total/NR }' "$times_file")

# Calculate standard deviation
stddev=$(awk -v avg="$avg" '{ sumsq += ($1 - avg)^2 } END { print sqrt(sumsq/NR) }' "$times_file")

echo "-----------------------------"
echo "Average time: $avg seconds"
echo "Standard deviation: $stddev seconds"
