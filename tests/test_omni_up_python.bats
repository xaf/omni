#!/usr/bin/env bats

load 'helpers/utils'
load 'helpers/mise'

setup() {
  # Setup the environment for the test; this should override $HOME too
  omni_setup 3>&-

  setup_omni_config 3>&-

  # Add one repository
  setup_git_dir "git/github.com/test1org/test1repo" "git@github.com:test1org/test1repo.git"

  # Change directory to the repository
  cd "git/github.com/test1org/test1repo"

  setup_uv_cache
}

setup_uv_cache() {
  # Set the versions for uv so that we don't do any API call during the tests
  uv_repo="astral-sh/uv"
  setup_cache_db
  local cache_db="${HOME}/.cache/omni/cache.db"
  local versions_file="${FIXTURES_DIR}/astral-sh-uv-versions.txt"
  local uv_version=$(cat "${versions_file}" | jq -r '
    [.[] | select(.draft == false and (.prerelease == false or .prerelease == null))] |
    map(.tag_name | sub("^v"; "")) |
    max_by(split(".") | map(tonumber)) |
    if . then . else "" end
  ')
  sqlite3 "$cache_db" \
    "INSERT INTO github_releases (repository, releases, fetched_at)
      VALUES ('${uv_repo}', CAST(readfile('${versions_file}') AS TEXT),
              strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '+1 day'))"
  uv_bin="${HOME}/.local/share/omni/ghreleases/${uv_repo}/${uv_version}/uv"
  if [ ! -e "$uv_bin" ]; then
    add_fakebin "$uv_bin"
  fi
}

teardown() {
  check_commands
}

add_mise_python_calls() {
  add_mise_tool_calls tool=python venv=true "$@"
}

add_brew_python_calls() {
  add_mise_tool_brew_calls python
}

add_nix_python_calls() {
  local tmpdir="${TMPDIR:-/tmp}"
  # Make sure that tmpdir does not end with /
  tmpdir="${tmpdir%/}"

  local nix=(nix --extra-experimental-features "nix-command flakes")

  add_command "${nix[@]}" print-dev-env --verbose --print-build-logs --profile "regex:${tmpdir}/omni_up_nix\..*/profile" --impure --expr 'with import <nixpkgs> {}; mkShell { buildInputs = [ bzip2 gawk gcc gdbm gnumake gnused libffi ncurses openssl pkg-config readline sqlite xz zlib ]; }'
  add_command "${nix[@]}" build --print-out-paths --out-link "regex:${HOME}/\.local/share/omni/wd/.*/nix/profile-pkgs-.*" "regex:${tmpdir}/omni_up_nix\..*/profile" <<EOF
#omni-test-bash-function
function process_response() {
  local args=("\$@")

  # Find the location of --out-link, and use the next argument
  local out_link=""
  for ((i=0; i<\${#args[@]}; i++)); do
    if [[ "\${args[i]}" == "--out-link" ]]; then
      out_link="\${args[i+1]}"
      break
    fi
  done

  # Write a fake nix profile to the file
  mkdir -p "\$(dirname "\$out_link")"
  echo '{"variables": {}}' > "\$out_link"
}
EOF
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=01] omni up python operation (latest) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_mise_python_calls

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=02] omni up python operation (latest) using brew for dependencies (other versions installed)" {
  cat > .omni.yaml <<EOF
up:
  - python:
      upgrade: true
EOF

  add_brew_python_calls
  add_mise_python_calls others_installed="3.11.6,3.11.8" upgrade=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=03] omni up python operation (latest) using brew for dependencies and call pip (single requirements file)" {
  cat > .omni.yaml <<EOF
up:
  - python:
      pip: requirements.txt
EOF

  add_brew_python_calls
  add_mise_python_calls

  touch requirements.txt
  add_command uv pip install --python "regex:${HOME}/\\.local/share/omni/wd/[^ /]+/python/3.12.3/root/bin/python" -r "${PWD}/requirements.txt"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"installing dependencies from requirements.txt"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=04] omni up python operation (latest) using brew for dependencies and call pip (multiple requirements file)" {
  cat > .omni.yaml <<EOF
