#!/usr/bin/env -S deno run -A
import { conditions, defineMatrix, expr, job, step, workflow } from "jsr:@david/gagen@^0.5.0";

enum OperatingSystem {
  Macx86 = "macos-15-intel",
  MacArm = "macos-latest",
  Windows = "windows-latest",
  Linux = "ubuntu-22.04",
}

interface ProfileData {
  os: OperatingSystem;
  target: string;
  cross?: boolean;
  runTests?: boolean;
}

const profileDataItems: ProfileData[] = [{
  os: OperatingSystem.Macx86,
  target: "x86_64-apple-darwin",
  runTests: true,
}, {
  os: OperatingSystem.MacArm,
  target: "aarch64-apple-darwin",
  runTests: true,
}, {
  os: OperatingSystem.Windows,
  target: "x86_64-pc-windows-msvc",
  runTests: true,
}, {
  os: OperatingSystem.Linux,
  target: "x86_64-unknown-linux-gnu",
  runTests: true,
}, {
  os: OperatingSystem.Linux,
  target: "x86_64-unknown-linux-musl",
}, {
  os: OperatingSystem.Linux,
  target: "aarch64-unknown-linux-gnu",
}, {
  os: OperatingSystem.Linux,
  target: "aarch64-unknown-linux-musl",
}, {
  os: OperatingSystem.Linux,
  cross: true,
  target: "riscv64gc-unknown-linux-gnu",
}, {
  os: OperatingSystem.Linux,
  cross: true,
  target: "loongarch64-unknown-linux-gnu",
}];

const profiles = profileDataItems.map((profile) => {
  return {
    ...profile,
    artifactsName: `${profile.target}-artifacts`,
    zipFileName: `dprint-plugin-exec-${profile.target}.zip`,
    zipChecksumEnvVarName: `ZIP_CHECKSUM_${profile.target.toUpperCase().replaceAll("-", "_")}`,
  };
});

const matrix = defineMatrix({
  config: profiles.map((profile) => ({
    os: profile.os,
    run_tests: (profile.runTests ?? false).toString(),
    target: profile.target,
    cross: (profile.cross ?? false).toString(),
  })),
});

const target = expr("matrix.config.target");
const os = expr("matrix.config.os");
const cross = expr("matrix.config.cross");
const runTests = expr("matrix.config.run_tests");

const isTag = conditions.isTag();
const isNotTag = isTag.not();
const isCross = cross.equals("true");
const isNotCross = cross.notEquals("true");

const preReleaseSteps = profiles.map((profile) => {
  function getRunSteps() {
    switch (profile.os) {
      case OperatingSystem.MacArm:
      case OperatingSystem.Macx86:
      case OperatingSystem.Linux:
        return [
          `cd target/${profile.target}/release`,
          `zip -r ${profile.zipFileName} dprint-plugin-exec`,
          `echo "::set-output name=ZIP_CHECKSUM::$(shasum -a 256 ${profile.zipFileName} | awk '{print $1}')"`,
        ];
      case OperatingSystem.Windows:
        return [
          `Compress-Archive -CompressionLevel Optimal -Force -Path target/${profile.target}/release/dprint-plugin-exec.exe -DestinationPath target/${profile.target}/release/${profile.zipFileName}`,
          `echo "::set-output name=ZIP_CHECKSUM::$(shasum -a 256 target/${profile.target}/release/${profile.zipFileName} | awk '{print $1}')"`,
        ];
    }
  }
  return step({
    id: `pre_release_${profile.target.replaceAll("-", "_")}`,
    name: `Pre-release (${profile.target})`,
    if: target.equals(profile.target).and(isTag),
    outputs: ["ZIP_CHECKSUM"],
    run: getRunSteps(),
  });
});

