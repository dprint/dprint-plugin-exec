use dprint_core::configuration::get_nullable_value;
use dprint_core::configuration::get_unknown_property_diagnostics;
use dprint_core::configuration::get_value;
use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::ConfigKeyValue;
use dprint_core::configuration::ConfigurationDiagnostic;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::configuration::NewLineKind;
use dprint_core::configuration::ResolveConfigurationResult;
use dprint_core::configuration::RECOMMENDED_GLOBAL_CONFIGURATION;
use globset::GlobMatcher;
use handlebars::Handlebars;
use serde::Serialize;
use serde::Serializer;
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
  pub new_line_kind: NewLineKind,
  /// Formatting commands to run
  pub commands: Vec<CommandConfiguration>,
  pub timeout: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandConfiguration {
  pub command_key: String,
  pub executable: String,
  pub cwd: PathBuf,
  /// Executable arguments to add
  pub args: Vec<String>,
  pub stdin: bool,
  #[serde(serialize_with = "serialize_glob")]
  pub associations: Option<GlobMatcher>,
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
  /// let config_map = ConfigKeyMap::new(); // get a collection of key value pairs from somewhere
  /// let global_config_result = resolve_global_config(config_map, &Default::default());
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
      new_line_kind: get_value(
        &mut config,
        "newLineKind",
        global_config
          .new_line_kind
          .unwrap_or(RECOMMENDED_GLOBAL_CONFIGURATION.new_line_kind),
        &mut diagnostics,
      ),
      commands: Vec::new(),
      timeout: get_value(&mut config, "timeout", 30, &mut diagnostics),
    };

    // the rest of the configuration values are for plugins
    let command_keys = config
      .keys()
      .filter(|c| !c.contains('.'))
      .cloned()
      .collect::<Vec<_>>();
    for command_key in command_keys {
      let mut command = splitty::split_unquoted_whitespace(&get_value(
        &mut config,
        &command_key,
        String::default(),
        &mut diagnostics,
      ))
      .unwrap_quotes(true)
      .filter(|p| !p.is_empty())
      .map(String::from)
      .collect::<Vec<_>>();
      if command.is_empty() {
        diagnostics.push(ConfigurationDiagnostic {
          property_name: command_key.to_string(),
          message: "Expected to find a command name.".to_string(),
        });
        continue;
      }
      resolved_config.commands.push(CommandConfiguration {
        command_key: command_key.clone(),
        executable: command.remove(0),
        args: command,
        associations: {
          let associations_key = format!("{}.associations", command_key);
          let maybe_value = config.remove(&associations_key).and_then(|value| match value {
            ConfigKeyValue::String(value) => Some(value),
            ConfigKeyValue::Array(mut value) => match value.len() {
              0 => None,
              1 => match value.remove(0) {
                ConfigKeyValue::String(value) => Some(value),
                _ => {
                  diagnostics.push(ConfigurationDiagnostic {
                    property_name: associations_key.clone(),
                    message: "Expected string value in array.".to_string(),
                  });
                  None
                }
              },
              _ => {
                diagnostics.push(ConfigurationDiagnostic {
                  property_name: associations_key.clone(),
                  message: "Unfortunately multiple globs haven't been implemented yet. Please provide a single glob or consider contributing this feature."
                    .to_string(),
                });
                None
              }
            },
            _ => {
              diagnostics.push(ConfigurationDiagnostic {
                property_name: associations_key.clone(),
                message: "Expected string or array value.".to_string(),
              });
              None
            }
          });

          match maybe_value {
            Some(value) => {
              let mut builder = globset::GlobBuilder::new(&value);
              builder.case_insensitive(cfg!(windows));
              match builder.build() {
                Ok(glob) => Some(glob.compile_matcher()),
                Err(err) => {
                  diagnostics.push(ConfigurationDiagnostic {
                    message: format!("Error parsing associations glob: {}", err),
                    property_name: associations_key,
                  });
                  None
                }
              }
            }
            None => {
              if resolved_config.commands.iter().any(|b| b.associations.is_none()) {
                diagnostics.push(ConfigurationDiagnostic {
                  property_name: associations_key.to_string(),
                  message: format!(
                    concat!(
                      "A \"{0}\" configuration key must be provided because another ",
                      "formatting command is specified without an associations key. ",
                      "(Example: `\"{0}\": \"**/*.rs\"` would format .rs files with this command)"
                    ),
                    associations_key,
                  ),
                })
              }
              None
            }
          }
        },
        cwd: get_cwd(get_nullable_value(&mut config, &format!("{}.cwd", command_key), &mut diagnostics)),
        stdin: get_value(&mut config, &format!("{}.stdin", command_key), true, &mut diagnostics),
      });
    }

    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(true);

    for command in &resolved_config.commands {
      for arg in &command.args {
        if let Err(e) = handlebars.register_template_string("tmp", arg) {
          diagnostics.push(ConfigurationDiagnostic {
            property_name: "args".to_string(),
            message: format!("Invalid template: {}", e),
          });
        }
        handlebars.unregister_template("tmp");
      }
    }

    diagnostics.extend(get_unknown_property_diagnostics(config));

    resolved_config.is_valid = diagnostics.is_empty();

    ResolveConfigurationResult {
      config: resolved_config,
      diagnostics,
    }
  }
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
  use dprint_core::configuration::NewLineKind;
  use pretty_assertions::assert_eq;

  #[test]
  fn handle_global_config() {
    let global_config = ConfigKeyMap::from([
      ("lineWidth".to_string(), ConfigKeyValue::from_i32(80)),
      ("indentWidth".to_string(), ConfigKeyValue::from_i32(8)),
      ("newLineKind".to_string(), ConfigKeyValue::from_str("crlf")),
      ("useTabs".to_string(), ConfigKeyValue::from_bool(true)),
    ]);
    let global_config = resolve_global_config(global_config, &Default::default()).config;
    let config = Configuration::resolve(ConfigKeyMap::new(), &global_config).config;
    assert_eq!(config.line_width, 80);
    assert_eq!(config.indent_width, 8);
    assert_eq!(config.new_line_kind, NewLineKind::CarriageReturnLineFeed);
    assert!(config.use_tabs);
  }

  #[test]
  fn general_test() {
    let unresolved_config = ConfigKeyMap::from([
      ("cacheKey".to_string(), ConfigKeyValue::from_str("2")),
      ("timeout".to_string(), ConfigKeyValue::from_i32(5)),
    ]);
    let config = Configuration::resolve(unresolved_config, &Default::default()).config;
    assert_eq!(config.line_width, 120);
    assert_eq!(config.indent_width, 2);
    assert_eq!(config.new_line_kind, NewLineKind::LineFeed);
    assert!(!config.use_tabs);
    assert_eq!(config.cache_key, "2");
    assert_eq!(config.timeout, 5);
  }

  #[test]
  fn empty_command_name() {
    let config = ConfigKeyMap::from([("command1".to_string(), ConfigKeyValue::from_str(""))]);
    run_diagnostics_test(
      config,
      vec![ConfigurationDiagnostic {
        property_name: "command1".to_string(),
        message: "Expected to find a command name.".to_string(),
      }],
    )
  }

  #[test]
  fn multiple_binaries_no_associations() {
    let config = ConfigKeyMap::from([
      ("command1".to_string(), ConfigKeyValue::from_str("command1")),
      ("command2".to_string(), ConfigKeyValue::from_str("command2")),
      ("command3".to_string(), ConfigKeyValue::from_str("command3")),
    ]);
    run_diagnostics_test(
      config,
      vec![
        ConfigurationDiagnostic {
          property_name: "command2.associations".to_string(),
          message: concat!(
            "A \"command2.associations\" configuration key must be provided because another formatting ",
            "command is specified without an associations key. (Example: `\"command2.associations\": \"**/*.rs\"` ",
            "would format .rs files with this command)"
          )
          .to_string(),
        },
        ConfigurationDiagnostic {
          property_name: "command3.associations".to_string(),
          message: concat!(
            "A \"command3.associations\" configuration key must be provided because another formatting ",
            "command is specified without an associations key. (Example: `\"command3.associations\": \"**/*.rs\"` ",
            "would format .rs files with this command)"
          )
          .to_string(),
        },
      ],
    )
  }

  #[test]
  fn handle_associations_value() {
    let unresolved_config = ConfigKeyMap::from([
      (
        "command.associations".to_string(),
        ConfigKeyValue::Array(vec![ConfigKeyValue::from_str("**/*.rs")]),
      ),
      ("command".to_string(), ConfigKeyValue::from_str("command")),
    ]);
    let mut config = Configuration::resolve(unresolved_config, &Default::default()).config;
    assert!(config.commands.remove(0).associations.is_some());

    let unresolved_config = ConfigKeyMap::from([
      (
        "command.associations".to_string(),
        ConfigKeyValue::Array(vec![]),
      ),
      ("command".to_string(), ConfigKeyValue::from_str("command")),
    ]);
    let mut config = Configuration::resolve(unresolved_config, &Default::default()).config;
    assert!(config.commands.remove(0).associations.is_none());

    let unresolved_config = ConfigKeyMap::from([
      (
        "command.associations".to_string(),
        ConfigKeyValue::Array(vec![
          ConfigKeyValue::from_str("**/*.rs"),
          ConfigKeyValue::from_str("**/*.json"),
        ]),
      ),
      ("command".to_string(), ConfigKeyValue::from_str("command")),
    ]);
    run_diagnostics_test(
      unresolved_config,
      vec![ConfigurationDiagnostic {
        property_name: "command.associations".to_string(),
        message: "Unfortunately multiple globs haven't been implemented yet. Please provide a single glob or consider contributing this feature.".to_string(),
      }],
    );

    let unresolved_config = ConfigKeyMap::from([
      (
        "command.associations".to_string(),
        ConfigKeyValue::Array(vec![ConfigKeyValue::from_bool(true)]),
      ),
      ("command".to_string(), ConfigKeyValue::from_str("command")),
    ]);
    run_diagnostics_test(
      unresolved_config,
      vec![ConfigurationDiagnostic {
        property_name: "command.associations".to_string(),
        message: "Expected string value in array.".to_string(),
      }],
    );

    let unresolved_config = ConfigKeyMap::from([
      (
        "command.associations".to_string(),
        ConfigKeyValue::from_bool(true),
      ),
      ("command".to_string(), ConfigKeyValue::from_str("command")),
    ]);
    run_diagnostics_test(
      unresolved_config,
      vec![ConfigurationDiagnostic {
        property_name: "command.associations".to_string(),
        message: "Expected string or array value.".to_string(),
      }],
    );
  }

  fn run_diagnostics_test(
    config: ConfigKeyMap,
    expected_diagnostics: Vec<ConfigurationDiagnostic>,
  ) {
    let result = Configuration::resolve(config, &Default::default());
    assert_eq!(result.diagnostics, expected_diagnostics);
    assert!(!result.config.is_valid);
  }
}
