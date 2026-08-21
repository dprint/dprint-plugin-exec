import { generateChangeLog } from "jsr:@dprint/automation@0.11.2";

const version = Deno.args[0];
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

1. Run \`dprint add exec\`, which will add the plugin to your dprint configuration file.
2. Follow the configuration setup instructions found at https://github.com/dprint/dprint-plugin-exec#configuration
`;

console.log(text);
