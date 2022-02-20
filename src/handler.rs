use std::path::Path;
use std::process::ExitStatus;
use std::process::Stdio;
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
use dprint_core::plugins::PluginHandler;
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
use tokio::time::error::Elapsed;
use tokio::time::timeout;

use crate::configuration::BinaryConfiguration;
use crate::configuration::Configuration;

pub struct ExecHandler;

impl PluginHandler<Configuration> for ExecHandler {
  fn resolve_config(
    &mut self,
    config: ConfigKeyMap,
    global_config: &GlobalConfiguration,
  ) -> ResolveConfigurationResult<Configuration> {
    Configuration::resolve(config, global_config)
  }

  fn get_plugin_info(&mut self) -> PluginInfo {
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

  fn get_license_text(&mut self) -> String {
    String::from(
      "MIT License (MIT)

Copyright (c) 2022 Canva

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.",
    )
  }

  fn format_text(
    &mut self,
    file_path: &Path,
    file_text: &str,
    config: &Configuration,
    mut _format_with_host: impl FnMut(&Path, String, &ConfigKeyMap) -> Result<String>,
  ) -> Result<String> {
    format_text(file_path, file_text, config, _format_with_host)
  }
}

#[tokio::main]
pub async fn format_text(
  file_path: &Path,
  file_text: &str,
  config: &Configuration,
  mut _format_with_host: impl FnMut(&Path, String, &ConfigKeyMap) -> Result<String>,
) -> Result<String> {
  let mut file_text = file_text.to_string();
  for binary in select_binaries(config, file_path)? {
    // format here
    let args = maybe_substitute_variables(file_path, &file_text, config, binary);

    let mut child = Command::new(&binary.executable)
      .current_dir(&binary.cwd)
      .kill_on_drop(true)
      .stdout(Stdio::piped())
      .stdin(if binary.stdin {
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
    if binary.stdin {
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
        Ok(status) => status,
        Err(e) => panic!("Error while waiting for formatter to complete: {}", e),
      }
    });

    file_text = match timeout(Duration::from_secs(config.timeout as u64), out_rx).await {
      Ok(result) => handle_child_exit_status(result?, err_rx, child_completed.await?).await,
      Err(e) => Err(timeout_err(config, e)),
    }?;
  }
  Ok(file_text)
}

fn select_binaries<'a>(
  config: &'a Configuration,
  file_path: &Path,
) -> Result<Vec<&'a BinaryConfiguration>> {
  if !config.is_valid {
    bail!("Cannot format because the configuration was not valid.");
  }

  let mut binaries = Vec::new();

  for binary in &config.binaries {
    if let Some(associations) = &binary.associations {
      if associations.is_match(file_path) {
        binaries.push(binary);
      }
    }
  }

  if binaries.is_empty() {
    if let Some(binary) = config.binaries.iter().find(|b| b.associations.is_none()) {
      binaries.push(binary);
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

fn timeout_err(config: &Configuration, e: Elapsed) -> Error {
  anyhow!(
    "Child process has not returned a result within {} seconds. {}",
    config.timeout,
    e
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
  file_text: &str,
  config: &Configuration,
  binary: &BinaryConfiguration,
) -> Vec<String> {
  let mut handlebars = Handlebars::new();
  handlebars.set_strict_mode(true);

  #[derive(Clone, Serialize, Deserialize)]
  struct TemplateVariables<'a> {
    file_path: String,
    file_text: &'a str,
    line_width: u32,
    use_tabs: bool,
    indent_width: u8,
    new_line_kind: NewLineKind,
    cwd: String,
    stdin: bool,
    timeout: u32,
  }

  let vars = TemplateVariables {
    file_path: file_path.to_string_lossy().to_string(),
    file_text,
    line_width: config.line_width,
    use_tabs: config.use_tabs,
    indent_width: config.indent_width,
    new_line_kind: config.new_line_kind,
    cwd: binary.cwd.to_string_lossy().to_string(),
    stdin: binary.stdin,
    timeout: config.timeout,
  };

  let mut c_args = vec![];
  for arg in &binary.args {
    let formatted = handlebars
      .render_template(arg, &vars)
      .unwrap_or_else(|err| panic!("Cannot format: {}\n\n{}", arg, err));
    c_args.push(formatted);
  }
  c_args
}
