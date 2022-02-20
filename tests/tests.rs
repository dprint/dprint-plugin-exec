extern crate dprint_development;
extern crate dprint_plugin_exec;

#[test]
#[cfg(unix)]
fn test_specs() {
  use std::path::Path;
  use std::path::PathBuf;

  use dprint_core::configuration::*;
  use dprint_development::*;
  use dprint_plugin_exec::configuration::Configuration;

  let mut tests_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  tests_dir.push("tests");

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

        let mut file = file_name;
        let mut td = tests_dir.clone();
        if !file_name.ends_with(Path::new("default.txt")) {
          td.push(file_name);
          file = td.as_path().clone();
        }

        dprint_plugin_exec::handler::format_text(
          file,
          &file_text,
          &config_result.config,
          |_, _, _| Result::Ok(String::from("")),
        )
      }
    },
    move |_file_name, _file_text, _spec_config| panic!("Not supported."),
  )
}
