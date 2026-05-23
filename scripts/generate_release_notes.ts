import { generateChangeLog } from "jsr:@dprint/automation@0.11.2";

const version = Deno.args[0];
const checksum = Deno.args[1];

// optional npm install block; only emitted if create_npm_packages.ts has
// run and produced a manifest with the main package's tarball checksum.
let npmBlock = "";
try {
  const manifest = JSON.parse(await Deno.readTextFile("npm-dist/publish-manifest.json")) as {
    mainPackageName: string;
    mainPackageChecksum: string;
  };
  npmBlock = `
    Alternatively, run \`dprint add npm:${manifest.mainPackageName}\`, which will update the config file as follows:
    \`\`\`jsonc
    {
      // etc...
      "plugins": [
        "npm:${manifest.mainPackageName}@${version}/plugin.json@${manifest.mainPackageChecksum}"
      ]
    }
    \`\`\`
`;
} catch (err) {
  if (!(err instanceof Deno.errors.NotFound)) throw err;
}

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
${npmBlock}2. Follow the configuration setup instructions found at https://github.com/dprint/dprint-plugin-exec#configuration
`;

console.log(text);
