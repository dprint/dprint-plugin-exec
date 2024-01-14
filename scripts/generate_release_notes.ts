import { generateChangeLog } from "https://raw.githubusercontent.com/dprint/automation/0.9.0/changelog.ts";

const version = Deno.args[0];
const checksum = Deno.args[1];
const changelog = await generateChangeLog({
  versionTo: version,
});
const text = `## Changes

${changelog}

## Install

Dependencies:

- Install dprint's CLI >= 0.40.0

In a dprint configuration file:

1. Specify the plugin url and checksum in the \`"plugins"\` array or run \`dprint config add exec\`:
    \`\`\`jsonc
    {
      // etc...
      "plugins": [
        "https://plugins.dprint.dev/exec-${version}.json@${checksum}"
      ]
    }
    \`\`\`
2. Follow the configuration setup instructions found at https://github.com/dprint/dprint-plugin-exec#configuration
`;

console.log(text);
