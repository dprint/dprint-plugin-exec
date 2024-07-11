use dprint_core::configuration::get_nullable_value;
use dprint_core::configuration::get_unknown_property_diagnostics;
use dprint_core::configuration::get_value;
use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::ConfigKeyValue;
use dprint_core::configuration::ConfigurationDiagnostic;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::configuration::ResolveConfigurationResult;
use dprint_core::configuration::RECOMMENDED_GLOBAL_CONFIGURATION;
use globset::GlobMatcher;
use handlebars::Handlebars;
use serde::Serialize;
use serde::Serializer;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
  /// Doesn't allow formatting unless the configuration had no diagnostics.
  pub is_valid: bool,
  pub cache_key: String,
  pub line_width: u32,
  pub use_tabs: bool,
  pub indent_width: u8,
  /// Formatting commands to run
  pub commands: Vec<CommandConfiguration>,
  pub timeout: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandConfiguration {
  pub executable: String,
  /// Executable arguments to add
  pub args: Vec<String>,
  pub cwd: PathBuf,
  pub stdin: bool,
  #[serde(serialize_with = "serialize_glob")]
  pub associations: Option<GlobMatcher>,
  pub file_extensions: Vec<String>,
  pub file_names: Vec<String>,
}

impl CommandConfiguration {
  pub fn matches_exts_or_filenames(&self, path: &Path) -> bool {
    if let Some(filename) = path.file_name() {
      let filename = filename.to_string_lossy().to_lowercase();
      for ext in &self.file_extensions {
        if filename.ends_with(ext) {
          return true;
        }
      }
      self.file_names.iter().any(|name| name == &filename)
    } else {
      false
    }
  }
}

fn serialize_glob<S: Serializer>(value: &Option<GlobMatcher>, s: S) -> Result<S::Ok, S::Error> {
  match value {
    Some(value) => s.serialize_str(value.glob().glob()),
    None => s.serialize_none(),
  }
}

impl Configuration {
  /// Resolves configuration from a collection of key value strings.
  ///
  /// # Example
  ///
  /// ```
  /// use dprint_core::configuration::ConfigKeyMap;
  /// use dprint_core::configuration::resolve_global_config;
  /// use dprint_plugin_exec::configuration::Configuration;
  ///
  /// let mut config_map = ConfigKeyMap::new(); // get a collection of key value pairs from somewhere
  /// let global_config_result = resolve_global_config(&mut config_map);
  ///
  /// // check global_config_result.diagnostics here...
  ///
  /// let exec_config_map = ConfigKeyMap::new(); // get a collection of k/v pairs from somewhere
  /// let config_result = Configuration::resolve(
  ///     exec_config_map,
  ///     &global_config_result.config
  /// );
  ///
  /// // check config_result.diagnostics here and use config_result.config
  /// ```
  pub fn resolve(
    config: ConfigKeyMap,
    global_config: &GlobalConfiguration,
  ) -> ResolveConfigurationResult<Configuration> {
    let mut diagnostics = vec![];
    let mut config = config;

    let mut resolved_config = Configuration {
      is_valid: true,
      cache_key: get_value(&mut config, "cacheKey", "0".to_string(), &mut diagnostics),
      line_width: get_value(
        &mut config,
        "lineWidth",
        global_config
          .line_width
          .unwrap_or(RECOMMENDED_GLOBAL_CONFIGURATION.line_width),
        &mut diagnostics,
      ),
      use_tabs: get_value(
        &mut config,
        "useTabs",
        global_config
          .use_tabs
          .unwrap_or(RECOMMENDED_GLOBAL_CONFIGURATION.use_tabs),
        &mut diagnostics,
      ),
      indent_width: get_value(
        &mut config,
        "indentWidth",
        global_config
          .indent_width
          .unwrap_or(RECOMMENDED_GLOBAL_CONFIGURATION.indent_width),
        &mut diagnostics,
      ),
      commands: Vec::new(),
      timeout: get_value(&mut config, "timeout", 30, &mut diagnostics),
    };

    let root_cwd = get_nullable_value(&mut config, "cwd", &mut diagnostics);

    if let Some(commands) = config.swap_remove("commands").and_then(|c| c.into_array()) {
      for (i, element) in commands.into_iter().enumerate() {
        let Some(command_obj) = element.into_object() else {
          diagnostics.push(ConfigurationDiagnostic {
            property_name: "commands".to_string(),
            message: "Expected to find only objects in the array.".to_string(),
          });
          continue;
        };
        let result = parse_command_obj(command_obj, root_cwd.as_ref());
        diagnostics.extend(result.1.into_iter().map(|mut diagnostic| {
          diagnostic.property_name = format!("commands[{}].{}", i, diagnostic.property_name);
          diagnostic
        }));
        if let Some(command_config) = result.0 {
          resolved_config.commands.push(command_config);
        }
      }
    } else {
      diagnostics.push(ConfigurationDiagnostic {
        property_name: "commands".to_string(),
        message: "Expected to find a \"commands\" array property (see https://github.com/dprint/dprint-plugin-exec for instructions)".to_string(),
      });
    }

    diagnostics.extend(get_unknown_property_diagnostics(config));

    resolved_config.is_valid = diagnostics.is_empty();

    ResolveConfigurationResult {
      config: resolved_config,
      diagnostics,
    }
  }
}