up:
  - python:
      pip:
      - requirements.txt
      - requirements2.txt
EOF

  add_brew_python_calls
  add_mise_python_calls

  touch requirements.txt
  touch requirements2.txt
  add_command uv pip install --python "regex:${HOME}/\\.local/share/omni/wd/[^ /]+/python/3.12.3/root/bin/python" -r "${PWD}/requirements.txt"
  add_command uv pip install --python "regex:${HOME}/\\.local/share/omni/wd/[^ /]+/python/3.12.3/root/bin/python" -r "${PWD}/requirements2.txt"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"installing dependencies from requirements.txt"* ]]
  [[ "${output}" == *"installing dependencies from requirements2.txt"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=05] omni up python operation (latest) using brew for dependencies (already installed)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_mise_python_calls installed=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"using python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 already installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=06] omni up python operation (latest) using brew for dependencies (already installed + other versions)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_mise_python_calls installed=true others_installed="3.11.6,3.11.8"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"using python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 already installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=07] omni up python operation (latest) using brew for dependencies (plugin already installed)" {
  cat > .omni.yaml <<EOF
up:
  - python:
      url: https://example.com/fake/plugin
EOF

  add_brew_python_calls
  add_mise_python_calls plugin_name=python-7d945a08 plugin_list=installed mise_registry=false mise_env=false plugin_update=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=08] omni up python operation (latest) using brew for dependencies (install fail fallback to matching installed version)" {
  cat > .omni.yaml <<EOF
up:
  - python:
      version: 3.11
      upgrade: true
EOF

  add_brew_python_calls
  add_mise_python_calls installed=fail version=3.11.9 upgrade=true others_installed="3.11.6,3.11.8" fallback_version=3.11.8

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.11.9"* ]]
  [[ "${output}" == *"falling back to installed version 3.11.8"* ]]
  [[ "${output}" == *"python 3.11.8 already installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=09] omni up python operation (latest) using brew for dependencies (cache versions expired but plugin update fail)" {
  cat > .omni.yaml <<EOF
up:
  - python:
      url: https://example.com/fake/plugin
EOF

  add_brew_python_calls
  add_mise_python_calls plugin_name=python-7d945a08 plugin_list=installed cache_versions=expired list_versions=fail-update mise_registry=false mise_env=false plugin_update=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=10] omni up python operation (latest) using brew for dependencies (cache versions hit)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_mise_python_calls cache_versions=true list_versions=false

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=11] omni up python operation (latest) using brew for dependencies (cache versions expired)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_mise_python_calls cache_versions=expired

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=12] omni up python operation (latest) using brew for dependencies (cache versions expired but list versions fail)" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_mise_python_calls cache_versions=expired list_versions=fail

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=13] omni up python operation (latest) with upgrade configured for the python version" {
  cat > .omni.yaml <<EOF
up:
  - python:
      upgrade: true
EOF

  add_brew_python_calls
  add_mise_python_calls upgrade=true others_installed="3.11.6,3.11.8"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=14] omni up python operation (latest) with upgrade configured at the work directory level" {
  cat > .omni.yaml <<EOF
up:
  - python

up_command:
  upgrade: true
EOF

  add_brew_python_calls
  add_mise_python_calls upgrade=true others_installed="3.11.6,3.11.8"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=15] omni up python operation (latest) with upgrade configured as a command-line parameter" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_mise_python_calls upgrade=true others_installed="3.11.6,3.11.8"

  run omni up --trust --upgrade 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=16] omni up python operation (latest) with upgrade disabled and only an older major installed" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  # Expect that it won't match latest since older major, and install is required
  add_mise_python_calls others_installed="2.7.18"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=17] omni up python operation (2) with upgrade disabled and a version 2 installed" {
  cat > .omni.yaml <<EOF
up:
  - python: 2
