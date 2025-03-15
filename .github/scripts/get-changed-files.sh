#!/usr/bin/env bash

set -e

# Complain if the BASE_SHA is not set
BASE_SHA=${BASE_SHA:-HEAD^}
GITHUB_OUTPUT=${GITHUB_OUTPUT:-/dev/null}

# Function to check if any files matching patterns were modified
check_patterns() {
    local result=false
    local patterns=("$@")
    
    for pattern in "${patterns[@]}"; do
        if git diff --name-only "${BASE_SHA}" | grep -q "^${pattern}"; then
            result=true
            break
        fi
    done
    
    echo "$result"
}

# Function to get all modified files matching patterns
get_modified_files() {
    local patterns=("$@")
    for pattern in "${patterns[@]}"; do
        git diff --name-only "${BASE_SHA}" | grep "^${pattern}" || true
    done
}

# Find all env variables in the format MODIFIED_FILES_<category>
while IFS= read -r var; do
    if [[ "$var" != "MODIFIED_FILES_"* ]]; then
        continue
    fi

    # Split the variable name and the value
    varname=${var%=*}
    value=${var#*=}

    # Get the category from the variable name
    category=$(echo "$varname" | cut -d'_' -f3-)
    category=${category,,}

    # Get the patterns from the value
    patterns=($value)

    # Check for modifications and set output
    echo "${category}_any_modified=$(check_patterns "${patterns[@]}")" | tee -a "$GITHUB_OUTPUT"

    # Get all modified files for logging
    files=$(get_modified_files "${patterns[@]}")
    echo "${category}_files=${files//$'\n'/ }" | tee -a "$GITHUB_OUTPUT"
done < <(printenv)