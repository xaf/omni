#!/usr/bin/env bats

load 'helpers/utils'

setup() {
  omni_setup 3>&-

  setup_omni_config 3>&-

  # Disable colors
  export NO_COLOR=1

  # Avoid wrapping
  export COLUMNS=1000
}

@test "[omni_sandbox=01] --name and --path are exclusive" {
  run omni sandbox --name clash --path . 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"cannot be used with '--path"* ]]
}
