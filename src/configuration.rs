use dprint_core::configuration::{
  get_nullable_value, get_unknown_property_diagnostics, get_value, ConfigKeyMap,
  ConfigurationDiagnostic, GlobalConfiguration, NewLineKind, ResolveConfigurationResult,
  DEFAULT_GLOBAL_CONFIGURATION,
};
use handlebars::Handlebars;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
  pub line_width: u32,
  pub use_tabs: bool,
  pub indent_width: u8,
  pub new_line_kind: NewLineKind,
  /// Formatting program to run
  pub binaries: Vec<BinaryConfiguration>,
  pub timeout: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryConfiguration {
  pub executable: String,
  pub cwd: PathBuf,
  /// Executable arguments to add
  pub args: Vec<String>,
  pub stdin: bool,
  pub associations: String,
}

impl Configuration {
  /// Resolves configuration from a collection of key value strings.
  ///
  /// # Example
  ///
  /// ```
  /// use std::collections::HashMap;
  /// use dprint_core::configuration::{resolve_global_config};
  /// use dprint_plugin_exec::configuration::Configuration;
  ///
  /// let config_map = HashMap::new(); // get a collection of key value pairs from somewhere
  /// let global_config_result = resolve_global_config(config_map, &Default::default());
  ///
  /// // check global_config_result.diagnostics here...
  ///
  /// let exec_config_map = HashMap::new(); // get a collection of k/v pairs from somewhere
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
      line_width: get_value(
        &mut config,
        "lineWidth",
        global_config
          .line_width
          .unwrap_or(DEFAULT_GLOBAL_CONFIGURATION.line_width),
        &mut diagnostics,
      ),
      use_tabs: get_value(
        &mut config,
        "useTabs",
        global_config
          .use_tabs
          .unwrap_or(DEFAULT_GLOBAL_CONFIGURATION.use_tabs),
        &mut diagnostics,
      ),
      indent_width: get_value(
        &mut config,
        "indentWidth",
        global_config
          .indent_width
          .unwrap_or(DEFAULT_GLOBAL_CONFIGURATION.indent_width),
        &mut diagnostics,
      ),
      new_line_kind: get_value(
        &mut config,
        "newLineKind",
        global_config
          .new_line_kind
          .unwrap_or(DEFAULT_GLOBAL_CONFIGURATION.new_line_kind),
        &mut diagnostics,
      ),
      binaries: Vec::new(),
      timeout: get_value(&mut config, "timeout", 30, &mut diagnostics),
    };

    // the rest of the configuration values are for plugins
    let binary_keys = config
      .keys()
      .filter(|c| !c.contains('.'))
      .cloned()
      .collect::<Vec<_>>();
    for binary_key in binary_keys {
      let mut command = splitty::split_unquoted_whitespace(&get_value(
        &mut config,
        &binary_key,
        String::default(),
        &mut diagnostics,
      ))
      .unwrap_quotes(true)
      .filter(|p| !p.is_empty())
      .map(String::from)
      .collect::<Vec<_>>();
      if command.is_empty() {
        diagnostics.push(ConfigurationDiagnostic {
          property_name: binary_key.to_string(),
          message: "Expected to find a command name.".to_string(),
        });
        continue;
      }
      resolved_config.binaries.push(BinaryConfiguration {
        executable: command.remove(0),
        args: command,
        associations: get_value(
          &mut config,
          &format!("{}.associations", binary_key),
          String::default(),
          &mut diagnostics,
        ),
        cwd: get_cwd(get_nullable_value(
          &mut config,
          &format!("{}.cwd", binary_key),
          &mut diagnostics,
        )),
        stdin: get_value(
          &mut config,
          &format!("{}.stdin", binary_key),
          false,
          &mut diagnostics,
        ),
      });
    }

    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(true);

    for binary in &resolved_config.binaries {
      for arg in &binary.args {
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

    ResolveConfigurationResult {
      config: resolved_config,
      diagnostics,
    }
  }
}

fn get_cwd(dir: Option<String>) -> PathBuf {
  match dir {
    Some(dir) => PathBuf::from(dir),
    None => std::env::current_dir()
      .expect("should get cwd")
      .to_path_buf(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use dprint_core::configuration::{resolve_global_config, ConfigKeyValue, NewLineKind};
  use std::collections::HashMap;

  #[test]
  fn handle_global_config() {
    let global_config = HashMap::from([
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
}
