#!/usr/bin/env bats

load 'helpers/utils'

setup() {
  omni_setup 3>&-

  setup_omni_config 3>&-

  # Depending on the 'cat' command, check if '-A' is supported
  if cat -A </dev/null 2>/dev/null; then
    export CAT_OPTS='A'
  else
    export CAT_OPTS='e'
  fi

  # Override the default columns to 100 so we have a controlled
  # environment for testing the output of the help command
  export COLUMNS=100

  # Disable colors
  export NO_COLOR=1
}

validate_test_output() {
  local fixture_file="$1"
  shift

  local exit_code=0
  if [[ "$1" = "exit_code="* ]]; then
    exit_code=${1#exit_code=}
    shift
  fi

  # Handle the fixtures
  fixture_file="${FIXTURES_DIR}/${fixture_file}"
  if [ "$GENERATE_FIXTURES" = "true" ]; then
    run mkdir -p "$(dirname "$fixture_file")"
    [ "$status" -eq 0 ]
    run "$@" 3>&-
    [ "$status" -eq "$exit_code" ]
    echo "$output" >"$fixture_file"
    return 0
  fi
  expected=$(cat "$fixture_file")

  # Run the test
  run "$@" 3>&-

  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq "$exit_code" ]

  # Remove the first line for both the expected and the output, so we can avoid
  # the version number in the comparison
  expected=$(echo "$expected" | tail -n +2)
  output=$(echo "$output" | tail -n +2)

  set -o pipefail
  diff -u <(echo "$expected") <(echo "$output") 3>&- | cat "-$CAT_OPTS" 3>&-
  [ "$?" -eq 0 ]
  [[ "$output" == "$expected" ]]
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help shows the help message with default omni commands" {
  # Avoiding any shorter-than-expected wrapping
  export COLUMNS=1000

  validate_test_output omni/help.txt omni help
}

# bats test_tags=generate,omni:help
@test "omni help shows the help message wrapped for smaller screens" {
  # Set the columns to 60 to force wrapping
  export COLUMNS=60

  validate_test_output omni/help-wrapped-60.txt omni help
}

# bats test_tags=generate,omni:help
@test "omni help help shows the help message for the command" {
  validate_test_output omni/help-help.txt omni help help
}

# bats test_tags=generate,omni:help
@test "omni help status shows the help message for the command" {
  validate_test_output omni/help-status.txt omni help status
}

# bats test_tags=generate,omni:help
@test "omni help cd shows the help message for the command" {
  validate_test_output omni/help-cd.txt omni help cd
}

# bats test_tags=generate,omni:help
@test "omni help clone shows the help message for the command" {
  validate_test_output omni/help-clone.txt omni help clone
}

# bats test_tags=generate,omni:help
@test "omni help down shows the help message for the command" {
  validate_test_output omni/help-down.txt omni help down
}

# bats test_tags=generate,omni:help
@test "omni help scope shows the help message for the command" {
  validate_test_output omni/help-scope.txt omni help scope
}

# bats test_tags=generate,omni:help
@test "omni help tidy shows the help message for the command" {
  validate_test_output omni/help-tidy.txt omni help tidy
}

# bats test_tags=generate,omni:help,omni:help:up
@test "omni help up shows the help message for the command" {
  validate_test_output omni/help-up.txt omni help up
}

setup_very_long_config_command() {
  local omni_config="${HOME}/.config/omni/config.yaml"
  mkdir -p "$(dirname "$omni_config")"
  cat <<EOF >>"$omni_config"
commands:
  supercalifragilisticexpialidocious:
    aliases:
      - abracadabra
      - hocuspocus
      - open-sesame
    desc: |
      lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do
      eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut
      enim ad minim veniam, quis nostrud exercitation ullamco laboris
      nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor
      in reprehenderit in voluptate velit esse cillum dolore eu fugiat
      nulla pariatur. Excepteur sint occaecat cupidatat non proident,
      sunt in culpa qui officia deserunt mollit anim id est laborum.
    run: |
      echo "Hello, world!"
EOF
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help shows the help message with a very long config command (columns=1000)" {
  setup_very_long_config_command
  export COLUMNS=1000
  validate_test_output omni/help-long-config-command-${COLUMNS}.txt omni help
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help shows the help message with a very long config command (columns=100)" {
  setup_very_long_config_command
  export COLUMNS=100
  validate_test_output omni/help-long-config-command-${COLUMNS}.txt omni help
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help shows the help message with a very long config command (columns=50)" {
  setup_very_long_config_command
  export COLUMNS=50
  validate_test_output omni/help-long-config-command-${COLUMNS}.txt omni help
}

# bats test_tags=generate,omni:help,omni:help:self
@test "omni help fails to show the help message if terminal width is too low (columns=10)" {
  setup_very_long_config_command
  export COLUMNS=10
  validate_test_output omni/help-long-config-command-${COLUMNS}.txt exit_code=1 omni help
}
