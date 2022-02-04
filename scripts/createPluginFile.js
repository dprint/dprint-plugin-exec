const fs = require("fs");
const crypto = require("crypto");

const hasTestArg = process.argv.slice(2).some(arg => arg === "--test");
const packageText = fs.readFileSync("Cargo.toml", {encoding: "utf8"});
// version = "x.x.x"
const version = packageText.match(/version\s*=\s*"(\d+\.\d+\.\d+)"/)[1];

if (!/^\d+\.\d+\.\d+$/.test(version)) {
    throw new Error("Error extracting version.");
}

const outputFile = {
    schemaVersion: 1,
    name: "dprint-plugin-exec",
    version,
    "mac-x86_64": getPlatformObject("dprint-plugin-exec-x86_64-apple-darwin.zip"),
    "linux-x86_64": getPlatformObject("dprint-plugin-exec-x86_64-unknown-linux-gnu.zip"),
    "windows-x86_64": getPlatformObject("dprint-plugin-exec-x86_64-pc-windows-msvc.zip"),
};
fs.writeFileSync(targetFolder() + "exec.exe-plugin", JSON.stringify(outputFile, undefined, 2), {encoding: "utf8"});

if (hasTestArg) {
    const fileBytes = fs.readFileSync("target/release/exec.exe-plugin");
    const hash = crypto.createHash("sha256");
    hash.update(fileBytes);
    const checksum = hash.digest("hex");
    console.log("Test plugin checksum: " + checksum);
}

function targetFolder() {
    return hasTestArg ? 'target/release/' : '';
}

function getPlatformObject(zipFileName) {
    const fileBytes = fs.readFileSync(targetFolder() + `${zipFileName}`);
    const hash = crypto.createHash("sha256");
    hash.update(fileBytes);
    const checksum = hash.digest("hex");
    console.log(zipFileName + ": " + checksum);
    return {
        "reference": hasTestArg
            ? `${zipFileName}`
            : `https://github.com/dprint/dprint-plugin-exec/releases/download/${version}/${zipFileName}`,
        "checksum": checksum,
    };
}
