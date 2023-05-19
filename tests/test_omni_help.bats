#!/usr/bin/env bats

load 'helpers/utils'

setup() {
  omni_setup 3>-

  # Override the default columns to 100 so we have a controlled
  # environment for testing the output of the help command
  export COLUMNS=100
}


# bats test_tags=omni:help
@test "omni help shows the help message with default omni commands" {
  # We do want to check the default commands and their help, not the
  # word wrapping, so let's use a very large COLUMNS value here for now
  export COLUMNS=1000

  run omni help 3>-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  echo "$output" | grep -q "^ *Usage: omni <command> \[options\] ARG\.\.\. *$"

  # General section
  echo "$output" | grep -q "^ *General *$"
  echo "$output" | grep -q "^ *help *Show help for omni commands *$"
  echo "$output" | grep -q "^ *status *Show status of omni *$"

  # Git commands section
  echo "$output" | grep -q "^ *Git commands *$"
  echo "$output" | grep -q "^ *cd *Change directory to the git directory of the specified repository *$"
  echo "$output" | grep -q "^ *clone *Clone the specified repository *$"
  echo "$output" | grep -q "^ *down, up *Sets up or tear down a repository depending on its up configuration *$"
  echo "$output" | grep -q "^ *organize *Organize your git repositories using the configured format *$"
  echo "$output" | grep -q "^ *scope *Runs an omni command in the context of the specified repository *$"
}

# bats test_tags=omni:help
@test "omni help shows the help message wrapped for smaller screens" {
  expected=$(echo 'omni - omnipotent tool

Usage: omni <command> [options] ARG...

General
  help            Show help for omni commands
  status          Show status of omni

Git commands
  cd              Change directory to the git directory of
                  the specified repository
  clone           Clone the specified repository
  down, up        Sets up or tear down a repository
                  depending on its up configuration
  organize        Organize your git repositories using the
                  configured format
  scope           Runs an omni command in the context of
                  the specified repository
')

  export COLUMNS=60
  run omni help 3>-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>- | cat -A 3>-
  [ "$?" -eq 0 ]

  [[ "$output" == "$expected" ]]
}
