import $, { type Path } from "jsr:@david/dax@^0.47.0";

const npmDistDir = $.path(import.meta.dirname!).join("../npm-dist");

const subPackages: Path[] = [];
let mainPackage: Path | undefined;

for await (const entry of npmDistDir.readDir()) {
  if (!entry.isDirectory) continue;
  if (entry.path.join("plugin.zip").existsSync()) {
    subPackages.push(entry.path);
  } else if (entry.path.join("plugin.json").existsSync()) {
    mainPackage = entry.path;
  }
}

// sub-packages contain plugin.zip and must publish first;
// the main package contains plugin.json and references them.
for (const dir of subPackages) {
  $.logStep("Publishing sub-package", dir.toString());
  await $`npm publish --access public --provenance`.cwd(dir);
}
if (mainPackage != null) {
  $.logStep("Publishing main package", mainPackage.toString());
  await $`npm publish --access public --provenance`.cwd(mainPackage);
}