EOF

  add_brew_python_calls
  add_mise_python_calls installed=true list_versions=false version="2.7.9" others_installed="2.7.9" venv=false

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"using python 2.7.9"* ]]
  [[ "${output}" == *"python 2.7.9 already installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=18] omni up python operation (latest) with upgrade disabled and the current major installed" {
  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_mise_python_calls installed=true version="3.7.1" others_installed="3.7.1"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"using python 3.7.1"* ]]
  [[ "${output}" == *"python 3.7.1 already installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=19] omni up python operation (3) with upgrade disabled and a version 3 installed" {
  cat > .omni.yaml <<EOF
up:
  - python: 3
EOF

  add_brew_python_calls
  add_mise_python_calls installed=true list_versions=false version="3.7.1" others_installed="3.7.1"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"using python 3.7.1"* ]]
  [[ "${output}" == *"python 3.7.1 already installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=20] omni up python operation (*) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: "*"
EOF

  add_brew_python_calls
  add_mise_python_calls

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.12.3"* ]]
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=21] omni up python operation (3.11.6) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: 3.11.6
EOF

  add_brew_python_calls
  add_mise_python_calls version=3.11.6

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.11.6"* ]]
  [[ "${output}" == *"python 3.11.6 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=22] omni up python operation (>=3.10, <3.11) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: ">=3.10, <3.11"
EOF

  add_brew_python_calls
  add_mise_python_calls version=3.10.14

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.10.14"* ]]
  [[ "${output}" == *"python 3.10.14 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=23] omni up python operation (2.6.x || >3.10.12 <=3.11.2) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: "2.6.x || >3.10.12 <=3.11.2"
EOF

  add_brew_python_calls
  add_mise_python_calls version=3.11.2

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.11.2"* ]]
  [[ "${output}" == *"python 3.11.2 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=24] omni up python operation (~3.11.6) using brew for dependencies" {
  cat > .omni.yaml <<EOF
    up:
    - python: "~3.11.6"
EOF

  add_brew_python_calls
  add_mise_python_calls version=3.11.9

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.11.9"* ]]
  [[ "${output}" == *"python 3.11.9 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=25] omni up python operation (3.11.x) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: 3.11.x
EOF

  add_brew_python_calls
  add_mise_python_calls version=3.11.9

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.11.9"* ]]
  [[ "${output}" == *"python 3.11.9 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=26] omni up python operation (3.11) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: 3.11
EOF

  add_brew_python_calls
  add_mise_python_calls version=3.11.9

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.11.9"* ]]
  [[ "${output}" == *"python 3.11.9 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=27] omni up python operation (2) using brew for dependencies (install fail does not fallback when no matching version installed)" {
  cat > .omni.yaml <<EOF
up:
  - python: 2
EOF

  add_brew_python_calls
  add_mise_python_calls version=2.7.18 venv=false installed=fail others_installed="3.11.6,3.11.8" mise_env=false

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 2.7.18"* ]]
  [[ "${output}" == *"execution error: process exited with status 1"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=28] omni up python operation (2) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: 2
EOF

  add_brew_python_calls
  add_mise_python_calls version=2.7.18 venv=false

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 2.7.18"* ]]
  [[ "${output}" == *"python 2.7.18 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:brew
@test "[omni_up_python=29] omni up python operation (^2.5.2) using brew for dependencies" {
  cat > .omni.yaml <<EOF
up:
  - python: "^2.5.2"
EOF

  add_brew_python_calls
  add_mise_python_calls version=2.7.18 venv=false

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 2.7.18"* ]]
  [[ "${output}" == *"python 2.7.18 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,omni:up:python:nix
@test "[omni_up_python=30] omni up python operation (latest) using nix for dependencies" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  preferred_tools:
  - nix
EOF

  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_nix_python_calls
  add_mise_python_calls

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
}

# bats test_tags=omni:up,omni:up:python
@test "[omni_up_python=31] omni up python operation (auto) with only the root directory" {
  cat > .omni.yaml <<EOF
up:
  - python: auto
EOF

  echo "3.11.9" > .python-version

  add_brew_python_calls
  add_mise_python_calls version=3.11.9 mise_where=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.11.9"* ]]
  [[ "${output}" == *"python 3.11.9 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python
@test "[omni_up_python=32] omni up python operation (auto) with only a subdirectory" {
  cat > .omni.yaml <<EOF
up:
  - python: auto
EOF

  mkdir subdir
  echo "3.11.9" > subdir/.python-version

  add_brew_python_calls
  add_mise_python_calls version=3.11.9 subdir=true mise_where=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.11.9"* ]]
  [[ "${output}" == *"python 3.11.9 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python
@test "[omni_up_python=33] omni up python operation (auto) with multiple directories" {
  cat > .omni.yaml <<EOF
up:
  - python: auto
EOF

  echo "3.12.0" > .python-version
  mkdir subdir1
  echo "3.11.7" > subdir1/.python-version
  mkdir subdir2
  echo "python 3.11.9" > subdir2/.tool-versions

  add_brew_python_calls
  add_mise_python_calls version=3.11.7 subdir=true auto=true
  add_mise_python_calls version=3.11.9 subdir=true list_versions=false mise_update=false mise_registry=false plugin_list=false
  add_mise_python_calls version=3.12.0 list_versions=false mise_update=false mise_registry=false plugin_list=false mise_where=true

  touch requirements.txt subdir1/requirements.txt subdir2/requirements.txt
  add_command uv pip install --python "regex:${HOME}/\\.local/share/omni/wd/[^ /]+/python/3.11.7/[^ /]+/bin/python" -r "${PWD}/subdir1/requirements.txt"
  add_command uv pip install --python "regex:${HOME}/\\.local/share/omni/wd/[^ /]+/python/3.11.9/[^ /]+/bin/python" -r "${PWD}/subdir2/requirements.txt"
  add_command uv pip install --python "regex:${HOME}/\\.local/share/omni/wd/[^ /]+/python/3.12.0/root/bin/python" -r "${PWD}/requirements.txt"

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right version was installed
  [[ "${output}" == *"installing python 3.11.7"* ]]
  [[ "${output}" == *"installing python 3.11.9"* ]]
  [[ "${output}" == *"installing python 3.12.0"* ]]
  [[ "${output}" == *"python 3.11.7, 3.11.9, 3.12.0 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,supply-chain
@test "[omni_up_python=34] omni up python operation fails when mise is disabled through the supply chain" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    allowed:
      - '!mise'
EOF

  cat > .omni.yaml <<EOF
up:
  - python
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"mise operations (python) are not allowed"* ]]
}

# bats test_tags=omni:up,omni:up:python,supply-chain
@test "[omni_up_python=35] omni up python operation fails when a specific url is disabled through the supply chain (global)" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    sources:
      - '!https://example.com/fake/plugin'
EOF

  cat > .omni.yaml <<EOF
up:
  - python:
      url: https://example.com/fake/plugin
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"cannot use URL https://example.com/fake/plugin as a source for tool installations"* ]]
}

# bats test_tags=omni:up,omni:up:python,supply-chain
@test "[omni_up_python=36] omni up python operation fails when a specific url is disabled through the supply chain (global, wildcard)" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    sources:
      - '!https://example.com/fake/*'
EOF

  cat > .omni.yaml <<EOF
up:
  - python:
      url: https://example.com/fake/plugin
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"cannot use URL https://example.com/fake/plugin as a source for tool installations"* ]]
}

# bats test_tags=omni:up,omni:up:python,supply-chain
@test "[omni_up_python=37] omni up python operation fails when a specific url is disabled through the supply chain (mise)" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    mise:
      sources:
        - '!https://example.com/fake/plugin'
EOF

  cat > .omni.yaml <<EOF
up:
  - python:
      url: https://example.com/fake/plugin
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"cannot use URL https://example.com/fake/plugin as a source for tool installations"* ]]
}

# bats test_tags=omni:up,omni:up:python,supply-chain
@test "[omni_up_python=38] omni up python operation fails when a specific backend is disabled through the supply chain (no alternative)" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    mise:
      backends:
        - '!core'
EOF

  cat > .omni.yaml <<EOF
up:
  - python
EOF

  # No alternate to the core plugin
  add_command mise registry <<EOF
python  core:python
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"unable to resolve tool: python"* ]]
}

# bats test_tags=omni:up,omni:up:python,supply-chain
@test "[omni_up_python=39] omni up python operation succeeds when a specific backend is disabled through the supply chain (alternative)" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    mise:
      backends:
        - '!core'
EOF

  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_mise_python_calls mise_registry_alt="asdf:alt/python" plugin_name="asdf-python-5c9935a9" plugin_list=installed plugin_update=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right python version was installed
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,supply-chain
@test "[omni_up_python=40] omni up python operation fails when a specific backend is allow-listed through the supply chain (backend not available)" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    mise:
      backends:
        - asdf
EOF

  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_command mise registry <<EOF
python  core:python
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"unable to resolve tool: python"* ]]
}

# bats test_tags=omni:up,omni:up:python,supply-chain
@test "[omni_up_python=41] omni up python operation succeeds when a specific backend is allow-listed through the supply chain (backend available)" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    mise:
      backends:
        - asdf
EOF

  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_brew_python_calls
  add_mise_python_calls mise_registry_alt="asdf:alt/python" plugin_name="asdf-python-5c9935a9" plugin_list=installed plugin_update=true

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]

  # Check that the right python version was installed
  [[ "${output}" == *"python 3.12.3 installed"* ]]
}

# bats test_tags=omni:up,omni:up:python,supply-chain
@test "[omni_up_python=42] omni up python operation fails when a specific backend is allow-listed through the supply chain but the URL is deny-listed" {
  cat >> ~/.config/omni/config.yaml <<EOF
up_command:
  operations:
    sources:
      - '!https://github.com/alt/python'
    mise:
      backends:
        - asdf
EOF

  cat > .omni.yaml <<EOF
up:
  - python
EOF

  add_command mise registry <<EOF
python  core:python asdf:alt/python
EOF

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 1 ]

  # Check that the right error was emitted
  [[ "${output}" == *"unable to resolve tool: python"* ]]
}

# These tests use real uv interpreter discovery and wheel installation. Run with
# OMNI_TEST_UV_BIN=/absolute/path/to/uv (or install uv on PATH). Everything else
# uses the usual fixtures; wheel resolution and installation are offline.
setup_real_uv_python() {
  local real_uv="${OMNI_TEST_UV_BIN:-$(command -v uv || true)}"
  [ -n "$real_uv" ] || skip "real uv required (set OMNI_TEST_UV_BIN)"
  local real_python="${OMNI_TEST_PYTHON_BIN:-$(command -v python3 || true)}"
  [ -n "$real_python" ] || skip "python3 required"

  rm "$uv_bin"
  ln -s "$real_uv" "$uv_bin"
  add_brew_python_calls
  add_mise_python_calls venv=false
  ln -s "$real_python" "$(mise_tool_path python 3.12.3)/bin/python"

  export UV_CACHE_DIR="$HOME/uv-cache"
  export UV_OFFLINE=true
  export UV_NO_INDEX=true
  export UV_FIND_LINKS="$HOME/wheels"
  mkdir -p "$UV_FIND_LINKS"
  "$real_python" - "$UV_FIND_LINKS" <<'PYCODE'
import pathlib
import sys
import zipfile

# A minimal seed wheel lets uv venv --seed run offline, including on Python
# versions where uv also seeds setuptools and wheel. The probe is installed by
# the requirements step, so its location identifies the installation target.
for name, version in [("pip", "99.0"), ("setuptools", "99.0"),
                      ("wheel", "99.0"), ("omni_venv_probe", "1.0")]:
    dist = f"{name}-{version}.dist-info"
    files = {
        f"{name}/__init__.py": f'__version__ = "{version}"\n',
        f"{dist}/METADATA": f"Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n",
        f"{dist}/WHEEL": "Wheel-Version: 1.0\nGenerator: omni-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    }
    files[f"{dist}/RECORD"] = "".join(f"{path},,\n" for path in files)
    with zipfile.ZipFile(pathlib.Path(sys.argv[1]) / f"{name}-{version}-py3-none-any.whl", "w") as wheel:
        for path, content in files.items():
            wheel.writestr(path, content)
PYCODE

  cat > .omni.yaml <<EOF
up:
  - python:
      version: 3.12.3
      pip: requirements.txt
EOF
  echo 'omni-venv-probe==1.0' > requirements.txt

  "$real_uv" venv --python "$real_python" --no-python-downloads "$HOME/unrelated"
  export VIRTUAL_ENV="$HOME/unrelated"
  # uv also supports an interpreter request in the environment. It must not
  # override Omni's managed interpreter, even if dynamic activation succeeds.
  export UV_PYTHON="$HOME/unrelated/bin/python"
}

reset_python_up_calls() {
  check_commands
  # Reset operation caches to make the second run use the same dependency
  # fixtures, while preserving the actual Python environments on disk.
  rm -rf "$HOME/.cache/omni" "$HOME/.commands"
  setup_uv_cache
  add_brew_python_calls
  add_mise_python_calls installed=true list_versions=false venv=false
}

assert_managed_python_dependency() {
  local managed_cfg
  managed_cfg="$(find "$HOME/.local/share/omni/wd" -path '*/python/3.12.3/root/pyvenv.cfg')"
  [ -n "$managed_cfg" ]
  local managed_venv="${managed_cfg%/pyvenv.cfg}"
  "$managed_venv/bin/python" -c 'import omni_venv_probe'
  run "$HOME/unrelated/bin/python" -c 'import omni_venv_probe'
  [ "$status" -ne 0 ]
}

# bats test_tags=omni:up,omni:up:python
@test "[omni_up_python=43] managed dependency target with real uv (fresh)" {
  setup_real_uv_python

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
  assert_managed_python_dependency
}

# bats test_tags=omni:up,omni:up:python
@test "[omni_up_python=44] managed dependency target with real uv (reused without seed failure)" {
  setup_real_uv_python
  # Create a complete environment without installing the probe, then reuse it.
  echo '' > requirements.txt
  run omni up --trust 3>&-
  echo "FIRST OUTPUT: $output"
  [ "$status" -eq 0 ]
  echo 'omni-venv-probe==1.0' > requirements.txt
  reset_python_up_calls

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"venv already exists"* ]]
  assert_managed_python_dependency
}

# bats test_tags=omni:up,omni:up:python
@test "[omni_up_python=45] managed dependency target with real uv (retry after failed seeding)" {
  setup_real_uv_python
  local wheels="$UV_FIND_LINKS"
  export UV_FIND_LINKS="$HOME/empty-index"
  mkdir -p "$UV_FIND_LINKS"

  run omni up --trust 3>&-
  echo "FIRST OUTPUT: $output"
  [ "$status" -ne 0 ]
  # A failed uv seed leaves a usable venv; neither pip nor the probe is present.
  local managed_cfg
  managed_cfg="$(find "$HOME/.local/share/omni/wd" -name pyvenv.cfg)"
  [ -n "$managed_cfg" ]
  "${managed_cfg%/pyvenv.cfg}/bin/python" -c 'import importlib.util, sys; assert sys.prefix != sys.base_prefix; assert importlib.util.find_spec("pip") is None; assert importlib.util.find_spec("omni_venv_probe") is None'
  export UV_FIND_LINKS="$wheels"
  reset_python_up_calls

  run omni up --trust 3>&-
  echo "STATUS: $status"
  echo "OUTPUT: $output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"venv already exists"* ]]
  assert_managed_python_dependency
}
