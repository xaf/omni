#!/usr/bin/env bash
#
# Lint our own shell scripts for pitfalls that fail SILENTLY.
#
# This exists because of a concrete incident. `check-static-linking.sh` used
#
#     producer | grep -q PATTERN
#
# inside an `if`, under `set -o pipefail`. `grep -q` exits the moment it
# matches, the producer is then killed by SIGPIPE, and pipefail makes the
# whole pipeline report failure -- so a MATCH is indistinguishable from a
# MISS. In a safety gate that means a false PASS.
#
# It is output-size dependent, which is what makes it so dangerous:
#
#     readelf -l /bin/ls | grep -q INTERP   -> detected     (small output)
#     readelf -a /bin/ls | grep -q INTERP   -> MISSED       (large output)
#
# The pattern was written up in PLAN/02-static-linking.md, and then
# reintroduced a few hours later in bench-hook-env.sh, and two further
# instances survived in the very file that had supposedly been fixed.
# Documentation plainly was not enough, hence this check.
#
# Usage:
#   check-shell-pitfalls.sh [paths...]     (default: .github/scripts)

set -euo pipefail

PATHS=("$@")
if [[ ${#PATHS[@]} -eq 0 ]]; then
    PATHS=(".github/scripts")
fi

findings=0

# Blank out comment lines while preserving line numbering, so that prose
# describing these pitfalls -- including this file's own header -- is not
# reported as an instance of them.
strip_comments() {
    sed 's/^[[:space:]]*#.*$//' "$1"
}

report() {
    echo "  ${1}:${2}"
    echo "      ${3}"
    echo "      ${4}"
    findings=$((findings + 1))
}

# Collect the scripts to inspect. Only files that actually enable pipefail
# are at risk, so the check is scoped to those.
scripts=()
while IFS= read -r f; do
    [[ -n "${f}" ]] || continue
    if grep -qE 'set -[a-z]*o pipefail|set -o pipefail' "${f}" 2>/dev/null; then
        scripts+=("${f}")
    fi
done < <(find "${PATHS[@]}" -type f -name '*.sh' 2>/dev/null | sort)

if [[ ${#scripts[@]} -eq 0 ]]; then
    echo "no pipefail-enabled shell scripts found under: ${PATHS[*]}"
    exit 0
fi

echo "Checking ${#scripts[@]} pipefail-enabled script(s) for silent-failure pitfalls"
echo

for f in "${scripts[@]}"; do
    # 1. `| grep -q` anywhere. Under pipefail this inverts on early exit.
    while IFS=: read -r lineno content; do
        [[ -n "${lineno}" ]] || continue
        report "${f}" "${lineno}" \
            "$(echo "${content}" | sed 's/^[[:space:]]*//')" \
            "grep -q after a pipe inverts under pipefail (SIGPIPE). Capture with \$(... || true) and test for emptiness."
    done < <(strip_comments "${f}" \
        | grep -nE '\|[[:space:]]*grep[^|]*[[:space:]]-[a-zA-Z]*q' || true)

    # 2. `| head` / `| head -n` in a command substitution. head exits after
    #    N lines, so the producer can die of SIGPIPE for the same reason.
    while IFS=: read -r lineno content; do
        [[ -n "${lineno}" ]] || continue
        case "${content}" in
            *'|| true'*) continue ;;
        esac
        report "${f}" "${lineno}" \
            "$(echo "${content}" | sed 's/^[[:space:]]*//')" \
            "head after a pipe can SIGPIPE the producer under pipefail. Use awk '...; exit', or append || true."
    done < <(strip_comments "${f}" \
        | grep -nE '=\$\(.*\|[[:space:]]*head([[:space:]]|\))' || true)
done

echo
if [[ "${findings}" -gt 0 ]]; then
    echo "${findings} pitfall(s) found"
    echo
    echo "These fail silently rather than loudly, which is why they are"
    echo "rejected outright rather than left to review."
    exit 1
fi

echo "no pitfalls found"
