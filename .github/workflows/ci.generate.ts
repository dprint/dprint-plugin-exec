import * as yaml from "https://deno.land/std@0.170.0/encoding/yaml.ts";

enum OperatingSystem {
  Macx86 = "macos-12",
  MacArm = "macos-latest",
  Windows = "windows-latest",
  Linux = "ubuntu-20.04",
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
}];
const profiles = profileDataItems.map((profile) => {
  return {
    ...profile,
    artifactsName: `${profile.target}-artifacts`,
    zipFileName: `dprint-plugin-exec-${profile.target}.zip`,
    zipChecksumEnvVarName: `ZIP_CHECKSUM_${profile.target.toUpperCase().replaceAll("-", "_")}`,
  };
});

const ci = {
  name: "CI",
  on: {
    pull_request: { branches: ["main"] },
    push: { branches: ["main"], tags: ["*"] },
  },
  concurrency: {
    // https://stackoverflow.com/a/72408109/188246
    group: "${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}",
    "cancel-in-progress": true,
  },
  jobs: {
    build: {
      name: "${{ matrix.config.target }}",
      "runs-on": "${{ matrix.config.os }}",
      strategy: {
        matrix: {
          config: profiles.map((profile) => ({
            os: profile.os,
            run_tests: (profile.runTests ?? false).toString(),
            target: profile.target,
            cross: (profile.cross ?? false).toString(),
          })),
        },
      },
      outputs: Object.fromEntries(
        profiles.map((profile) => [
          profile.zipChecksumEnvVarName,
          "${{steps.pre_release_" + profile.target.replaceAll("-", "_")
          + ".outputs.ZIP_CHECKSUM}}",
        ]),
      ),
      env: {
        // disabled to reduce ./target size and generally it's slower enabled
        CARGO_INCREMENTAL: 0,
        RUST_BACKTRACE: "full",
      },
      steps: [
        {
          uses: "actions/checkout@v4",
          with: {
            config: [
              "core.autocrlf=false",
              "core.eol=lf",
            ].join("\n"),
          },
        },
        { uses: "dsherret/rust-toolchain-file@v1" },
        {
          name: "Cache cargo",
          if: "startsWith(github.ref, 'refs/tags/') != true",
          uses: "Swatinem/rust-cache@v2",
          with: {
            key: "${{ matrix.config.target }}",
          },
        },
        { uses: "denoland/setup-deno@v2" },
        {
          name: "Setup (Linux x86_64-musl)",
          if: "matrix.config.target == 'x86_64-unknown-linux-musl'",
          run: [
            "sudo apt update",
            "sudo apt install musl musl-dev musl-tools",
            "rustup target add x86_64-unknown-linux-musl",
          ].join("\n"),
        },
        {
          name: "Setup (Linux aarch64)",
          if: "matrix.config.target == 'aarch64-unknown-linux-gnu'",
          run: [
            "sudo apt update",
            "sudo apt install -y gcc-aarch64-linux-gnu",
            "rustup target add aarch64-unknown-linux-gnu",
          ].join("\n"),
        },
        {
          name: "Setup (Linux aarch64-musl)",
          if: "matrix.config.target == 'aarch64-unknown-linux-musl'",
          run: [
            "sudo apt update",
            "sudo apt install gcc-aarch64-linux-gnu musl musl-dev musl-tools",
            "rustup target add aarch64-unknown-linux-musl",
          ].join("\n"),
        },
        {
          name: "Setup cross",
          if: "matrix.config.cross == 'true'",
          run: [
            "cargo install cross --git https://github.com/cross-rs/cross --rev 88f49ff79e777bef6d3564531636ee4d3cc2f8d2",
          ].join("\n"),
        },
        {
          name: "Build (Debug)",
          if: "matrix.config.cross != 'true' && !startsWith(github.ref, 'refs/tags/')",
          env: {
            "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER": "aarch64-linux-gnu-gcc",
          },
          run: "cargo build --locked --all-targets --target ${{matrix.config.target}}",
        },
        {
          name: "Build release",
          if: "matrix.config.cross != 'true' && startsWith(github.ref, 'refs/tags/')",
          env: {
            "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER": "aarch64-linux-gnu-gcc",
          },
          run: "cargo build --locked --all-targets --target ${{matrix.config.target}} --release",
        },
        {
          name: "Build cross (Debug)",
          if: "matrix.config.cross == 'true' && !startsWith(github.ref, 'refs/tags/')",
          run: [
            "cross build --locked --target ${{matrix.config.target}}",
          ].join("\n"),
        },
        {
          name: "Build cross (Release)",
          if: "matrix.config.cross == 'true' && startsWith(github.ref, 'refs/tags/')",
          run: [
            "cross build --locked --target ${{matrix.config.target}} --release",
          ].join("\n"),
        },
        {
          name: "Lint",
          if:
            "!startsWith(github.ref, 'refs/tags/') && matrix.config.target == 'x86_64-unknown-linux-gnu'",
          run: "cargo clippy",
        },
        {
          name: "Test (Debug)",
          if: "matrix.config.run_tests == 'true' && !startsWith(github.ref, 'refs/tags/')",
          run: "cargo test --locked --all-features",
        },
        {
          name: "Test (Release)",
          if: "matrix.config.run_tests == 'true' && startsWith(github.ref, 'refs/tags/')",
          run: "cargo test --locked --all-features --release",
        },
        // zip files
        ...profiles.map((profile) => {
          function getRunSteps() {
            switch (profile.os) {
              case OperatingSystem.MacArm:
              case OperatingSystem.Macx86:
                return [
                  `cd target/${profile.target}/release`,
                  `zip -r ${profile.zipFileName} dprint-plugin-exec`,
                  `echo \"::set-output name=ZIP_CHECKSUM::$(shasum -a 256 ${profile.zipFileName} | awk '{print $1}')\"`,
                ];
              case OperatingSystem.Linux:
                return [
                  `cd target/${profile.target}/release`,
                  `zip -r ${profile.zipFileName} dprint-plugin-exec`,
                  `echo \"::set-output name=ZIP_CHECKSUM::$(shasum -a 256 ${profile.zipFileName} | awk '{print $1}')\"`,
                ];
              case OperatingSystem.Windows:
                return [
                  `Compress-Archive -CompressionLevel Optimal -Force -Path target/${profile.target}/release/dprint-plugin-exec.exe -DestinationPath target/${profile.target}/release/${profile.zipFileName}`,
                  `echo "::set-output name=ZIP_CHECKSUM::$(shasum -a 256 target/${profile.target}/release/${profile.zipFileName} | awk '{print $1}')"`,
                ];
            }
          }
          return {
            name: `Pre-release (${profile.target})`,
            id: `pre_release_${profile.target.replaceAll("-", "_")}`,
            if:
              `matrix.config.target == '${profile.target}' && startsWith(github.ref, 'refs/tags/')`,
            run: getRunSteps().join("\n"),
          };
        }),
        // upload artifacts
        ...profiles.map((profile) => {
          return {
            name: `Upload artifacts (${profile.target})`,
            if:
              `matrix.config.target == '${profile.target}' && startsWith(github.ref, 'refs/tags/')`,
            uses: "actions/upload-artifact@v4",
            with: {
              name: profile.artifactsName,
              path: `target/${profile.target}/release/${profile.zipFileName}`,
            },
          };
        }),
      ],
    },
    draft_release: {
      name: "draft_release",
      if: "startsWith(github.ref, 'refs/tags/')",
      needs: "build",
      "runs-on": "ubuntu-latest",
      steps: [
        { name: "Checkout", uses: "actions/checkout@v4" },
        { name: "Download artifacts", uses: "actions/download-artifact@v4" },
        { uses: "denoland/setup-deno@v2" },
        {
          name: "Move downloaded artifacts to root directory",
          run: profiles.map((profile) => {
            return `mv ${profile.artifactsName}/${profile.zipFileName} .`;
          }).join("\n"),
        },
        {
          name: "Output checksums",
          run: profiles.map((profile) => {
            return `echo "${profile.zipFileName}: \${{needs.build.outputs.${profile.zipChecksumEnvVarName}}}"`;
          }).join("\n"),
        },
        {
          name: "Create plugin file",
          run: "deno run --allow-read=. --allow-write=. scripts/create_plugin_file.ts",
        },
        {
          name: "Get tag version",
          id: "get_tag_version",
          run: "echo ::set-output name=TAG_VERSION::${GITHUB_REF/refs\\/tags\\//}",
        },
        {
          name: "Get plugin file checksum",
          id: "get_plugin_file_checksum",
          run:
            "echo \"::set-output name=CHECKSUM::$(shasum -a 256 plugin.json | awk '{print $1}')\"",
        },
        {
          name: "Update Config Schema Version",
          run:
            "sed -i 's/exec\\/0.0.0/exec\\/${{ steps.get_tag_version.outputs.TAG_VERSION }}/' deployment/schema.json",
        },
        {
          name: "Release",
          uses: "softprops/action-gh-release@v1",
          env: { GITHUB_TOKEN: "${{ secrets.GITHUB_TOKEN }}" },
          with: {
            files: [
              ...profiles.map((profile) => profile.zipFileName),
              "plugin.json",
              "deployment/schema.json",
            ].join("\n"),
            body: `## Install

Dependencies:

- Install dprint's CLI >= 0.40.0

In a dprint configuration file:

1. Specify the plugin url and checksum in the \`"plugins"\` array or run \`dprint config add exec\`:
   \`\`\`jsonc
   {
     // etc...
     "plugins": [
       "https://plugins.dprint.dev/exec-\${{ steps.get_tag_version.outputs.TAG_VERSION }}.json@\${{ steps.get_plugin_file_checksum.outputs.CHECKSUM }}"
     ]
   }
   \`\`\`
2. Follow the configuration setup instructions found at https://github.com/dprint/dprint-plugin-exec#configuration`,
            draft: false,
          },
        },
      ],
    },
  },
};

let finalText = `# GENERATED BY ./ci.generate.ts -- DO NOT DIRECTLY EDIT\n\n`;
finalText += yaml.stringify(ci, {
  noRefs: true,
  lineWidth: 10_000,
  noCompatMode: true,
});

Deno.writeTextFileSync(new URL("./ci.yml", import.meta.url), finalText);