fn parse_command_obj(
  mut command_obj: ConfigKeyMap,
  root_cwd: Option<&String>,
) -> (Option<CommandConfiguration>, Vec<ConfigurationDiagnostic>) {
  let mut diagnostics = Vec::new();
  let mut command = splitty::split_unquoted_whitespace(&get_value(
    &mut command_obj,
    "command",
    String::default(),
    &mut diagnostics,
  ))
  .unwrap_quotes(true)
  .filter(|p| !p.is_empty())
  .map(String::from)
  .collect::<Vec<_>>();
  if command.is_empty() {
    diagnostics.push(ConfigurationDiagnostic {
      property_name: "command".to_string(),
      message: "Expected to find a command name.".to_string(),
    });
    return (None, diagnostics);
  }

  {
    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(true);
    for arg in command.iter().skip(1) {
      if let Err(e) = handlebars.register_template_string("tmp", arg) {
        diagnostics.push(ConfigurationDiagnostic {
          property_name: "command".to_string(),
          message: format!("Invalid template: {}", e),
        });
      }
      handlebars.unregister_template("tmp");
    }
  }

  let config = CommandConfiguration {
    executable: command.remove(0),
    args: command,
    associations: {
      let maybe_value = command_obj.swap_remove("associations").and_then(|value| match value {
        ConfigKeyValue::String(value) => Some(value),
        ConfigKeyValue::Array(mut value) => match value.len() {
          0 => None,
          1 => match value.remove(0) {
            ConfigKeyValue::String(value) => Some(value),
            _ => {
              diagnostics.push(ConfigurationDiagnostic {
                property_name: "associations".to_string(),
                message: "Expected string value in array.".to_string(),
              });
              None
            }
          },
          _ => {
            diagnostics.push(ConfigurationDiagnostic {
              property_name: "associations".to_string(),
              message: "Unfortunately multiple globs haven't been implemented yet. Please provide a single glob or consider contributing this feature."
                .to_string(),
            });
            None
          }
        },
        _ => {
          diagnostics.push(ConfigurationDiagnostic {
            property_name: "associations".to_string(),
            message: "Expected string or array value.".to_string(),
          });
          None
        }
      });

      maybe_value.and_then(|value| {
        let mut builder = globset::GlobBuilder::new(&value);
        builder.case_insensitive(cfg!(windows));
        match builder.build() {
          Ok(glob) => Some(glob.compile_matcher()),
          Err(err) => {
            diagnostics.push(ConfigurationDiagnostic {
              message: format!("Error parsing associations glob: {:#}", err),
              property_name: "associations".to_string(),
            });
            None
          }
        }
      })
    },
    cwd: get_cwd(
      get_nullable_value(&mut command_obj, "cwd", &mut diagnostics)
        .or_else(|| root_cwd.map(ToOwned::to_owned)),
    ),
    stdin: get_value(&mut command_obj, "stdin", true, &mut diagnostics),
    file_extensions: take_string_or_string_vec(&mut command_obj, "exts", &mut diagnostics)
      .into_iter()
      .map(|ext| {
        if ext.starts_with('.') {
          ext
        } else {
          format!(".{}", ext)
        }
      })
      .collect::<Vec<_>>(),
    file_names: take_string_or_string_vec(&mut command_obj, "fileNames", &mut diagnostics),
  };
  diagnostics.extend(get_unknown_property_diagnostics(command_obj));

  if diagnostics.is_empty()
    && config.file_names.is_empty()
    && config.file_extensions.is_empty()
    && config.associations.is_none()
  {
    diagnostics.push(ConfigurationDiagnostic {
      property_name: "exts".to_string(),
      message: "You must specify either: exts (recommended), fileNames, or associations"
        .to_string(),
    })
  }

  (Some(config), diagnostics)
}

fn take_string_or_string_vec(
  command_obj: &mut ConfigKeyMap,
  key: &str,
  diagnostics: &mut Vec<ConfigurationDiagnostic>,
) -> Vec<String> {
  command_obj
    .swap_remove(key)
    .map(|values| match values {
      ConfigKeyValue::String(value) => vec![value],
      ConfigKeyValue::Array(elements) => {
        let mut values = Vec::with_capacity(elements.len());
        for (i, element) in elements.into_iter().enumerate() {
          match element {
            ConfigKeyValue::String(value) => {
              values.push(value);
            }
            _ => diagnostics.push(ConfigurationDiagnostic {
              property_name: format!("{}[{}]", key, i),
              message: "Expected string element.".to_string(),
            }),
          }
        }
        values
      }
      _ => {
        diagnostics.push(ConfigurationDiagnostic {
          property_name: key.to_string(),
          message: "Expected string or array value.".to_string(),
        });
        vec![]
      }
    })
    .unwrap_or_default()
}

