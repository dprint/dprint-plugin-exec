extern crate dprint_development;
extern crate dprint_plugin_exec;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dprint_core::configuration::*;
use dprint_development::*;
use dprint_plugin_exec::configuration::Configuration;

#[test]
#[cfg(unix)]
fn test_specs() {
  let global_config = resolve_global_config(HashMap::new(), &Default::default()).config;

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
      let global_config = global_config.clone();
      move |file_name, file_text, spec_config| {
        let config_result =
          Configuration::resolve(parse_config_key_map(spec_config), &global_config);
        ensure_no_diagnostics(&config_result.diagnostics);

        let mut file = file_name;
        let mut td = tests_dir.clone();
        if !file_name.ends_with(Path::new("default.txt")) {
          td.push(file_name);
          file = td.as_path().clone();
        }

        return match dprint_plugin_exec::handler::format_text(
          file,
          &file_text,
          &config_result.config,
          |_, _, _| Result::Ok(String::from("")),
        ) {
          Ok(text) => Result::Ok(text),
          Err(err) => Result::Err(ErrBox::from(err)),
        };
      }
    },
    move |_file_name, _file_text, _spec_config| {
      #[cfg(feature = "tracing")]
      {
        let config_result = resolve_config(parse_config_key_map(_spec_config), &global_config);
        ensure_no_diagnostics(&config_result.diagnostics);
        return "ok".to_string();
      }

      #[cfg(not(feature = "tracing"))]
      panic!("\n====\nPlease run with `cargo test --features tracing` to get trace output\n====\n")
    },
  )
}
