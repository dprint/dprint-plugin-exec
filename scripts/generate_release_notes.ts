import { generateChangeLog } from "jsr:@dprint/automation@0.10.3";

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
- Run \`dprint init\` to create a config file.

Then:

1. Run \`dprint add exec\`, which will update the config file as follows:
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
