#!/usr/bin/env bash

mise_tool_path() {
  local tool=$1
  local version=$2
  if [[ -z "${tool}" ]] || [[ -z "${version}" ]]; then
    echo "Tool and version should be specified as the first and second arguments when calling mise_tool_bin_path"
    return 1
  fi

  echo "${HOME}/.local/share/omni/mise/installs/${tool}/${version}"
}

mise_tool_latest_version() {
  local tool=$1
  if [[ -z "${tool}" ]]; then
    echo "Tool should be specified as the first argument when calling mise_tool_latest_version"
    return 1
  fi

  # Try to grab the latest version from the versions file
  latest_version=$(perl -ne '$last = $_ if /^\d+(\.\d+)*$/; END{print $last if $last}' \
                   "${PROJECT_DIR}/tests/fixtures/${tool}-versions.txt")

  if [ -z "${latest_version}" ]; then
    echo "Latest version for ${tool} could not be determined"
    return 1
  fi

  echo "${latest_version}"
}

setup_cache_db() {
  local cache_db="${HOME}/.cache/omni/cache.db"
  if [[ -f "${cache_db}" ]]; then
    # DB already exists, no need to recreate it
    return
  fi

  # Create the cache directory
  mkdir -p "${HOME}/.cache/omni"

  # Create the sqlite file and apply the schema and upgrades
  local sql_dir="${PROJECT_DIR}/src/internal/cache/database/sql"
  local ordered_upgrades=$(ls -1 "${sql_dir}" 2>/dev/null | grep '^upgrade_' | sort -V)
  sqlite3 "$cache_db" <"${sql_dir}/create_tables.sql"
  for upgrade_script in $ordered_upgrades; do
    local upgrade_script="${sql_dir}/${upgrade_script}"
    if [ -f "${upgrade_script}" ]; then
      sqlite3 "$cache_db" <"${upgrade_script}"
    fi
  done
}

