const packageText = Deno.readTextFileSync("Cargo.toml");
// version = "x.x.x"
const version = packageText.match(/version\s*=\s*\"(\d+\.\d+\.\d+)\"/)![1];
const pluginName = "dprint-plugin-exec";

if (!/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error("Error extracting version.");
}

const outputFile = {
  schemaVersion: 1,
  name: pluginName,
  version,
  "mac-x86_64": await getPlatformObject(`${pluginName}-x86_64-apple-darwin.zip`),
  "linux-x86_64": await getPlatformObject(`${pluginName}-x86_64-unknown-linux-gnu.zip`),
  "windows-x86_64": await getPlatformObject(`${pluginName}-x86_64-pc-windows-msvc.zip`),
};
Deno.writeTextFile("plugin.exe-plugin", JSON.stringify(outputFile, undefined, 2) + "\n");

async function getPlatformObject(zipFileName: string) {
  const fileBytes = Deno.readFileSync(zipFileName);
  const checksum = await getChecksum(fileBytes);
  console.log(zipFileName + ": " + checksum);
  return {
    "reference": getPlatformReferenceUrl(zipFileName),
    "checksum": checksum,
  };
}

async function getChecksum(bytes: Uint8Array) {
  // https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto/digest#converting_a_digest_to_a_hex_string
  const hashBuffer = await crypto.subtle.digest("SHA-256", bytes);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  const hashHex = hashArray.map(b => b.toString(16).padStart(2, "0")).join("");
  return hashHex;
}

function getPlatformReferenceUrl(zipFileName: string) {
  if (Deno.args.some(arg => arg === "--test")) {
    return zipFileName;
  } else {
    return `https://github.com/dprint/${pluginName}/releases/download/${version}/${zipFileName}`
  }
}