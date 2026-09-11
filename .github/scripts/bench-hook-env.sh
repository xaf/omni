#!/usr/bin/env bash
#
# Benchmark `omni hook env`, the hottest path in the tool.
#
# The shell integration runs this on every prompt in bash, zsh and fish, and
# fish additionally runs it on every PWD change, so a `cd` fires it twice.
# It is therefore the single most user-visible piece of omni's performance.
#
# The measurement is only meaningful against a workdir that has an up
# environment COMMITTED to the cache. Without one, hook env short-circuits
# almost immediately and reports the bare process startup cost (~0.3 ms),
# which looks great and measures nothing. This script therefore sets up its
# own throwaway workdir, runs `omni up` in it, and only then measures.
#
# Everything is isolated via HOME and the OMNI_* home variables, so the
# caller's real cache and data directories are never touched.
#
# Usage:
#   bench-hook-env.sh <omni-binary> [iterations]
#
# Environment:
#   MAX_MS   fail if the mean exceeds this many milliseconds. Unset means
#            report only, which is the right mode when first establishing a
#            baseline on new hardware.
#
# Note on CI: runners are noisy, so treat this as a regression gate for
# large changes (a 2x blowup), not as a precision benchmark.

set -euo pipefail

BINARY=${1:-}
ITERATIONS=${2:-50}

if [[ -z "${BINARY}" ]]; then
    echo >&2 "usage: $0 <omni-binary> [iterations]"
    exit 2
fi

if [[ ! -x "${BINARY}" ]]; then
    echo >&2 "error: not an executable: ${BINARY}"
    exit 2
fi

BINARY=$(cd "$(dirname "${BINARY}")" && pwd)/$(basename "${BINARY}")

WORKDIR=$(mktemp -d)
cleanup() { rm -rf "${WORKDIR}"; }
trap cleanup EXIT

cd "${WORKDIR}"
git init -q .
git -c user.email=bench@example.com -c user.name=bench \
    commit -q --allow-empty -m "bench"

# A config with an `up` block, so the workdir gets a real committed
# environment and hook env has to do its actual work. A custom step is used
# rather than a real tool so the benchmark needs no network.
cat > .omni.yaml <<'YAML'
up:
  - custom:
      meet: |
        true
      met?: |
        exit 1
env:
  BENCH_VAR: bench-value
YAML

export HOME="${WORKDIR}/home"
export OMNI_DATA_HOME="${WORKDIR}/data"
export OMNI_CACHE_HOME="${WORKDIR}/cache"
export OMNI_STATE_HOME="${WORKDIR}/state"
mkdir -p "${HOME}"

# Establish the environment. Without this the benchmark measures nothing.
if ! "${BINARY}" up --trust >/dev/null 2>&1; then
    echo >&2 "error: \`omni up\` failed in the benchmark workdir"
    exit 2
fi

# Confirm hook env actually emits an environment, i.e. that we are measuring
# the real path and not an early return.
#
# Do NOT pipe into `grep -q` here. Under `set -o pipefail` grep exits on the
# first match, omni then dies of SIGPIPE, and the pipeline reports failure --
# so a perfectly healthy binary looks broken. This bit the linking gate too;
# see PLAN/02-static-linking.md.
hook_output=$("${BINARY}" hook env bash 2>/dev/null || true)
if [[ "${hook_output}" != *BENCH_VAR* ]]; then
    echo >&2 "error: hook env did not emit the expected environment;"
    echo >&2 "       the benchmark would measure an early return, refusing to continue"
    exit 2
fi

# What the shell integration sets. Its presence changes the code path, and
# without it the update-error check is skipped.
export OMNI_SHELL_PPID=$$

# Warm the page cache and any first-run cache work.
for _ in 1 2 3; do "${BINARY}" hook env bash >/dev/null 2>&1; done

start=$(date +%s%N)
for _ in $(seq 1 "${ITERATIONS}"); do
    "${BINARY}" hook env bash >/dev/null 2>&1
done
end=$(date +%s%N)

mean_ms=$(awk -v s="${start}" -v e="${end}" -v n="${ITERATIONS}" \
    'BEGIN { printf "%.2f", (e - s) / 1000000 / n }')

# Bare process startup, as a floor to compare against.
start=$(date +%s%N)
for _ in $(seq 1 "${ITERATIONS}"); do
    "${BINARY}" hook uuid >/dev/null 2>&1
done
end=$(date +%s%N)
floor_ms=$(awk -v s="${start}" -v e="${end}" -v n="${ITERATIONS}" \
    'BEGIN { printf "%.2f", (e - s) / 1000000 / n }')

echo "omni hook env benchmark"
echo "  binary:      ${BINARY}"
echo "  iterations:  ${ITERATIONS}"
echo "  hook env:    ${mean_ms} ms/invocation"
echo "  floor:       ${floor_ms} ms/invocation  (hook uuid, process startup)"
echo "  overhead:    $(awk -v a="${mean_ms}" -v b="${floor_ms}" 'BEGIN{printf "%.2f", a-b}') ms above floor"

if [[ -n "${MAX_MS:-}" ]]; then
    if awk -v m="${mean_ms}" -v x="${MAX_MS}" 'BEGIN { exit !(m > x) }'; then
        echo
        echo "FAIL: ${mean_ms} ms exceeds the ${MAX_MS} ms budget"
        exit 1
    fi
    echo "  budget:      ${MAX_MS} ms, within budget"
fi