add_mise_tool_calls() {
  local tool=
  local latest_version=
  local version="latest"
  local fallback_version=
  local plugin_list=false
  local plugin_name=
  local plugin_update=false
  local installed=false
  local others_installed=false
  local venv=false
  local cache_versions=false
  local list_versions=true
  local upgrade=false
  local subdir=false
  local mise_update=true
  local mise_registry=true
  local mise_registry_alt=
  local mise_env=true
  local mise_where=false
  local auto=false

  for arg in "$@"; do
    case $arg in
      tool=*)
	tool="${arg#tool=}"
	shift
	;;
      latest_version=*)
	latest_version="${arg#latest_version=}"
	shift
	;;
      version=*)
        version="${arg#version=}"
        shift
        ;;
      fallback_version=*)
        fallback_version="${arg#fallback_version=}"
        shift
        ;;
      plugin_name=*)
        plugin_name="${arg#plugin_name=}"
        shift
        ;;
      plugin_list=*)
        plugin_list="${arg#plugin_list=}"
        shift
        ;;
      plugin_update=*)
        plugin_update="${arg#plugin_update=}"
        shift
        ;;
      installed=*)
        installed="${arg#installed=}"
        shift
        ;;
      others_installed=*)
        others_installed="${arg#others_installed=}"
        shift
        ;;
      venv=*)
        venv="${arg#venv=}"
        shift
        ;;
      cache_versions=*)
        cache_versions="${arg#cache_versions=}"
        shift
        ;;
      list_versions=*)
        list_versions="${arg#list_versions=}"
        shift
        ;;
      upgrade=*)
        upgrade="${arg#upgrade=}"
        shift
        ;;
      subdir=*)
        subdir="${arg#subdir=}"
        shift
        ;;
      mise_update=*)
        mise_update="${arg#mise_update=}"
        shift
        ;;
      mise_registry=*)
        mise_registry="${arg#mise_registry=}"
        shift
        ;;
      mise_registry_alt=*)
        mise_registry_alt="${arg#mise_registry_alt=}"
        shift
        ;;
      mise_env=*)
        mise_env="${arg#mise_env=}"
        shift
        ;;
      mise_where=*)
        mise_where="${arg#mise_where=}"
        shift
        ;;
      auto=*)
        auto="${arg#auto=}"
        shift
        ;;
      *)
        echo "Unknown argument: $arg"
        return 1
        ;;
    esac
  done

  if [ -z "${tool}" ]; then
    echo "Tool should be specified using tool={tool_name} when calling add_mise_tool_calls"
    return 1
  fi

  if [ -z "${plugin_name}" ]; then
    plugin_name="${tool}"
  fi

  normalized_plugin_name=$(echo "${plugin_name}" | sed 's/[^a-zA-Z0-9_-]/_/g' | tr '[:upper:]' '[:lower:]')

  if [ "$version" = "latest" ] || [ "$version" = "*" ]; then
    if [[ -z "${latest_version}" ]]; then
      latest_version=$(mise_tool_latest_version "${tool}")
    fi
    version="${latest_version}"
  fi

  if [ "$cache_versions" = "true" ] || [ "$cache_versions" = "expired" ]; then
    local date
    if [ "$cache_versions" = "true" ]; then
      date=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    else
      # This allows to support GNU date but also BSD date
      date=$(date -u +"%Y-%m-%dT%H:%M:%SZ" -d "300 days ago" 2>/dev/null ||
             date -u -v-300d +"%Y-%m-%dT%H:%M:%SZ")
    fi

    local cache_db="${HOME}/.cache/omni/cache.db"
    setup_cache_db

    # Format the tool versions to insert in the cache
    local versions_list="${FIXTURES_DIR}/${tool}-versions.txt"
    local cached_versions=$(cat "${versions_list}" | \
      jq --raw-input | \
      jq --slurp --compact-output)
    sqlite3 "$cache_db" \
      "INSERT INTO mise_plugins (plugin_name, updated_at, versions, versions_fetched_at)
       VALUES ('${plugin_name}', '${date}', '${cached_versions}', '${date}')"
  fi

  # List of objects in the shape { version: string }
  local installed_versions='[]'
  if [ "$installed" = "true" ]; then
    installed_versions=$(echo "$installed_versions" | jq \
      --compact-output \
      --arg version "$version" \
      '. += [{"version": $version}]')
  fi
  if [ "$others_installed" != "false" ]; then
    # others_installed is a comma-separated list of versions, and
    # we need to append each individual version to the list
    IFS=',' read -ra version_array <<< "$others_installed"
    for other_version in "${version_array[@]}"; do
      installed_versions=$(echo "$installed_versions" | jq \
        --compact-output \
        --arg version "$other_version" \
        '. += [{"version": $version}]')
    done
  fi
  installed_versions=$(echo "$installed_versions" | jq \
    --compact-output \
    'sort_by(.version | split(".") | map(tonumber))')


  if [ "$mise_registry" = "true" ]; then
    # Checking the mise registry to check the available plugins
    # and their fully qualified plugin name for each of the mise
    # backends they are available in
    add_command mise registry <<EOF
${tool}  core:${tool}${mise_registry_alt:+ $mise_registry_alt}
EOF
  fi

  if [ "$mise_update" = "true" ]; then
    # Checking the version of mise, which allows to decide if mise
    # should be updated or not
    add_command mise --version
  fi

  if [ "$auto" = "true" ]; then
    # This is used to check using mise if there are specified versions needing
    # to be installed for this directory
    add_command mise ls --current --offline --json --quiet ${plugin_name}
  fi

  if [ "$plugin_list" = "true" ]; then
    add_command mise plugins ls
    add_command mise plugins install "${plugin_name}" "regex:https?://.*"
  elif [ "$plugin_list" = "installed" ]; then
    add_command mise plugins ls <<EOF
