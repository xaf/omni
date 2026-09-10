#!/usr/bin/env bash
#
# Verify that no NEW native library gets linked into omni.
#
# The self-sufficiency check (check-static-linking.sh) only inspects the
# finished binary, so it can only see *dynamic* dependencies. A newly
# introduced *statically* linked C library is invisible to it: it produces no
# DT_NEEDED entry, and it silently grows the binary.
#
# This check works from the authoritative source instead. Every -sys crate
# declares what it links by emitting `cargo:rustc-link-lib=` from its build
# script, so collecting those directives across the whole build yields the
# complete set of native libraries, static and dynamic alike.
#
# The allowlist is keyed on CRATE NAME plus link KIND rather than library
# name, because library names are not stable:
#
#   aws_lc_0_45_0_crypto  embeds the crate version, so it changes on bump
#   blake3_neon           is architecture dependent (blake3_sse2/avx2 on x86)
#
# What we actually want to know is "which crates link native code, and has
# that set changed", which crate+kind captures precisely.
#
# Usage:
#   check-linked-libraries.sh <build-dir> [allowlist]
#
# Example:
#   check-linked-libraries.sh target/aarch64-unknown-linux-musl/dist
#
# The build must already have run: this reads build script output that cargo
# leaves behind. A sanity floor guards against passing vacuously on a tree
# that was never built.

set -euo pipefail

BUILD_DIR=${1:-}
ALLOWLIST=${2:-.github/linked-libraries.allow}

# If fewer than this many link directives are found, assume the build did not
# actually run and refuse to pass. omni currently links 7 native libraries;
# 3 is a deliberately loose floor that still catches an empty target dir.
MIN_EXPECTED_LIBS=${MIN_EXPECTED_LIBS:-3}

if [[ -z "${BUILD_DIR}" ]]; then
    echo >&2 "usage: $0 <build-dir> [allowlist]"
    exit 2
fi

if [[ ! -d "${BUILD_DIR}/build" ]]; then
    echo >&2 "error: ${BUILD_DIR}/build not found; has the build run?"
    exit 2
fi

if [[ ! -f "${ALLOWLIST}" ]]; then
    echo >&2 "error: allowlist not found: ${ALLOWLIST}"
    exit 2
fi

# Collect "<crate>\t<kind>\t<libname>" for every link directive emitted.
# Build directories are named "<crate>-<hash>", so strip the trailing hash.
collect() {
    local f d crate directive kind lib
    for f in "${BUILD_DIR}"/build/*/output; do
        [[ -f "${f}" ]] || continue
        d=$(basename "$(dirname "${f}")")
        crate="${d%-*}"
        while IFS= read -r directive; do
            directive="${directive#cargo:rustc-link-lib=}"
            if [[ "${directive}" == *=* ]]; then
                kind="${directive%%=*}"
                lib="${directive#*=}"
            else
                # No explicit kind: rustc decides, which generally means
                # dynamic. Flag it rather than guess.
                kind="unspecified"
                lib="${directive}"
            fi
            printf '%s\t%s\t%s\n' "${crate}" "${kind}" "${lib}"
        done < <(grep -h '^cargo:rustc-link-lib=' "${f}" 2>/dev/null || true)
    done
}

found=$(collect | sort -u)

if [[ -z "${found}" ]]; then
    echo >&2 "error: no link directives found under ${BUILD_DIR}/build"
    echo >&2 "       the build likely did not run, refusing to pass"
    exit 2
fi

num_found=$(printf '%s\n' "${found}" | wc -l | tr -d ' ')
if [[ "${num_found}" -lt "${MIN_EXPECTED_LIBS}" ]]; then
    echo >&2 "error: only ${num_found} link directive(s) found, expected at least ${MIN_EXPECTED_LIBS}"
    echo >&2 "       the build was probably partial, refusing to pass"
    exit 2
fi

# Allowlist format: "<crate> <kind>", one per line. Blank lines and lines
# starting with # are ignored.
# `|| true` so an allowlist containing only comments (grep matches nothing,
# exit 1) does not abort the script under `set -e` / `pipefail`.
allowed=$(grep -vE '^[[:space:]]*(#|$)' "${ALLOWLIST}" \
    | awk '{print $1"\t"$2}' | sort -u || true)

failures=0

echo "Native libraries linked into ${BUILD_DIR}:"
printf '%s\n' "${found}" | while IFS=$'\t' read -r crate kind lib; do
    printf '  %-18s %-12s %s\n' "${crate}" "${kind}" "${lib}"
done
echo

# Anything dynamic breaks the self-sufficiency invariant outright, whatever
# the allowlist says -- with one named exception.
#
# libgit2-sys links iconv unconditionally for any apple target, without a
# feature gate (its build.rs: `if target.contains("apple")`). Apple ships no
# static libiconv, so this cannot be satisfied; the only way to drop it is to
# drop git2, which is on the shell prompt hot path. libiconv lives in
# SIP-protected /usr/lib and exists on every macOS, so the binary still runs
# anywhere -- which is the property this check defends.
#
# Written as an exact crate+library pair so it stays a single hole rather
# than a widened rule: any other unspecified linkage still fails.
dynamic=$(printf '%s\n' "${found}" \
    | awk -F'\t' '
        $2 == "dylib" { print; next }
        $2 == "unspecified" && !($1 == "libgit2-sys" && $3 == "iconv") { print }
      ' || true)
if [[ -n "${dynamic}" ]]; then
    echo "FAIL: dynamically linked native libraries found:"
    printf '%s\n' "${dynamic}" | while IFS=$'\t' read -r crate kind lib; do
        echo "    ${crate} -> ${kind}=${lib}"
    done
    echo "  omni must be self-sufficient; link these statically or drop them."
    failures=$((failures + 1))
fi

# New crates linking native code.
new=$(comm -23 \
    <(printf '%s\n' "${found}" | awk -F'\t' '{print $1"\t"$2}' | sort -u) \
    <(printf '%s\n' "${allowed}") || true)
if [[ -n "${new}" ]]; then
    echo "FAIL: native library linkage not present in ${ALLOWLIST}:"
    printf '%s\n' "${new}" | while IFS=$'\t' read -r crate kind; do
        echo "    ${crate} (${kind})"
    done
    echo
    echo "  A new native library is now compiled into the binary. This grows"
    echo "  the binary and adds a C toolchain requirement for every target."
    echo "  If it is genuinely wanted, add it to ${ALLOWLIST} in this PR so"
    echo "  the decision is reviewed. See PLAN/02-static-linking.md"
    failures=$((failures + 1))
fi

# Entries that are allowed but no longer present. Informational: a removal is
# good news, but the allowlist should be tidied so it keeps its meaning.
gone=$(comm -13 \
    <(printf '%s\n' "${found}" | awk -F'\t' '{print $1"\t"$2}' | sort -u) \
    <(printf '%s\n' "${allowed}") || true)
if [[ -n "${gone}" ]]; then
    echo "note: allowlisted but no longer linked (consider removing):"
    printf '%s\n' "${gone}" | while IFS=$'\t' read -r crate kind; do
        echo "    ${crate} (${kind})"
    done
    echo
fi

if [[ "${failures}" -gt 0 ]]; then
    echo "${BUILD_DIR}: ${failures} check(s) failed"
    exit 1
fi

echo "${BUILD_DIR}: ${num_found} native library/libraries, all allowlisted"
