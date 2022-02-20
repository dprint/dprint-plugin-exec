# dprint-plugin-exec

Exec formatting plugin for dprint.

This plugin can execute any CLI on the host machine. It's written in Rust (it's as fast as your CLI tool).

## Install

1. Install [dprint](https://dprint.dev/install/)
2. Follow instructions at https://github.com/dprint/dprint-plugin-exec/releases/

## Configuration

The configuration for dprint-plugin-exec is more complicated than most dprint plugins due to its nature.

1. Specify an includes pattern in dprint's config.
1. Specify an ["associations"](https://dprint.dev/config/#associations) property in the config in order to get the files that match that pattern to format with this exec plugin.
1. Add general configuration if desired (shown below).
1. Add binaries as shown below and specify what file patterns they match via a `<binary-name>.associations` property.
   - You may omit this from one binary in order to make any file pattern that doesn't match another binary to be formatted with this binary.
   - You may have associations match multiple binaries in order to format a file with multiple binaries instead of just one. The order in the config file will dictate the order the formatting occurs in. This functionality requires at least dprint 0.23 because in previous versions the config key order was unstable.

```jsonc
{
  // ...etc...
  "exec": {
    "associations": "**/*.{rs,js}",

    // general config (optional -- shown are the defaults)
    "lineWidth": 120,
    "indentWidth": 2,
    "useTabs": false,
    "newLineKind": "lf",
    "cacheKey": "1",
    "timeout": 30,

    // now define your binaries
    "rustfmt": "rustfmt",
    "rustfmt.associations": "**/*.rs",

    "prettier": "prettier --stdin-filepath {{file_path}} --tab-width {{indent_width}} --print-width {{line_width}}",
    "prettier.associations": "**/*.js"
  },
  "includes": [
    "**/*.{rs,txt}"
  ]
}
```

General config:

- `cacheKey` - Optional value used to bust dprint's incremental cache (ex. provide `"1"`). This is useful if you want to force formatting to occur because the underlying binary has changed.
- `timeout` - Number of seconds to allow an executable format to progress before a timeout error occurs (default: `30`).

Binary config:

- `<binary-name>` - Command to execute.
- `<binary-name>.associations` - File patterns to format with this binary.
- `<binary-name>.stdin` - If the text should be provided via stdin (default: `true`)
- `<binary-name>.cwd` - Current working directory to use when launching this binary (default: dprint's cwd)

Command templates (ex. see the prettier example above):

- `{{file_path}}` - File path being formatted.
- `{{file_text}}` - File text being formatted.
- `{{line_width}}` - Configured line width.
- `{{use_tabs}}` - Whether tabs should be used.
- `{{indent_width}}` - Whether tabs should be used.
- `{{cwd}}` - Current working directory.
- `{{timeout}}` - Specified timeout in seconds.