const buildJob = job("build", {
  name: target,
  runsOn: os,
  strategy: { matrix },
  outputs: Object.fromEntries(
    profiles.map((profile, i) => [
      profile.zipChecksumEnvVarName,
      preReleaseSteps[i].outputs.ZIP_CHECKSUM,
    ]),
  ),
  env: {
    // disabled to reduce ./target size and generally it's slower enabled
    CARGO_INCREMENTAL: 0,
    RUST_BACKTRACE: "full",
  },
  steps: [
    {
      name: "Prepare git",
      run: [
        "git config --global core.autocrlf false",
        "git config --global core.eol lf",
      ],
    },
    { uses: "actions/checkout@v6" },
    { uses: "dsherret/rust-toolchain-file@v1" },
    {
      name: "Cache cargo",
      if: isNotTag,
      uses: "Swatinem/rust-cache@v2",
      with: { key: target },
    },
    { uses: "denoland/setup-deno@v2" },
    {
      name: "Setup (Linux x86_64-musl)",
      if: target.equals("x86_64-unknown-linux-musl"),
      run: [
        "sudo apt update",
        "sudo apt install musl musl-dev musl-tools",
        "rustup target add x86_64-unknown-linux-musl",
      ],
    },
    {
      name: "Setup (Linux aarch64)",
      if: target.equals("aarch64-unknown-linux-gnu"),
      run: [
        "sudo apt update",
        "sudo apt install -y gcc-aarch64-linux-gnu",
        "rustup target add aarch64-unknown-linux-gnu",
      ],
    },
    {
      name: "Setup (Linux aarch64-musl)",
      if: target.equals("aarch64-unknown-linux-musl"),
      run: [
        "sudo apt update",
        "sudo apt install gcc-aarch64-linux-gnu musl musl-dev musl-tools",
        "rustup target add aarch64-unknown-linux-musl",
      ],
    },
    {
      name: "Setup cross",
      if: isCross,
      run:
        "cargo install cross --git https://github.com/cross-rs/cross --rev 4090beca3cfffa44371a5bba524de3a578aa46c3",
    },
    {
      name: "Build (Debug)",
      if: isNotCross.and(isNotTag),
      env: {
        CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER: "aarch64-linux-gnu-gcc",
      },
      run: "cargo build --locked --all-targets --target ${{matrix.config.target}}",
    },
    {
      name: "Build release",
      if: isNotCross.and(isTag),
      env: {
        CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER: "aarch64-linux-gnu-gcc",
      },
      run: "cargo build --locked --all-targets --target ${{matrix.config.target}} --release",
    },
    {
      name: "Build cross (Debug)",
      if: isCross.and(isNotTag),
      run: "cross build --locked --target ${{matrix.config.target}}",
    },
    {
      name: "Build cross (Release)",
      if: isCross.and(isTag),
      run: "cross build --locked --target ${{matrix.config.target}} --release",
    },
    {
      name: "Lint",
      if: isNotTag.and(target.equals("x86_64-unknown-linux-gnu")),
      run: "cargo clippy",
    },
    {
      name: "Lint workflow generation",
      if: isNotTag.and(target.equals("x86_64-unknown-linux-gnu")),
      run: [
        "deno run -A .github/workflows/ci.ts --lint",
        "deno run -A .github/workflows/release.ts --lint",
      ],
    },
    {
      name: "Test (Debug)",
      if: runTests.equals("true").and(isNotTag),
      run: "cargo test --locked --all-features",
    },
    {
      name: "Test (Release)",
      if: runTests.equals("true").and(isTag),
      run: "cargo test --locked --all-features --release",
    },
    ...preReleaseSteps,
    ...profiles.map((profile) => ({
      name: `Upload artifacts (${profile.target})`,
      if: target.equals(profile.target).and(isTag),
      uses: "actions/upload-artifact@v7",
      with: {
        name: profile.artifactsName,
        path: `target/${profile.target}/release/${profile.zipFileName}`,
      },
    })),
  ],
});

const getTagVersion = step({
  id: "get_tag_version",
  name: "Get tag version",
  run: "echo ::set-output name=TAG_VERSION::${GITHUB_REF/refs\\/tags\\//}",
  outputs: ["TAG_VERSION"],
});

const getPluginFileChecksum = step({
  id: "get_plugin_file_checksum",
  name: "Get plugin file checksum",
  run: `echo "::set-output name=CHECKSUM::$(shasum -a 256 plugin.json | awk '{print $1}')"`,
  outputs: ["CHECKSUM"],
});

const draftReleaseJob = job("draft_release", {
  name: "draft_release",
  if: isTag,
  needs: [buildJob],
  runsOn: "ubuntu-latest",
  // id-token: write is required for npm --provenance
  permissions: { contents: "write", "id-token": "write" },
  steps: [
    { name: "Checkout", uses: "actions/checkout@v6" },
    { name: "Download artifacts", uses: "actions/download-artifact@v8" },
    { uses: "denoland/setup-deno@v2" },
    {
      uses: "actions/setup-node@v6",
      with: { "node-version": "24.x", "registry-url": "https://registry.npmjs.org" },
    },
    {
      name: "Move downloaded artifacts to root directory",
      run: profiles.map((profile) => `mv ${profile.artifactsName}/${profile.zipFileName} .`),
    },
    {
      name: "Output checksums",
      run: profiles.map((profile) =>
        `echo "${profile.zipFileName}: ${buildJob.outputs[profile.zipChecksumEnvVarName]}"`
      ),
    },
    {
      name: "Create plugin file",
      run: "deno run --allow-read=. --allow-write=. scripts/create_plugin_file.ts",
    },
    getTagVersion,
    getPluginFileChecksum,
    {
      name: "Update Config Schema Version",
      run:
        `sed -i 's/exec\\/0.0.0/exec\\/${getTagVersion.outputs.TAG_VERSION}/' deployment/schema.json`,
    },
    {
      name: "Create release notes",
      run:
        `deno run -A ./scripts/generate_release_notes.ts ${getTagVersion.outputs.TAG_VERSION} ${getPluginFileChecksum.outputs.CHECKSUM} > \${{ github.workspace }}-CHANGELOG.txt`,
    },
    {
      name: "Release",
      uses: "softprops/action-gh-release@v2.6.1",
      env: { GITHUB_TOKEN: "${{ secrets.GITHUB_TOKEN }}" },
      with: {
        files: [
          ...profiles.map((profile) => profile.zipFileName),
          "plugin.json",
          "deployment/schema.json",
        ].join("\n"),
        body_path: "${{ github.workspace }}-CHANGELOG.txt",
      },
    },
    {
      name: "Build npm packages",
      run: "deno run -A scripts/create_npm_packages.ts",
    },
    {
      name: "Publish npm packages",
      run: "deno run -A scripts/publish_npm_packages.ts",
    },
  ],
});

workflow({
  name: "CI",
  on: {
    pull_request: { branches: ["main"] },
    push: { branches: ["main"], tags: ["*"] },
  },
  concurrency: {
    // https://stackoverflow.com/a/72408109/188246
    group: "${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}",
    cancelInProgress: true,
  },
  jobs: [buildJob, draftReleaseJob],
}).writeOrLint({
  filePath: new URL("./ci.generated.yml", import.meta.url),
  header: "# GENERATED BY ./ci.ts -- DO NOT DIRECTLY EDIT",
  pinDeps: false,
});
