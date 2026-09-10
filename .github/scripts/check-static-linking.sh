#!/usr/bin/env bash
#
# Verify that a built omni binary is self-sufficient, i.e. that it does not
# depend on any dynamic library that we do not control.
#
# The invariant differs per platform:
#
#   *-linux-musl    zero dynamic dependencies; no DT_NEEDED, no PT_INTERP
#   *-linux-gnu     informational only; glibc builds are dynamic by design
#                   and are not shipped, so this is reported and not enforced
#   *-apple-darwin  only the system libraries are allowed, because fully
#                   static linking is not supported on macOS
#
# This inspects the finished binary, so it can only see DYNAMIC
# dependencies. A statically linked library leaves no trace here -- no
# DT_NEEDED entry, nothing in `otool -L`. Use check-linked-libraries.sh
# alongside this to cover static linkage; the two are complementary.
#
# Usage:
#   check-static-linking.sh <binary> <target-triple>

set -euo pipefail

# NOTE on `grep -q` and `set -o pipefail`
#
# Never write `producer | grep -q pattern` in a condition in this repo.
# `grep -q` exits as soon as it matches; the producer is then killed by
# SIGPIPE, and with `pipefail` the whole pipeline reports failure. So a
# MATCH can look like a MISS -- which in a safety gate means a false pass.
#
# It is output-size dependent, so it appears to work on small inputs and
# silently breaks on large ones. Capture into a variable with `|| true` and
# test for emptiness instead. Enforced by check-shell-pitfalls.sh.

BINARY=${1:-}
TARGET=${2:-}

if [[ -z "${BINARY}" || -z "${TARGET}" ]]; then
    echo >&2 "usage: $0 <binary> <target-triple>"
    exit 2
fi

if [[ ! -f "${BINARY}" ]]; then
    echo >&2 "error: binary not found: ${BINARY}"
    exit 2
fi

failures=0

fail() {
    echo "  FAIL: $*"
    failures=$((failures + 1))
}

ok() {
    echo "  ok: $*"
}

# Resolve a tool, preferring an llvm- prefixed variant when the plain one is
# missing. Cross-build containers do not always ship the binutils name.
find_tool() {
    local name=$1
    local candidate
    for candidate in "${name}" "llvm-${name}"; do
        if command -v "${candidate}" >/dev/null 2>&1; then
            echo "${candidate}"
            return 0
        fi
    done
    return 1
}

echo "Checking dynamic linking of ${BINARY} (${TARGET})"

case "${TARGET}" in
    *-linux-musl)
        # A musl build must be entirely static. Prefer reading the ELF
        # headers over `ldd`, whose output text varies between libc
        # implementations while the headers do not.
        if ! readelf=$(find_tool readelf); then
            echo >&2 "error: readelf not found, cannot verify ${TARGET}"
            exit 2
        fi

        needed=$("${readelf}" -d "${BINARY}" 2>/dev/null | grep -c 'NEEDED' || true)
        if [[ "${needed}" -ne 0 ]]; then
            fail "${needed} dynamic dependency entries (DT_NEEDED) found:"
            "${readelf}" -d "${BINARY}" 2>/dev/null | grep 'NEEDED' | sed 's/^/    /'
        else
            ok "no dynamic dependencies (DT_NEEDED)"
        fi

        # A program interpreter means the loader is involved, i.e. not static.
        #
        # Captured into a variable rather than piped into `grep -q`: see the
        # note in the header about SIGPIPE under `set -o pipefail`.
        interp=$("${readelf}" -l "${BINARY}" 2>/dev/null \
            | grep -A1 'INTERP' || true)
        if [[ -n "${interp}" ]]; then
            fail "program interpreter (PT_INTERP) present, binary is not static"
            printf '%s\n' "${interp}" | sed 's/^/    /'
        else
            ok "no program interpreter (PT_INTERP)"
        fi
        ;;

    *-apple-darwin)
        # Fully static linking is not supported on macOS: libSystem is always
        # dynamic. Allow the system libraries and nothing else.
        if ! otool=$(find_tool otool); then
            echo >&2 "error: otool not found, cannot verify ${TARGET}"
            exit 2
        fi

        # The first line of `otool -L` output is the binary's own name.
        libs=$("${otool}" -L "${BINARY}" | tail -n +2 | awk '{print $1}')

        if [[ -z "${libs}" ]]; then
            ok "no dynamic dependencies at all"
        fi

        while IFS= read -r lib; do
            [[ -z "${lib}" ]] && continue
            case "${lib}" in
                /usr/lib/libSystem.B.dylib) ok "system: ${lib}" ;;
                /System/Library/Frameworks/*) ok "system framework: ${lib}" ;;
                *) fail "disallowed dynamic dependency: ${lib}" ;;
            esac
        done <<< "${libs}"

        # An @rpath entry means the binary expects to locate libraries at
        # runtime, which is exactly what we are trying to avoid.
        # Captured rather than piped into `grep -q`; `otool -l` output is
        # large, which makes the SIGPIPE inversion especially likely here.
        rpaths=$("${otool}" -l "${BINARY}" 2>/dev/null | grep 'LC_RPATH' || true)
        if [[ -n "${rpaths}" ]]; then
            fail "LC_RPATH present, binary expects runtime library lookup"
        else
            ok "no LC_RPATH"
        fi
        ;;

    *-linux-gnu)
        # glibc builds are dynamic by design. We do not ship them, so report
        # what is linked but do not fail: this keeps the script usable for
        # local development on a gnu host.
        echo "  note: ${TARGET} is a glibc target and is not shipped;" \
             "reporting only"
        if readelf=$(find_tool readelf); then
            "${readelf}" -d "${BINARY}" 2>/dev/null \
                | grep 'NEEDED' | sed 's/^/    /' || echo "    (none)"
        fi
        ;;

    *)
        echo >&2 "error: unhandled target ${TARGET}; refusing to pass silently"
        exit 2
        ;;
esac

echo
if [[ "${failures}" -gt 0 ]]; then
    echo "${BINARY} (${TARGET}): ${failures} check(s) failed"
    exit 1
fi

echo "${BINARY} (${TARGET}): all checks passed"
