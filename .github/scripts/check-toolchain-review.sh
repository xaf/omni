#!/usr/bin/env bash
# A compiler change needs a current approval from a human collaborator.
set -euo pipefail

: "${GITHUB_REPOSITORY:?}"
: "${PR_NUMBER:?}"
: "${PR_HEAD_SHA:?}"

gh api --paginate --slurp \
    "repos/${GITHUB_REPOSITORY}/pulls/${PR_NUMBER}/reviews" |
    jq --exit-status --arg head "$PR_HEAD_SHA" '
        add
        | map(select(.state != "COMMENTED" and .state != "PENDING"))
        | group_by(.user.login)
        | map(max_by(.submitted_at))
        | any(
            .state == "APPROVED"
            and .commit_id == $head
            and .user.type == "User"
            and (.author_association == "OWNER"
                 or .author_association == "MEMBER"
                 or .author_association == "COLLABORATOR")
        )
    ' || {
        echo >&2 "Rust toolchain changes require human maintainer approval of the current PR head."
        exit 1
    }
