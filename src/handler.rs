use std::borrow::Cow;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use anyhow::bail;
use anyhow::Error;
use anyhow::Result;
use dprint_core::async_runtime::async_trait;
use dprint_core::async_runtime::LocalBoxFuture;
use dprint_core::configuration::resolve_new_line_kind;
use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::configuration::NewLineKind;
use dprint_core::plugins::AsyncPluginHandler;
use dprint_core::plugins::CancellationToken;
use dprint_core::plugins::FileMatchingInfo;
use dprint_core::plugins::FormatRequest;
use dprint_core::plugins::FormatResult;
use dprint_core::plugins::HostFormatRequest;
use dprint_core::plugins::PluginInfo;
use dprint_core::plugins::PluginResolveConfigurationResult;
use handlebars::Handlebars;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Receiver;
use tokio::sync::oneshot::Sender;

use crate::configuration::CommandConfiguration;
use crate::configuration::Configuration;

struct ChildKillOnDrop(std::process::Child);

impl Drop for ChildKillOnDrop {
  fn drop(&mut self) {
    let _ignore = self.0.kill();
  }
}

impl Deref for ChildKillOnDrop {
  type Target = std::process::Child;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl DerefMut for ChildKillOnDrop {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

pub struct ExecHandler;

#[async_trait(?Send)]
impl AsyncPluginHandler for ExecHandler {
  type Configuration = Configuration;

  fn plugin_info(&self) -> PluginInfo {
    let name = env!("CARGO_PKG_NAME").to_string();
    let version = env!("CARGO_PKG_VERSION").to_string();
    PluginInfo {
      name: name.clone(),
      version: version.clone(),
      config_key: "exec".to_string(),
      help_url: env!("CARGO_PKG_HOMEPAGE").to_string(),
      config_schema_url: format!(
        "https://plugins.dprint.dev/dprint/{}/{}/schema.json",
        name, version
      ),
      update_url: Some(format!(
        "https://plugins.dprint.dev/dprint/{}/latest.json",
        name
      )),
    }
  }

  fn license_text(&self) -> String {
    include_str!("../LICENSE").to_string()
  }

  async fn resolve_config(
    &self,
    config: ConfigKeyMap,
    global_config: GlobalConfiguration,
  ) -> PluginResolveConfigurationResult<Configuration> {
    let result = Configuration::resolve(config, &global_config);
    let config = result.config;
    PluginResolveConfigurationResult {
      file_matching: FileMatchingInfo {
        file_extensions: config
          .commands
          .iter()
          .flat_map(|c| c.file_extensions.iter())
          .map(|s| s.trim_start_matches('.').to_string())
          .collect(),
        file_names: config
          .commands
          .iter()
          .flat_map(|c| c.file_names.iter())
          .map(|s| s.to_string())
          .collect(),
      },
      config,
      diagnostics: result.diagnostics,
    }
  }

  async fn format(
    &self,
    request: FormatRequest<Self::Configuration>,
    _format_with_host: impl FnMut(HostFormatRequest) -> LocalBoxFuture<'static, FormatResult> + 'static,
  ) -> FormatResult {
    if request.range.is_some() {
      // we don't support range formatting for this plugin
      return Ok(None);
    }

    format_text(
      request.file_path,
      request.file_text,
      request.config,
      request.token.clone(),
    )
    .await
  }
}

pub async fn format_text(
  file_path: PathBuf,
  original_file_text: String,
  config: Arc<Configuration>,
  token: Arc<dyn CancellationToken>,
) -> FormatResult {
  let mut file_text: Cow<str> = Cow::Borrowed(&original_file_text);
  for command in select_commands(&config, &file_path)? {
    // format here
    let args = maybe_substitute_variables(&file_path, &config, command);

    let mut child = ChildKillOnDrop(
      Command::new(&command.executable)
        .current_dir(&command.cwd)
        .stdout(Stdio::piped())
        .stdin(if command.stdin {
          Stdio::piped()
        } else {
          Stdio::null()
        })
        .stderr(Stdio::piped())
        .args(args)
        .spawn()
        .map_err(|e| anyhow!("Cannot start formatter process: {}", e))?,
    );

    // capturing stdout
    let (out_tx, out_rx) = oneshot::channel();
    let mut handles = Vec::with_capacity(2);
    if let Some(stdout) = child.stdout.take() {
      let eol = resolve_new_line_kind(&file_text, config.new_line_kind);
      handles.push(dprint_core::async_runtime::spawn_blocking(|| {
        read_stream_lines(stdout, eol, out_tx)
      }));
    } else {
      let _ = child.kill();
      return Err(anyhow!("Formatter did not have a handle for stdout"));
    }

    // capturing stderr
    let (err_tx, err_rx) = oneshot::channel();
    if let Some(stderr) = child.stderr.take() {
      let system_eol = resolve_new_line_kind(&file_text, NewLineKind::System);
      handles.push(dprint_core::async_runtime::spawn_blocking(|| {
        read_stream_lines(stderr, system_eol, err_tx)
      }));
    }

    // write file text into child's stdin
    if command.stdin {
      let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| {
          anyhow!(
            "Cannot open the command's stdin. Perhaps you meant to set the command's \"stdin\" configuration to false?",
          )
        })?;
      let file_text = file_text.to_string();
      dprint_core::async_runtime::spawn_blocking(move || {
        stdin
          .write_all(file_text.as_bytes())
          .map_err(|err| anyhow!("Cannot write into the command's stdin. {}", err))
      })
      .await??;
    }

    let child_completed = dprint_core::async_runtime::spawn_blocking(move || match child.wait() {
      Ok(status) => Ok(status),
      Err(e) => Err(anyhow!(
        "Error while waiting for formatter to complete: {}",
        e
      )),
    });

    let result_future = async {
      let handles_future = dprint_core::async_runtime::future::join_all(handles);
      let (output_result, child_rs, handle_results) =
        tokio::join!(out_rx, child_completed, handles_future);
      let exit_status = child_rs??;
      let output = output_result?;
      for handle_result in handle_results {
        handle_result??; // surface any errors capturing
      }
      Ok::<_, Error>((output, exit_status))
    };

    tokio::select! {
      _ = token.wait_cancellation() => {
        // return back the original text when cancelled
        return Ok(None);
      }
      _ = tokio::time::sleep(Duration::from_secs(config.timeout as u64)) => {
        return Err(timeout_err(&config));
      }
      result = result_future => {
        let (ok_text, exit_status) = result?;
        file_text = Cow::Owned(handle_child_exit_status(ok_text, err_rx, exit_status).await?)
      }
    }
  }

  const MIN_CHARS_TO_EMPTY: usize = 100;
  Ok(if file_text == original_file_text {
    None
  } else if original_file_text.trim().len() > MIN_CHARS_TO_EMPTY && file_text.trim().is_empty() {
    // prevent someone formatting all their files to empty files
    bail!(
      concat!(
        "The original file text was greater than {} characters, but the formatted text was empty. ",
        "Perhaps dprint-plugin-exec has been misconfigured?",
      ),
      MIN_CHARS_TO_EMPTY
    )
  } else {
    Some(file_text.to_string())
  })
}

fn select_commands<'a>(
  config: &'a Configuration,
  file_path: &Path,
) -> Result<Vec<&'a CommandConfiguration>> {
  if !config.is_valid {
    bail!("Cannot format because the configuration was not valid.");
  }

  let mut binaries = Vec::new();

  for command in &config.commands {
    if let Some(associations) = &command.associations {
      if associations.is_match(file_path) {
        binaries.push(command);
      }
    } else if binaries.is_empty() && command.matches_exts_or_filenames(file_path) {
      binaries.push(command);
      break;
    }
  }

  Ok(binaries)
}

