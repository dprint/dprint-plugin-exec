use std::borrow::Cow;
use std::path::Path;
use std::process::ExitStatus;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use anyhow::bail;
use anyhow::Error;
use anyhow::Result;
use dprint_core::configuration::resolve_new_line_kind;
use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::configuration::NewLineKind;
use dprint_core::configuration::ResolveConfigurationResult;
use dprint_core::plugins::AsyncPluginHandler;
use dprint_core::plugins::BoxFuture;
use dprint_core::plugins::CancellationToken;
use dprint_core::plugins::FormatRequest;
use dprint_core::plugins::FormatResult;
use dprint_core::plugins::Host;
use dprint_core::plugins::PluginInfo;
use handlebars::Handlebars;
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Receiver;
use tokio::sync::oneshot::Sender;

use crate::configuration::CommandConfiguration;
use crate::configuration::Configuration;

pub struct ExecHandler;

impl AsyncPluginHandler for ExecHandler {
  type Configuration = Configuration;

  fn plugin_info(&self) -> PluginInfo {
    let name = env!("CARGO_PKG_NAME").to_string();
    let version = env!("CARGO_PKG_VERSION").to_string();
    PluginInfo {
      name: name.clone(),
      version: version.clone(),
      config_key: "exec".to_string(),
      file_extensions: vec![], // this configured in the plugins' `associations`
      file_names: vec![],      // this configured in the plugins' `associations`
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

  fn resolve_config(
    &self,
    config: ConfigKeyMap,
    global_config: GlobalConfiguration,
  ) -> ResolveConfigurationResult<Configuration> {
    Configuration::resolve(config, &global_config)
  }

  fn format(
    &self,
    request: FormatRequest<Self::Configuration>,
    _host: Arc<dyn Host>,
  ) -> BoxFuture<FormatResult> {
    Box::pin(async move {
      if request.range.is_some() {
        // we don't support range formatting for this plugin
        return Ok(None);
      }

      let result = format_text(
        &request.file_path,
        &request.file_text,
        &request.config,
        request.token.clone(),
      )?;
      Ok(if result == request.file_text {
        None
      } else {
        Some(result.to_string())
      })
    })
  }
}

#[tokio::main]
pub async fn format_text<'a>(
  file_path: &Path,
  original_file_text: &'a str,
  config: &Configuration,
  token: Arc<dyn CancellationToken>,
) -> Result<Cow<'a, str>> {
  let mut file_text: Cow<'a, str> = Cow::Borrowed(original_file_text);
  for command in select_commands(config, file_path)? {
    // format here
    let args = maybe_substitute_variables(file_path, config, command);

    let mut child = Command::new(&command.executable)
      .current_dir(&command.cwd)
      .kill_on_drop(true)
      .stdout(Stdio::piped())
      .stdin(if command.stdin {
        Stdio::piped()
      } else {
        Stdio::null()
      })
      .stderr(Stdio::piped())
      .args(args)
      .spawn()
      .map_err(|e| anyhow!("Cannot start formatter process: {}", e))?;

    // capturing stdout
    let (out_tx, out_rx) = oneshot::channel();
    if let Some(stdout) = child.stdout.take() {
      let eol = resolve_new_line_kind(&file_text, config.new_line_kind);
      tokio::spawn(read_stream_lines(stdout, eol, out_tx));
    } else {
      return Err(anyhow!("Formatter did not have a handle for stdout"));
    }

    // capturing stderr
    let (err_tx, err_rx) = oneshot::channel();
    if let Some(stderr) = child.stderr.take() {
      let system_eol = resolve_new_line_kind(&file_text, NewLineKind::System);
      tokio::spawn(read_stream_lines(stderr, system_eol, err_tx));
    }

    // write file text into child's stdin
    if command.stdin {
      child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("Cannot open formatter stdin"))?
        .write_all(file_text.as_bytes())
        .await
        .map_err(|_| anyhow!("Cannot write into formatter stdin"))
        .unwrap();
    }

    // Ensure the child process is spawned in the runtime so it can
    // make progress on its own while we await for any output.
    let child_completed = tokio::spawn(async move {
      match child.wait().await {
        Ok(status) => Ok(status),
        Err(e) => Err(anyhow!(
          "Error while waiting for formatter to complete: {}",
          e
        )),
      }
    });

    tokio::select! {
      _ = token.wait_cancellation() => {
        // return back the original text when cancelled
        return Ok(Cow::Borrowed(original_file_text));
      }
      _ = tokio::time::sleep(Duration::from_secs(config.timeout as u64)) => {
        return Err(timeout_err(config));
      }
      result = out_rx => {
        file_text = Cow::Owned(handle_child_exit_status(result?, err_rx, child_completed.await??).await?)
      }
    }
  }
  Ok(file_text)
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
    }
  }

  if binaries.is_empty() {
    if let Some(command) = config.commands.iter().find(|b| b.associations.is_none()) {
      binaries.push(command);
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
  return Err(anyhow!(
    "Child process exited with code {}: {}",
    exit_status.code().unwrap(),
    err_rx
      .await
      .expect("Could not propagate error message from child process")
  ));
}

fn timeout_err(config: &Configuration) -> Error {
  anyhow!(
    "Child process has not returned a result within {} seconds.",
    config.timeout,
  )
}

async fn read_stream_lines<R>(readable: R, eol: &str, sender: Sender<String>) -> Result<(), String>
where
  R: AsyncRead + Unpin,
{
  let mut reader = BufReader::new(readable).lines();
  let mut formatted = String::new();
  while let Some(line) = reader.next_line().await.unwrap() {
    formatted.push_str(line.as_str());
    formatted.push_str(eol);
  }
  sender.send(formatted)
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
