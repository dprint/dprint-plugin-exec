# dprint-plugin-exec

Plugin that formats code via mostly any formatting CLI found on the host machine.

This plugin executes CLI commands to format code via stdin (recommended) or via a file path.

## Install

1. Install [dprint](https://dprint.dev/install/)
2. `dprint init`
3. `dprint add exec`
   - Or install from npm: `dprint add npm:@dprint/exec`

## Configuration

1. Add general configuration if desired (shown below).
1. Add binaries similar to what's shown below and specify what file extensions they match via a `exts` property.

```jsonc
{
  // ...etc...
  "exec": {
    // lets the plugin know the cwd, see https://dprint.dev/config/#configuration-variables
    "cwd": "${configDir}",

    // general config (optional -- shown are the defaults)
    "lineWidth": 120,
    "indentWidth": 2,
    "useTabs": false,
    "cacheKey": "1",
    "timeout": 30,

    // now define your commands, for example...
    "commands": [{
      "command": "rustfmt",
      "exts": ["rs"],
    }, {
      "command": "java -jar formatter.jar {{file_path}}",
      "exts": ["java"],
    }, {
      "command": "yapf",
      "exts": ["py"],
    }],
  },
  "plugins": [
    // run `dprint config add exec` to add the latest exec plugin's url here
  ],
}
```

General config:

- `cacheKey` - Optional value used to bust dprint's incremental cache (ex. provide `"1"`). This is useful if you want to force formatting to occur because the underlying command's code has changed.
  - If you want to automatically calculate the cache key, consider using `command.cacheKeyFiles`.
- `timeout` - Number of seconds to allow an executable format to occur before a timeout error occurs (default: `30`).
- `cwd` - Recommend setting this to `${configDir}` to force it to use the cwd of the current config file.

Matching files to commands:

A command can be matched to files in three ways: `exts`, `fileNames`, or
`associations`. They differ in what they match and — importantly — in whether
multiple commands can run on the same file.

What each matches:

- `exts` — by file extension, e.g. `["rs", "py"]`.
- `fileNames` — by full file name, useful for files without an extension, e.g.
  `["Dockerfile", "BUILD"]`.
- `associations` — by glob pattern, e.g. `"**/*.{bazel,bzl}"`. Only **one** glob
  per command is supported, though brace expansion within that glob is allowed.

How many commands run per file:

- With `exts` or `fileNames`, only the **first** matching command runs on a
  given file. Subsequent commands that also match are skipped.
- With `associations`, **every** matching command runs, in the order they appear
  in the `commands` array.

So if you want to chain formatters on the same file, each chained command must use `associations` — and
you must also declare `associations` at the **plugin level** (the top-level
`"exec"` block), otherwise dprint won't route those files to this plugin at all:

```jsonc
{
  "exec": {
    "associations": ["**/*.{rs,swift,txt}"],
    "commands": [
      { "command": "rustfmt", "associations": "**/*.rs" },
      { "command": "swift-format -", "associations": "**/*.swift" },
      { "command": "keep-sorted -", "associations": "**/*" },
    ],
  },
}
```

`associations` can do the work of `exts` and `fileNames`; the latter two exist
as a convenience for the common case where you don't need glob matching or
command chaining.

Mixing styles across commands is allowed, but if you want chaining for a given
file type, use `associations` on every command that should participate — an
`exts`/`fileNames`-only command that matches first will short-circuit the loop
and prevent later commands from running on that file.

Command config:

- `command` - Command to execute.
- `exts` - Array of file extensions to format with this command.
- `fileNames` - Array of file names to format with this command (useful for files without extensions).
- `associations` - File patterns to format with this command. If specified, then you MUST specify associations on this plugin's config as well.
  - You may have associations match multiple binaries in order to format a file with multiple binaries instead of just one. The order in the config file will dictate the order the formatting occurs in.
- `stdin` - If the text should be provided via stdin (default: `true`)
- `cwd` - Current working directory to use when launching this command (default: dprint's cwd or the root `cwd` setting if set)
- `cacheKeyFiles` - A list of paths (relative to `cwd`) to files used to automatically compute a `cacheKey`. This allows automatic invalidation of dprint's incremental cache when any of these files are changed.
- `setupCommand` - Command to run a single time before this command formats its first file. It runs to completion before any formatting starts, which is useful for one-time setup that would otherwise race when formatting in parallel (ex. installing a toolchain). It is only run when a file actually matches this command, runs in the command's `cwd`, is not subject to the `timeout`, and is not run if formatting is cancelled. It does not support command templates.

Command templates (ex. see the prettier example above):

- `{{file_path}}` - File path being formatted.
- `{{line_width}}` - Configured line width.
- `{{use_tabs}}` - Whether tabs should be used.
- `{{indent_width}}` - Whether tabs should be used.
- `{{cwd}}` - Current working directory.
- `{{timeout}}` - Specified timeout in seconds.

### Example - yapf

```jsonc
{
  // ...etc...
  "exec": {
    "cwd": "${configDir}",
    "commands": [{
      "command": "yapf",
      "exts": ["py"],
    }],
  },
  "plugins": [
    // run `dprint config add exec` to add the latest exec plugin's url here
  ],
}
```

### Example - java

```jsonc
{
  // ...etc...
  "exec": {
    "cwd": "${configDir}",
    "commands": [{
      "command": "java -jar formatter.jar {{file_path}}",
      "exts": ["java"],
    }],
  },
  "plugins": [
    // run `dprint config add exec` to add the latest exec plugin's url here
  ],
}
```

### Example - rustfmt

Use the `rustfmt` binary so you can format stdin.

```jsonc
{
  // ...etc...
  "exec": {
    "cwd": "${configDir}",
    "commands": [{
      "command": "rustfmt --edition 2024",
      "exts": ["rs"],
      // add the config files for automatic cache invalidation when the rust version or rustfmt config changes
      "cacheKeyFiles": [
        "rustfmt.toml",
        "rust-toolchain.toml",
      ],
    }],
  },
  "plugins": [
    // run `dprint config add exec` to add the latest exec plugin's url here
  ],
}
```

### Example - prettier

Consider using [dprint-plugin-prettier](https://dprint.dev/plugins/prettier/) instead as it will be much faster.

```jsonc
{
  // ...etc...
  "exec": {
    "cwd": "${configDir}",
    "commands": [{
      "command": "prettier --stdin-filepath {{file_path}} --tab-width {{indent_width}} --print-width {{line_width}}",
      // add more extensions that prettier should format
      "exts": ["js", "ts", "html"],
      // add the config files for automatic cache invalidation when the prettier config config changes
      "cacheKeyFiles": [
        ".prettierrc.json",
      ],
    }],
  },
  "plugins": [
    // run `dprint config add exec` to add the latest exec plugin's url here
  ],
}
```
