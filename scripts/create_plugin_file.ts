import { $, CargoToml, processPlugin } from "jsr:@dprint/automation@0.12.2";

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
    "linux-loongarch64-musl",
    "linux-powerpc64",
    "linux-powerpc64-musl",
    "android-aarch64",
    "android-x86_64",
    "windows-x86_64",
  ],
  isTest: Deno.args.some(a => a == "--test"),
});
