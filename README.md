# dprint-plugin-exec

Plugin that formats code via mostly any formatting CLI found on the host machine.

This plugin executes CLI commands to format code via stdin (recommended) or via a file path.

## Install

1. Install [dprint](https://dprint.dev/install/)
2. Follow instructions at https://github.com/dprint/dprint-plugin-exec/releases/

## Configuration

The configuration for dprint-plugin-exec is more complicated than most dprint plugins due to its nature.

1. Specify an includes pattern in dprint's config.
1. Specify an [`"associations"`](https://dprint.dev/config/#associations) property in the plugin config in order to get the files that match that pattern to format with this exec plugin.
1. Add general configuration if desired (shown below).
1. Add binaries similar to what's shown below and specify what file patterns they match via a `<command-name>.associations` property.
   - You may omit this from one command in order to make any file pattern that doesn't match another command to be formatted with this command.
   - You may have associations match multiple binaries in order to format a file with multiple binaries instead of just one. The order in the config file will dictate the order the formatting occurs in. This functionality requires at least dprint 0.23 because in previous versions the config key order was unstable.

```jsonc
{
  // ...etc...
  "exec": {
    "associations": "**/*.{rs,js,html,ts,js}",

    // general config (optional -- shown are the defaults)
    "lineWidth": 120,
    "indentWidth": 2,
    "useTabs": false,
    "newLineKind": "lf",
    "cacheKey": "1",
    "timeout": 30,

    // now define your commands, for example...
    "rustfmt": "rustfmt",
    "rustfmt.associations": "**/*.rs",

    "java": "java -jar formatter.jar {{file_path}}",
    "java.associations": "**/*.java",

    "prettier": "prettier --stdin-filepath {{file_path}} --tab-width {{indent_width}} --print-width {{line_width}}"
  },
  "includes": [
    "**/*.{rs,java,html,ts,js}"
  ]
}
```

General config:

- `cacheKey` - Optional value used to bust dprint's incremental cache (ex. provide `"1"`). This is useful if you want to force formatting to occur because the underlying command's code has changed.
- `timeout` - Number of seconds to allow an executable format to progress before a timeout error occurs (default: `30`).

Command config:

- `<command-name>` - Command to execute.
- `<command-name>.associations` - File patterns to format with this command.
- `<command-name>.stdin` - If the text should be provided via stdin (default: `true`)
- `<command-name>.cwd` - Current working directory to use when launching this command (default: dprint's cwd)

Command templates (ex. see the prettier example above):

- `{{file_path}}` - File path being formatted.
- `{{line_width}}` - Configured line width.
- `{{use_tabs}}` - Whether tabs should be used.
- `{{indent_width}}` - Whether tabs should be used.
- `{{cwd}}` - Current working directory.
- `{{timeout}}` - Specified timeout in seconds.
