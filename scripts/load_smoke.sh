#!/usr/bin/env bash
set -euo pipefail

url="${1:-http://127.0.0.1:8080/}"
duration="${DURATION:-10s}"
connections="${CONNECTIONS:-10}"

if command -v oha >/dev/null 2>&1; then
    exec oha -z "$duration" -c "$connections" "$url"
fi

if ! command -v curl >/dev/null 2>&1; then
    printf 'load smoke requires oha or curl\n' >&2
    exit 1
fi

end_time=$((SECONDS + ${duration%s}))
requests=0
failures=0
while (( SECONDS < end_time )); do
    for _ in $(seq 1 "$connections"); do
        if curl --fail --silent --show-error --max-time 5 "$url" >/dev/null; then
            ((requests += 1))
        else
            ((failures += 1))
        fi
    done
done

printf 'requests=%d failures=%d duration=%s concurrency=%s\n' "$requests" "$failures" "$duration" "$connections"
(( failures == 0 ))
