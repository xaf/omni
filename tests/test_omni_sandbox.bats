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

@test "[omni_sandbox=01] creates sandbox directory with configuration and id" {
  run omni sandbox --name demo python go@1.21.0 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  sandbox_dir="${HOME}/sandbox/demo"
  [ -d "${sandbox_dir}" ]
  [ -f "${sandbox_dir}/.omni.yaml" ]
  [ -f "${sandbox_dir}/.omni/id" ]

  run grep -F "  - python" "${sandbox_dir}/.omni.yaml"
  [ "$status" -eq 0 ]
  run grep -F "  - go@1.21.0" "${sandbox_dir}/.omni.yaml"
  [ "$status" -eq 0 ]
}

@test "[omni_sandbox=02] initializes current directory as sandbox with --path" {
  mkdir project
  cd project

  run omni sandbox --path . nodejs@20.0.0 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  [ -f ".omni.yaml" ]
  [ -f ".omni/id" ]

  run grep -F "  - nodejs@20.0.0" ".omni.yaml"
  [ "$status" -eq 0 ]
}

@test "[omni_sandbox=03] generates name when none is provided" {
  run omni sandbox python 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  sandbox_dir="$(echo "$output" | sed -n 's/^omni: sandbox: sandbox initialized at //p' | tail -n 1)"
  [ -n "$sandbox_dir" ]
  [[ "$sandbox_dir" == "${HOME}/sandbox/"* ]]

  [ -d "${sandbox_dir}" ]
  [ -f "${sandbox_dir}/.omni.yaml" ]
  [ -f "${sandbox_dir}/.omni/id" ]

  run grep -F "  - python" "${sandbox_dir}/.omni.yaml"
  [ "$status" -eq 0 ]
}

@test "[omni_sandbox=04] --name and --path are exclusive" {
  run omni sandbox --name clash --path . 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"cannot be used with '--path"* ]]
}
