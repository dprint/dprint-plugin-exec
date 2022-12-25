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

  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_time()
    .enable_io()
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
    {
      move |file_name, file_text, spec_config| {
        let config_result =
          Configuration::resolve(parse_config_key_map(spec_config), &Default::default());
        ensure_no_diagnostics(&config_result.diagnostics);

        let mut file = file_name.to_path_buf();
        let mut td = tests_dir.clone();
        if !file_name.ends_with(Path::new("default.txt")) {
          td.push(file_name);
          file = td.clone();
        }

        handle.block_on(async {
          dprint_plugin_exec::handler::format_text(
            file,
            file_text.to_string(),
            Arc::new(config_result.config),
            Arc::new(dprint_core::plugins::NullCancellationToken),
          )
          .await
        })
      }
    },
    move |_file_name, _file_text, _spec_config| panic!("Not supported."),
  )
}
