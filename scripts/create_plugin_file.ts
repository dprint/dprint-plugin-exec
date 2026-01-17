import {
  $,
  CargoToml,
  processPlugin,
} from "https://raw.githubusercontent.com/dprint/automation/0.10.0/mod.ts";

const currentDirPath = $.path(import.meta.dirname!);
const cargoFilePath = currentDirPath.join("../Cargo.toml");

await processPlugin.createDprintOrgProcessPlugin({
  pluginName: "dprint-plugin-exec",
  version: new CargoToml(cargoFilePath).version(),
  platforms: [
    "darwin-aarch64",
    "darwin-x86_64",
    "linux-aarch64",
    "linux-aarch64-musl",
    "linux-x86_64",
    "linux-x86_64-musl",
    "linux-riscv64",
    "linux-loongarch64",
    "windows-x86_64",
  ],
  isTest: Deno.args.some(a => a == "--test"),
});
