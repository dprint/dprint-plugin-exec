extern crate dprint_development;
extern crate dprint_plugin_exec;

#[test]
fn test_specs() {
  use std::path::Path;
  use std::path::PathBuf;
  use std::sync::Arc;

  use dprint_core::configuration::*;
  use dprint_development::*;
  use dprint_plugin_exec::configuration::Configuration;

  let mut tests_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  tests_dir.push("tests");

  let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_time()
    .build()
    .unwrap();
  let handle = runtime.handle().clone();

  run_specs(
    &PathBuf::from("./tests/specs"),
    &ParseSpecOptions {
      default_file_name: "default.txt",
    },
    &RunSpecsOptions {
      fix_failures: false,
      format_twice: true,
    },
    Arc::new(move |file_name, file_text, spec_config| {
      let map: ConfigKeyMap = serde_json::from_value(spec_config.clone().into()).unwrap();
      let config_result = Configuration::resolve(map, &Default::default());
      ensure_no_diagnostics(&config_result.diagnostics);

      let mut file = file_name.to_path_buf();
      let mut td = tests_dir.clone();
      if !file_name.ends_with(Path::new("default.txt")) {
        td.push(file_name);
        file = td.clone();
      }

      eprintln!("{}", file_name.display());
      let file_text = file_text.to_string();
      handle.block_on(async {
        dprint_plugin_exec::handler::format_bytes(
          file,
          file_text.into_bytes(),
          Arc::new(config_result.config),
          Arc::new(dprint_core::plugins::NullCancellationToken),
        )
        .await
        .map(|maybe_bytes| maybe_bytes.map(|bytes| String::from_utf8(bytes).unwrap()))
      })
    }),
    Arc::new(move |_file_name, _file_text, _spec_config| panic!("Not supported.")),
  )
}
