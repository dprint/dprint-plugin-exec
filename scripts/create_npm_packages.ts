import { $, CargoToml, processPlugin } from "jsr:@dprint/automation@0.11.0";

const pluginName = "dprint-plugin-exec";
const mainPackageName = "@dprint/exec";
const outDir = "npm-dist";

const platforms: processPlugin.Platform[] = [
  "darwin-aarch64",
  "darwin-x86_64",
  "linux-aarch64",
  "linux-aarch64-musl",
  "linux-x86_64",
  "linux-x86_64-musl",
  "linux-riscv64",
  "linux-loongarch64",
  "windows-x86_64",
];

const rootDir = $.path(import.meta.dirname!).join("..");
const version = new CargoToml(rootDir.join("Cargo.toml")).version();

const result = await processPlugin.createDprintOrgNpmPackages({
  pluginName,
  mainPackageName,
  version,
  outDir: rootDir.join(outDir).toString(),
  platforms: platforms.map((platform) => ({
    platform,
    zipFilePath: rootDir.join(
      processPlugin.getStandardZipFileName(pluginName, platform),
    ).toString(),
  })),
  packageJsonExtra: {
    description: "Execute a CLI as a dprint plugin to format code.",
    license: "MIT",
    repository: {
      type: "git",
      url: "git+https://github.com/dprint/dprint-plugin-exec.git",
    },
    homepage: "https://github.com/dprint/dprint-plugin-exec",
  },
});

console.log("Main package:", result.mainPackageDir);
console.log("Sub-packages:");
for (const dir of result.subPackageDirs) {
  console.log("  " + dir);
}
