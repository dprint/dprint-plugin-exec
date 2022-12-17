import * as path from "https://deno.land/std@0.153.0/path/mod.ts";
import { extractCargoVersionOrThrow, processPlugin } from "https://raw.githubusercontent.com/dprint/automation/0.6.0/mod.ts";

const currentDirPath = path.dirname(path.fromFileUrl(import.meta.url));
const cargoFilePath = path.join(currentDirPath, "../", "Cargo.toml");

await processPlugin.createDprintOrgProcessPlugin({
  pluginName: "dprint-plugin-exec",
  version: await extractCargoVersionOrThrow(cargoFilePath),
  platforms: [
    "darwin-aarch64",
    "darwin-x86_64",
    "linux-aarch64",
    "linux-x86_64",
    "linux-x86_64-musl",
    "windows-x86_64",
  ],
  isTest: Deno.args.some(a => a == "--test"),
});
