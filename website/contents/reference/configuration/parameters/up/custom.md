---
description: Configuration of the `custom` kind of `up` parameter
---

# `custom` operation

A custom command to be executed as a step.

:::info
Any of the commands executed will be passed to `bash` for execution.
:::

## Parameters

The following parameters can be used:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `name` | string | the name of the step to be shown when `omni up` or `omni down` are being run |
| `meet` | multiline string | the command to run to meet the requirement during set up |
| `met?` | multiline string | the command to run to know if we are currently meeting the requirement |
| `unmeet` | multiline string | the command to run to 'unmeet' the requirement during tear down |
| `dir` | path | Relative path to the directory the custom operation needs to be run from. The dynamic environment of that directory will be loaded before any of the executions for the operation. Defaults to the work dir root. |

## Environment variables

The following environment variables are available:

| Variable | Description |
|----------|-------------|
| `$OMNI_ENV` | The path to a file that can be used to manipulate the dynamic environment being built for the work directory. See the section below for more information. |
| `$PREFIX` | The path to the directory where artifacts can be stored for the custom operation. If they exist, the `$PREFIX/bin` directory is automatically added to the `PATH` environment variable, and the `$PREFIX/lib` directory is automatically added to the `LD_LIBRARY_PATH` environment variable. |

### The `$OMNI_ENV` environment variable

When running the `meet` step of a `custom` operation, the `$OMNI_ENV` environment variable is set to the path of a file that can be used to manipulate the dynamic environment being built for the work directory. When writing to this file, specific patterns must be followed to ensure the dynamic environment is correctly built.

| Pattern | Description |
|---------|-------------|
| `VAR=value` | Sets the environment variable `VAR` to `value` |
| `unset VAR` | Unsets the environment variable `VAR` |
| `VAR<<EOF` | Starts a multiline value for the environment variable `VAR`. The value will be read until `EOF` is found on a line by itself. |
| `VAR<<-EOF` | Same as `VAR<<EOF`, but all leading tabs and spaces will be removed from the multiline value. |
| `VAR<<~EOF` | Same as `VAR<<EOF`, but the minimum indentation of the multiline value will be removed from all lines. |
| `VAR<<=value` | Prepends `value` to the path-like environment variable `VAR`; i.e. `VAR` will be set to `value:$VAR`. |
| `VAR>>=value` | Appends `value` to the path-like environment variable `VAR`; i.e. `VAR` will be set to `$VAR:value`. |
| `VAR-=value` | Removes `value` from the path-like environment variable `VAR`; i.e. if `VAR` is set to `otherval:value:someval`, `VAR` will be set to `otherval:someval`. |
| `VAR<=value` | Adds `value` as a prefix to `VAR`; i.e. if `VAR` is set to `someval`, `VAR` will be set to `valuesomeval`. |
| `VAR>=value` | Adds `value` as a suffix to `VAR`; i.e. if `VAR` is set to `someval`, `VAR` will be set to `somevalvalue`. |

:::tip

It is recommended to prefer the advanced patterns (e.g. `>>=`, `<<=`, `-=`, `>=`, `<=`) as the environment variable manipulation will happen when the dynamic environment is loaded for the work directory, making those changes relative to the current environment being changed.

When using the `VAR=value` pattern, the value of `VAR` will be set to `value` regardless of the current value of `VAR`. If using `echo PATH=$PATH:/some/path >"$OMNI_ENV`", the value of `$PATH` will be calculated at the time of `omni up` and the resulting value will be fixed no matter future changes in the environment of the user.

:::

## Examples

```yaml
up:
  # Simple command for which the meet operation will be run
  # each time `omni up` is called
  - custom:
      name: Printing hello
      meet: echo "hello"

  # Now we say goodbye during `omni down`, but we don't do
  # anything during `omni up`
  - custom:
      name: Saying goodbye
      unmeet: echo "goodbye"

  # Let's say both
  - custom:
      name: Greetings
      meet: echo "hello"
      unmeet: echo "goodbye"

  # But now we wanna say hello only if we haven't said it yet
  # and we wanna say goodbye only if we said hello before
  - custom:
      name: Proper greetings
      met?: test -f /tmp/did_greet
      meet: touch /tmp/did_greet && echo "hello"
      unmeet: rm /tmp/did_greet && echo "goodbye"

  # Set the environment variable `HELLO` to `world`
  - custom:
      name: Setting HELLO
      meet: echo "HELLO=world" >"$OMNI_ENV"

  # Set a multiline environment variable
  - custom:
      name: Setting multiline
      meet: |
        echo "MULTILINE<<-EOF" >"$OMNI_ENV"
        echo "  line1" >"$OMNI_ENV"
        echo "  line2" >"$OMNI_ENV"
        echo "EOF" >"$OMNI_ENV"

  # Set a path-like environment variable
  - custom:
      name: Setting PATH
      meet: |
        # Append to the PATH environment variable
        echo "PATH>>=/some/path" >"$OMNI_ENV"
        # Prepend to the PATH environment variable
        echo "PATH<=/some/other/path" >"$OMNI_ENV"
        # Remove from the PATH environment variable
        echo "PATH-=/some/older/path" >"$OMNI_ENV"

  # Do other environment variable manipulations
  - custom:
      name: Other environment variable manipulations
      meet: |
        # Add a prefix to the MYVAR environment variable
        echo "MYVAR<=prefix" >"$OMNI_ENV"
        # Add a suffix to the MYVAR environment variable
        echo "MYVAR>=suffix" >"$OMNI_ENV"
        # Unset the HELLO environment variable
        echo "unset HELLO" >"$OMNI_ENV"
```
