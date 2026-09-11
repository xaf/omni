#!/usr/bin/env bash
#
# Report the size of a built omni binary.
#
# Deliberately a report, not a gate. A baseline was tried and removed: any
# threshold loose enough not to trip on ordinary feature work is also loose
# enough for growth to ratchet through it unnoticed, and the routine
# response to a tripped threshold is to bump the baseline, which turns the
# check into a rubber stamp with maintenance cost attached.
#
# The sharp edge -- a new native C dependency being linked in -- is caught
# precisely by check-linked-libraries.sh, which has a real yes/no answer.
# This just keeps the number visible.
#
# Usage: report-binary-size.sh <binary> <target-triple>

set -euo pipefail

BINARY=${1:-}
TARGET=${2:-}

if [[ -z "${BINARY}" ]] || [[ -z "${TARGET}" ]]; then
    echo >&2 "usage: $0 <binary> <target-triple>"
    exit 2
fi

if [[ ! -f "${BINARY}" ]]; then
    echo >&2 "error: binary not found: ${BINARY}"
    exit 2
fi

# stat's flags differ between GNU and BSD/macOS.
if size=$(stat -c %s "${BINARY}" 2>/dev/null); then
    :
else
    size=$(stat -f %z "${BINARY}")
fi

size_mb=$(awk -v s="${size}" 'BEGIN { printf "%.2f", s / 1048576 }')

echo "omni binary size"
echo "  target: ${TARGET}"
echo "  size:   ${size} bytes (${size_mb} MB)"

# Surface it on the run summary page too, so the number does not require
# expanding a step in a per-target job log to find.
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    {
        echo "### omni binary size"
        echo
        echo "| target | bytes | size |"
        echo "| --- | ---: | ---: |"
        echo "| \`${TARGET}\` | ${size} | ${size_mb} MB |"
    } >> "${GITHUB_STEP_SUMMARY}"
fi