async fn handle_child_exit_status(
  ok_text: String,
  err_rx: Receiver<String>,
  exit_status: ExitStatus,
) -> Result<String, Error> {
  if exit_status.success() {
    return Ok(ok_text);
  }
  Err(anyhow!(
    "Child process exited with code {}: {}",
    exit_status.code().unwrap(),
    err_rx
      .await
      .expect("Could not propagate error message from child process")
  ))
}

fn timeout_err(config: &Configuration) -> Error {
  anyhow!(
    "Child process has not returned a result within {} seconds.",
    config.timeout,
  )
}

fn read_stream_lines<R>(readable: R, eol: &str, sender: Sender<String>) -> Result<(), Error>
where
  R: std::io::Read + Unpin,
{
  let mut reader = BufReader::new(readable).lines();
  let mut formatted = String::new();
  while let Some(line) = reader.next() {
    formatted.push_str(line?.as_str());
    formatted.push_str(eol);
  }
  let _ignore = sender.send(formatted); // ignore error as that means the other end is closed
  Ok(())
}

fn maybe_substitute_variables(
  file_path: &Path,
  config: &Configuration,
  command: &CommandConfiguration,
) -> Vec<String> {
  let mut handlebars = Handlebars::new();
  handlebars.set_strict_mode(true);

  #[derive(Clone, Serialize, Deserialize)]
  struct TemplateVariables {
    file_path: String,
    line_width: u32,
    use_tabs: bool,
    indent_width: u8,
    cwd: String,
    timeout: u32,
  }

  let vars = TemplateVariables {
    file_path: file_path.to_string_lossy().to_string(),
    line_width: config.line_width,
    use_tabs: config.use_tabs,
    indent_width: config.indent_width,
    cwd: command.cwd.to_string_lossy().to_string(),
    timeout: config.timeout,
  };

  let mut c_args = vec![];
  for arg in &command.args {
    let formatted = handlebars
      .render_template(arg, &vars)
      .unwrap_or_else(|err| panic!("Cannot format: {}\n\n{}", arg, err));
    c_args.push(formatted);
  }
  c_args
}

#[cfg(test)]
mod test {
  use std::path::PathBuf;
  use std::sync::Arc;

  use dprint_core::plugins::NullCancellationToken;

  use crate::configuration::Configuration;
  use crate::format_text;

  #[tokio::test]
  async fn should_error_output_empty_file() {
    let token = Arc::new(NullCancellationToken);
    let unresolved_config = r#"{
      "commands": [{
        "command": "deno eval 'Deno.exit(0)'",
        "exts": ["txt"]
      }]
    }"#;
    let unresolved_config = serde_json::from_str(unresolved_config).unwrap();
    let config = Configuration::resolve(unresolved_config, &Default::default()).config;
    let result = format_text(
      PathBuf::from("path.txt"),
      "1".repeat(101),
      Arc::new(config),
      token,
    )
    .await;
    let err_text = result.err().unwrap().to_string();
    assert_eq!(
      err_text,
      concat!(
        "The original file text was greater than 100 characters, ",
        "but the formatted text was empty. ",
        "Perhaps dprint-plugin-exec has been misconfigured?"
      )
    )
  }
}
