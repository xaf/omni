#!/usr/bin/env bash
#
# Guard against binary size regressions.
#
# omni ships as a single self-contained binary, so its size is a
# user-visible property: it is what people download. Nothing checked it
# before, which is how the tool reached 28 MB with no [profile.release]
# section at all.
#
# Baselines are stored per target in .github/binary-size-baseline.txt so
# that any change is reviewed in a pull request rather than drifting.
#
# Usage:
#   check-binary-size.sh <binary> <target-triple> [baseline-file]
#
# Environment:
#   TOLERANCE_PCT   allowed growth over the baseline, default 5
#   UPDATE_BASELINE=1  rewrite the baseline for this target instead of
#                      checking. Intended for deliberate, reviewed updates.

set -euo pipefail

BINARY=${1:-}
TARGET=${2:-}
BASELINE_FILE=${3:-.github/binary-size-baseline.txt}
TOLERANCE_PCT=${TOLERANCE_PCT:-5}

if [[ -z "${BINARY}" || -z "${TARGET}" ]]; then
    echo >&2 "usage: $0 <binary> <target-triple> [baseline-file]"
    exit 2
fi

if [[ ! -f "${BINARY}" ]]; then
    echo >&2 "error: binary not found: ${BINARY}"
    exit 2
fi

size=$(wc -c < "${BINARY}" | tr -d ' ')
size_mb=$(awk -v s="${size}" 'BEGIN { printf "%.2f", s / 1048576 }')

if [[ "${UPDATE_BASELINE:-0}" == "1" ]]; then
    touch "${BASELINE_FILE}"
    tmp=$(mktemp)
    grep -v -E "^${TARGET}[[:space:]]" "${BASELINE_FILE}" > "${tmp}" || true
    printf '%s\t%s\n' "${TARGET}" "${size}" >> "${tmp}"
    # Keep comments first, then sorted entries, so the file stays readable.
    { grep -E '^\s*#' "${tmp}" || true; grep -vE '^\s*#|^\s*$' "${tmp}" | sort; } \
        > "${BASELINE_FILE}"
    rm -f "${tmp}"
    echo "baseline updated: ${TARGET} = ${size} bytes (${size_mb} MB)"
    exit 0
fi

if [[ ! -f "${BASELINE_FILE}" ]]; then
    echo >&2 "error: baseline file not found: ${BASELINE_FILE}"
    echo >&2 "       create it with UPDATE_BASELINE=1 $0 ${BINARY} ${TARGET}"
    exit 2
fi

baseline=$(awk -v t="${TARGET}" '$1 == t { print $2 }' "${BASELINE_FILE}" | head -1)

if [[ -z "${baseline}" ]]; then
    # An unknown target is reported, not failed: a new target should not break
    # the build before anyone has had a chance to record its baseline.
    echo "omni binary size"
    echo "  target:   ${TARGET}"
    echo "  size:     ${size} bytes (${size_mb} MB)"
    echo "  baseline: none recorded"
    echo
    echo "note: no baseline for ${TARGET} in ${BASELINE_FILE}."
    echo "      record it with:"
    echo "        UPDATE_BASELINE=1 $0 ${BINARY} ${TARGET}"
    exit 0
fi

baseline_mb=$(awk -v s="${baseline}" 'BEGIN { printf "%.2f", s / 1048576 }')
limit=$(awk -v b="${baseline}" -v t="${TOLERANCE_PCT}" \
    'BEGIN { printf "%d", b * (1 + t / 100) }')
delta=$((size - baseline))
delta_pct=$(awk -v d="${delta}" -v b="${baseline}" \
    'BEGIN { printf "%+.2f", 100 * d / b }')

echo "omni binary size"
echo "  target:    ${TARGET}"
echo "  size:      ${size} bytes (${size_mb} MB)"
echo "  baseline:  ${baseline} bytes (${baseline_mb} MB)"
echo "  delta:     ${delta} bytes (${delta_pct}%)"
echo "  tolerance: ${TOLERANCE_PCT}% (limit ${limit} bytes)"

if [[ "${size}" -gt "${limit}" ]]; then
    echo
    echo "FAIL: binary grew beyond the ${TOLERANCE_PCT}% tolerance."
    echo "  If this growth is intended, update the baseline in"
    echo "  ${BASELINE_FILE} in this pull request so the change is reviewed."
    exit 1
fi

# Shrinking is good news, but a stale baseline stops the gate being useful,
# so say so loudly enough to be acted on.
shrink_pct=$(awk -v d="${delta}" -v b="${baseline}" \
    'BEGIN { printf "%.0f", -100 * d / b }')
if [[ "${delta}" -lt 0 ]] && [[ "${shrink_pct}" -ge "${TOLERANCE_PCT}" ]]; then
    echo
    echo "note: the binary is ${shrink_pct}% smaller than the baseline."
    echo "      consider lowering the baseline so the gate keeps its value."
fi

echo
echo "within tolerance"
