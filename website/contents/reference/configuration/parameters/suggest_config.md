---
description: Configuration of the `suggest_config` parameter
---

# `suggest_config`

:::info
This parameter can only be used inside of a git repository. Any global configuration for that parameter will be ignored.
:::

Configuration that a git repository suggests should be added to the user configuration, this is picked up when calling `omni up --update-user-config` or when this command is directly called by `omni clone`.

## Parameters

This can contain any value otherwise available in the configuration.

### Template

The `suggest_config` parameter can be templated. The template needs to resolve to a valid `suggest_config` value. You can template this parameter by using the following parameters:

| Parameter | Type | Description |
|-----------|------|-------------|
| `template_file` | string | The path to the file containing the template to use. The path is relative to the root of the work directory. |
| `template` | string | The template to use. |

## Configuration merging strategies

You can use merging strategies to better suggest configuration changes, by appending `__<strategy>` at the end of the key for which you are making a suggestion.

The following configuration merging strategies can be used:

| Strategy | Description |
|----------|-------------|
| `toappend` | Can be used to append to an existing list |
| `toprepend` | Can be used to prepend to an existing list |
| `toreplace` | Can be used to replace an existing value |
| `ifnone` | Can be used to only be considered if no value already exists |

## Examples

```yaml
# To suggest appending a value to the `path/append` configuration of the user
suggest_config:
  path:
    append__toappend:
      - path

# To prepend a value to the list of organizations of the user
suggest_config:
  org__toprepend:
    - handle: git@github.com:xaf/omni
      trusted: true

# We can also template the suggest_config parameter using a template file
suggest_config:
  template_file: .omni/suggest_config.tmpl

# Or template the suggest_config parameter using a template string
suggest_config:
  template: |
    path:
      append__toappend:
        - special/commands/{{ prompts.team }}
    {% if prompts.team == "team1" %}
    org_toprepend:
    - handle: {{ partial_resolve(handle="team1-private") }}
      trusted: true
    {% endif %}
```