${plugin_name}
EOF
  fi

  if [ "$upgrade" = "false" ]; then
    # list_installed_versions_from_plugin - to refresh the installed versions
    add_command mise ls --installed --offline --json --quiet ${plugin_name} <<< "${installed_versions}"
  fi

  # If the plugin is a url-specified plugin, we expect an update
  if [ "${plugin_update}" = "true" ]; then
    if [ "$list_versions" = "fail-update" ]; then
      add_command mise plugins update ${plugin_name} exit=1
    else
      add_command mise plugins update ${plugin_name}
    fi
  fi

  # Listing the available versions
  if [ "$list_versions" = "true" ]; then
    add_command mise ls-remote ${plugin_name} <"${PROJECT_DIR}/tests/fixtures/${tool}-versions.txt"
  elif [ "$list_versions" = "fail-update" ]; then
    : # Do nothing
  elif [ "$list_versions" = "fail" ]; then
    add_command mise ls-remote ${plugin_name} exit=1
  fi

  # is_mise_tool_version_installed - to check if the version is installed
  add_command mise ls --installed --offline --json --quiet ${plugin_name} <<< "${installed_versions}"

  # Installing the requested version
  if [ "$installed" = "false" ]; then
    add_command mise install "${plugin_name}@${version}"
  elif [ "$installed" = "fail" ]; then
    add_command mise install "${plugin_name}@${version}" exit=1 <<EOF
stderr:Error installing ${tool} ${version}
EOF

    # list_installed_versions_from_plugin - to find the fallback version
    add_command mise ls --installed --offline --json --quiet ${plugin_name} <<< "${installed_versions}"

    if [ -n "${fallback_version}" ]; then
      # Replace version by the fallback version here!
      version="${fallback_version}"

      # is_mise_tool_version_installed - to check if the fallback version is installed
      add_command mise ls --installed --offline --json --quiet ${plugin_name} <<< "${installed_versions}"
    fi
  fi

  local bin_path="${HOME}/.local/share/omni/mise/installs/${normalized_plugin_name}/${version}/bin"
  if [ "$mise_env" = "true" ]; then
    mkdir -p "${bin_path}"
    # Identify the location of the binaries for the tool
    add_command mise env --json "${plugin_name}@${version}" <<EOF
{
  "PATH": "${bin_path}:"
}
EOF
  fi

  if [ "$mise_where" = "true" ]; then
    # Identify the normalized path for the tool, this call does not
    # use the version as we just try to resolve the tool path
    add_command mise ls --installed --offline --json --quiet ${plugin_name} <<EOF
[
  {
    "version": "7.8.9",
    "install_path": "${HOME}/.local/share/omni/mise/installs/${normalized_plugin_name}/7.8.9",
    "installed": true,
    "active": false
  }
]
EOF
  fi

  if [ "$venv" = "true" ]; then
    add_fakebin "${bin_path}/${tool}"
    sub=root
    if [ "$subdir" = "true" ]; then
      sub="[^ /]+"
    fi
    add_command uv "venv" "--seed" "--python" "${bin_path}/python" "--no-python-downloads" "regex:${HOME}/\.local/share/omni/wd/[^ /]+/${normalized_plugin_name}/${version}/${sub}"
  fi
}

add_mise_tool_brew_calls() {
  local tool=$1
  if [[ -z "${tool}" ]]; then
    echo "Tool should be specified as the first argument when calling add_mise_tool_brew_calls"
    return 1
  fi

  formulas=(autoconf coreutils curl libyaml openssl@3 readline)
  if [[ "${tool}" == "python" ]]; then
    formulas+=(pkg-config)
  fi

  local checked_prefix=false
  for formula in "${formulas[@]}"; do
    add_command brew list --formula "${formula}" exit=1
    add_command brew install --formula "${formula}"
    add_command brew --prefix --installed "${formula}"
    if [ "$checked_prefix" = false ]; then
      checked_prefix=true
      add_command brew --prefix
    fi
  done
}