fn get_cwd(dir: Option<String>) -> PathBuf {
  match dir {
    Some(dir) => PathBuf::from(dir),
    None => std::env::current_dir().expect("should get cwd"),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use dprint_core::configuration::resolve_global_config;
  use dprint_core::configuration::ConfigKeyValue;
  use pretty_assertions::assert_eq;
  use serde_json::json;

  #[test]
  fn handle_global_config() {
    let mut global_config = ConfigKeyMap::from([
      ("lineWidth".to_string(), ConfigKeyValue::from_i32(80)),
      ("indentWidth".to_string(), ConfigKeyValue::from_i32(8)),
      ("useTabs".to_string(), ConfigKeyValue::from_bool(true)),
    ]);
    let global_config = resolve_global_config(&mut global_config).config;
    let config = Configuration::resolve(ConfigKeyMap::new(), &global_config).config;
    assert_eq!(config.line_width, 80);
    assert_eq!(config.indent_width, 8);
    assert!(config.use_tabs);
  }

  #[test]
  fn general_test() {
    let unresolved_config = parse_config(json!({
      "cacheKey": "2",
      "timeout": 5
    }));
    let result = Configuration::resolve(unresolved_config, &Default::default());
    let config = result.config;
    assert_eq!(config.line_width, 120);
    assert_eq!(config.indent_width, 2);
    assert!(!config.use_tabs);
    assert_eq!(config.cache_key, "2");
    assert_eq!(config.timeout, 5);
    assert_eq!(result.diagnostics, vec![ConfigurationDiagnostic {
      property_name: "commands".to_string(),
      message: "Expected to find a \"commands\" array property (see https://github.com/dprint/dprint-plugin-exec for instructions)".to_string(),
    }]);
  }

  #[test]
  fn empty_command_name() {
    let config = parse_config(json!({
      "commands": [{
        "command": "",
      }],
    }));
    run_diagnostics_test(
      config,
      vec![ConfigurationDiagnostic {
        property_name: "commands[0].command".to_string(),
        message: "Expected to find a command name.".to_string(),
      }],
    )
  }

  #[test]
  fn cwd_test() {
    let unresolved_config = parse_config(json!({
      "cwd": "test-cwd",
      "commands": [{
        "command": "1"
      }, {
        "cwd": "test-cwd2",
        "command": "1"
      }]
    }));
    let result = Configuration::resolve(unresolved_config, &Default::default());
    let config = result.config;
    assert_eq!(config.commands[0].cwd, PathBuf::from("test-cwd"));
    assert_eq!(config.commands[1].cwd, PathBuf::from("test-cwd2"));
  }

  #[test]
  fn handle_associations_value() {
    let unresolved_config = parse_config(json!({
      "commands": [{
        "command": "command",
        "associations": ["**/*.rs"]
      }],
    }));
    let mut config = Configuration::resolve(unresolved_config, &Default::default()).config;
    assert!(config.commands.remove(0).associations.is_some());

    let unresolved_config = parse_config(json!({
      "commands": [{
        "command": "command",
        "associations": []
      }],
    }));
    let mut config = Configuration::resolve(unresolved_config, &Default::default()).config;
    assert!(config.commands.remove(0).associations.is_none());

    let unresolved_config = parse_config(json!({
      "commands": [{
        "command": "command",
        "associations": [
          "**/*.rs",
          "**/*.json",
        ]
      }],
    }));
    run_diagnostics_test(
      unresolved_config,
      vec![ConfigurationDiagnostic {
        property_name: "commands[0].associations".to_string(),
        message: "Unfortunately multiple globs haven't been implemented yet. Please provide a single glob or consider contributing this feature.".to_string(),
      }],
    );

    let unresolved_config = parse_config(json!({
      "commands": [{
        "command": "command",
        "associations": [true]
      }],
    }));
    run_diagnostics_test(
      unresolved_config,
      vec![ConfigurationDiagnostic {
        property_name: "commands[0].associations".to_string(),
        message: "Expected string value in array.".to_string(),
      }],
    );

    let unresolved_config = parse_config(json!({
      "commands": [{
        "command": "command",
        "associations": true
      }],
    }));
    run_diagnostics_test(
      unresolved_config,
      vec![ConfigurationDiagnostic {
        property_name: "commands[0].associations".to_string(),
        message: "Expected string or array value.".to_string(),
      }],
    );
  }

  #[track_caller]
  fn run_diagnostics_test(
    config: ConfigKeyMap,
    expected_diagnostics: Vec<ConfigurationDiagnostic>,
  ) {
    let result = Configuration::resolve(config, &Default::default());
    assert_eq!(result.diagnostics, expected_diagnostics);
    assert!(!result.config.is_valid);
  }

  fn parse_config(value: serde_json::Value) -> ConfigKeyMap {
    serde_json::from_value(value).unwrap()
  }
}
