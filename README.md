# dprint-plugin-exec

Plugin that formats code via mostly any formatting CLI found on the host machine.

This plugin executes CLI commands to format code via stdin (recommended) or via a file path.

## Install

1. Install [dprint](https://dprint.dev/install/)
2. Follow instructions at https://github.com/dprint/dprint-plugin-exec/releases/

## Configuration

1. Add general configuration if desired (shown below).
1. Add binaries similar to what's shown below and specify what file extensions they match via a `exts` property.

```jsonc
{
  // ...etc...
  "exec": {
    // general config (optional -- shown are the defaults)
    "lineWidth": 120,
    "indentWidth": 2,
    "useTabs": false,
    "newLineKind": "lf",
    "cacheKey": "1",
    "timeout": 30,

    // now define your commands, for example...
    "commands": [{
      "command": "rustfmt",
      "exts": ["rs"]
    }, {
      "command": "java -jar formatter.jar {{file_path}}",
      "exts": ["java"]
    }, {
      "command": "yapf",
      "exts": ["py"]
    }]
  },
  "plugins": [
    // run `dprint config add exec` to add the latest exec plugin's url here
  ]
}
```

General config:

- `cacheKey` - Optional value used to bust dprint's incremental cache (ex. provide `"1"`). This is useful if you want to force formatting to occur because the underlying command's code has changed.
- `timeout` - Number of seconds to allow an executable format to occur before a timeout error occurs (default: `30`).

Command config:

- `command` - Command to execute.
- `exts` - Array of file extensions to format with this command.
- `fileNames` - Array of file names to format with this command (useful for files without extensions).
- `associations` - File patterns to format with this command. If specified, then you MUST specify associations on this plugin's config as well.
  - You may have associations match multiple binaries in order to format a file with multiple binaries instead of just one. The order in the config file will dictate the order the formatting occurs in.
- `stdin` - If the text should be provided via stdin (default: `true`)
- `cwd` - Current working directory to use when launching this command (default: dprint's cwd)

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
    "commands": [{
      "command": "yapf",
      "exts": ["py"]
    }]
  },
  "plugins": [
    // run `dprint config add exec` to add the latest exec plugin's url here
  ]
}
```

### Example - java

```jsonc
{
  // ...etc...
  "exec": {
    "commands": [{
      "command": "java -jar formatter.jar {{file_path}}",
      "exts": ["java"]
    }]
  },
  "plugins": [
    // run `dprint config add exec` to add the latest exec plugin's url here
  ]
}
```

### Example - rustfmt

Use the `rustfmt` binary so you can format stdin.

```jsonc
{
  // ...etc...
  "exec": {
    "commands": [{
      "command": "rustfmt --edition 2021",
      "exts": ["rs"]
    }]
  },
  "plugins": [
    // run `dprint config add exec` to add the latest exec plugin's url here
  ]
}
```

### Example - prettier

Consider using [dprint-plugin-prettier](https://dprint.dev/plugins/prettier/) instead as it will be much faster.

```jsonc
{
  // ...etc...
  "exec": {
    "commands": [{
      "command": "prettier --stdin-filepath {{file_path}} --tab-width {{indent_width}} --print-width {{line_width}}",
      // add more extensions that prettier should format
      "exts": ["js", "ts", "html"]
    }]
  },
  "plugins": [
    // run `dprint config add exec` to add the latest exec plugin's url here
  ]
}
```
