# Live Manual `FromContextValue` Implementations

This inventory is derived from the uncommented `feuilletage::FromContextValue`
implementations under `src/` at the current branch tip. There are exactly
**17** manual implementations.

## Inventory

| # | Type | Source | Reason for the current manual implementation |
|---:|---|---|---|
| 1 | `UpConfigBash` | `src/internal/config/up/tool.rs` | Parses the shared `UpConfigMise` shape, then injects Bash-specific state: the plugin URL, requested tool name, and tag post-processing. |
| 2 | `UpConfigNodejs` | `src/internal/config/up/nodejs.rs` | Builds a shared mise backend, injects the `node` tool identity and Node-specific detection/post-install callbacks, retains the original config value, and separately parses object-only Node parameters. |
| 3 | `UpConfigPython` | `src/internal/config/up/python.rs` | Builds a shared mise backend, injects the `python` tool identity and Python-specific detection/post-install callbacks, retains the original config value, and separately parses object-only Python parameters. |
| 4 | `AssetNameMatcher` | `src/internal/config/up/github_release.rs` | Accepts a string, string array, or `{ os, arch, patterns }` object; splits multiline patterns and computes the runtime `disabled` flag from the current OS and architecture. |
| 5 | `CommandSyntax` | `src/internal/config/parser/command_definition.rs` | Preserves the command-syntax grammar: array/object/scalar forms, six parameter-key aliases with alias-dependent `required` defaults, compact keyed parameters, groups, range syntax, and nested argument options. |
| 6 | `UpConfigHomebrew` | `src/internal/config/up/homebrew.rs` | Coordinates Homebrew's flexible install/tap forms, distinguishes formulae from casks, derives implicit taps from install names, and deduplicates taps. |
| 7 | `UpConfig` | `src/internal/config/up/base.rs` | Parses a heterogeneous step array, normalizes bare tool names, treats numeric YAML values as mise tool names, detects empty operations, and collects both Feuilletage diagnostics and omni `UpError` values while continuing. |
| 8 | `EnvConfig` | `src/internal/config/parser/env.rs` | Normalizes map or array input into derived `EnvVarConfig` entries, then expands each entry into one or more ordered `EnvOperationConfig` values with path resolution. |
| 9 | `EnvOpValue` | `src/internal/config/parser/env.rs` | Parses each operation operand from null, scalar, or `{ value, type }`, coercing scalar values and validating the `text`/`path` type discriminator. |
| 10 | `StoredConfig` | `src/internal/config/parser/suggest_config.rs` | Intentionally accepts any value and strips its context into a context-free `feuilletage::Value`. |
| 11 | `SuggestConfig` | `src/internal/config/parser/suggest_config.rs` | Applies ordered key-presence handling for `config`, `template`, and `template_file`; values without those recognized forms are stored whole as arbitrary config. |
| 12 | `SuggestCloneConfig` | `src/internal/config/parser/suggest_clone.rs` | Filters the merged value to local-scope contributions before parsing, then handles array repositories or the object-level `repositories`, `template`, and `template_file` forms while recording child errors. |
| 13 | `PromptsConfig` | `src/internal/config/parser/prompts.rs` | Parses the top-level bare prompt array, records errors per element, and keeps valid prompts instead of failing the complete collection. Its serializer also emits the bare array. |
| 14 | `PromptConfig` | `src/internal/config/parser/prompts.rs` | Validates and trims required `id`/`prompt` fields, preserves an arbitrary context-free default, coerces supported `if` scalars, and passes the full parent object to prompt-type and scope parsing. |
| 15 | `PromptScope` | `src/internal/config/parser/prompts.rs` | Reads the `scope` sibling from the parent prompt object, normalizes aliases case-insensitively, and records invalid input while returning the default scope. |
| 16 | `PromptType` | `src/internal/config/parser/prompts.rs` | Reads the `type` sibling from the parent prompt object and conditionally consumes sibling `choices`, `min`, and `max` fields, including aliases, numeric coercion, defaults, and recoverable diagnostics. |
| 17 | `PromptChoicesConfig` | `src/internal/config/parser/prompts.rs` | Accepts either a non-empty array of derived `PromptChoiceConfig` values or a template string, and serializes the selected representation directly. |

## Count Verification

The source count is produced by matching uncommented implementation headers:

```text
^\s*impl<.*feuilletage::FromContextValue
```

That search returns 17 implementation sites and the 17 target types listed
above.

## Not Manual at This Tip

The following stale entries from earlier inventories now use
`#[derive(feuilletage::Config)]` and are not part of the count:

- `ShellAliasesConfig`
- `SuggestCloneRepositoryConfig`
- `PromptChoiceConfig`
- `StringFilter`
- `GithubAuthConfig`
- `CommandDefinition`
- `UpConfigGolang`

`EnvOperationConfig` is also not a manual implementation target. The live
environment helper implementation is for the private `EnvOpValue` type; it is
later converted into `EnvOperationConfig` by derived `EnvVarConfig` parsing.
