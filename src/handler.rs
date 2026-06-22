use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Error;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use dprint_core::async_runtime::LocalBoxFuture;
use dprint_core::async_runtime::async_trait;
use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::GlobalConfiguration;
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
use tokio::sync::OnceCell;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Receiver;
use tokio::sync::oneshot::Sender;

use crate::configuration::CommandConfiguration;
use crate::configuration::Configuration;
use crate::configuration::SetupCommand;

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

#[derive(Default)]
pub struct ExecHandler {
  /// Tracks setup commands that have already run so they only run once
  /// for the lifetime of the process, even while formatting in parallel.
  setup_state: SetupState,
}

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

    format_bytes(
      request.file_path,
      request.file_bytes,
      request.config,
      request.token.clone(),
      &self.setup_state,
    )
    .await
  }
}

pub async fn format_bytes(
  file_path: PathBuf,
  original_file_bytes: Vec<u8>,
  config: Arc<Configuration>,
  token: Arc<dyn CancellationToken>,
  setup_state: &SetupState,
) -> FormatResult {
  fn trim_bytes_len(bytes: &[u8]) -> usize {
    let mut start = 0;
    let mut end = bytes.len();

    while start < end && bytes[start].is_ascii_whitespace() {
      start += 1;
    }

    if start == end {
      return 0;
    }

    while end > start && bytes[end - 1].is_ascii_whitespace() {
      end -= 1;
    }

    if end < start { 0 } else { end - start }
  }

  let mut file_bytes: Cow<Vec<u8>> = Cow::Borrowed(&original_file_bytes);
  for command in select_commands(&config, &file_path)? {
    // run the command's setup once before formatting with it for the first time
    if let Some(setup_command) = &command.setup_command {
      match setup_state
        .run_once(&command.cwd, setup_command, &config, &token)
        .await?
      {
        SetupRun::Completed => {}
        SetupRun::Cancelled => return Ok(None),
      }
    }

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
      handles.push(dprint_core::async_runtime::spawn_blocking(|| {
        read_stream_lines(stdout, out_tx)
      }));
    } else {
      let _ = child.kill();
      return Err(anyhow!("Formatter did not have a handle for stdout"));
    }

    // capturing stderr
    let (err_tx, err_rx) = oneshot::channel();
    if let Some(stderr) = child.stderr.take() {
      handles.push(dprint_core::async_runtime::spawn_blocking(|| {
        read_stream_lines(stderr, err_tx)
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
      let file_bytes = file_bytes.into_owned();
      dprint_core::async_runtime::spawn_blocking(move || {
        stdin
          .write_all(&file_bytes)
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
        file_bytes = Cow::Owned(handle_child_exit_status(ok_text, err_rx, exit_status).await?)
      }
    }
  }

  const MIN_CHARS_TO_EMPTY: usize = 100;
  Ok(if *file_bytes == original_file_bytes {
    None
  } else if trim_bytes_len(&original_file_bytes) > MIN_CHARS_TO_EMPTY
    && trim_bytes_len(&file_bytes) == 0
  {
    // prevent someone formatting all their files to empty files
    bail!(
      concat!(
        "The original file text was greater than {} characters, but the formatted text was empty. ",
        "Perhaps dprint-plugin-exec has been misconfigured?",
      ),
      MIN_CHARS_TO_EMPTY
    )
  } else {
    Some(file_bytes.into_owned())
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
  ok_text: Vec<u8>,
  err_rx: Receiver<Vec<u8>>,
  exit_status: ExitStatus,
) -> Result<Vec<u8>, Error> {
  if exit_status.success() {
    return Ok(ok_text);
  }
  Err(anyhow!(
    "Child process exited with code {}: {}",
    exit_status.code().unwrap(),
    String::from_utf8_lossy(
      &err_rx
        .await
        .expect("Could not propagate error message from child process")
    )
  ))
}

fn timeout_err(config: &Configuration) -> Error {
  anyhow!(
    "Child process has not returned a result within {} seconds.",
    config.timeout,
  )
}

/// Remembers which setup commands have already been run so that a command's
/// `setupCommand` only runs a single time, even when many files are being
/// formatted in parallel (see https://github.com/dprint/dprint/issues/1023).
#[derive(Default, Clone)]
pub struct SetupState {
  cells: Rc<RefCell<HashMap<String, Rc<OnceCell<()>>>>>,
}

enum SetupRun {
  Completed,
  Cancelled,
}

enum SetupInitError {
  Cancelled,
  Failed(Error),
}

impl SetupState {
  async fn run_once(
    &self,
    cwd: &Path,
    setup_command: &SetupCommand,
    config: &Configuration,
    token: &Arc<dyn CancellationToken>,
  ) -> Result<SetupRun> {
    // the cwd is part of the key because the same command run in different
    // directories may produce different results
    let key = format!(
      "{}\0{} {}",
      cwd.display(),
      setup_command.executable,
      setup_command.args.join(" ")
    );
    let cell = {
      let mut cells = self.cells.borrow_mut();
      cells.entry(key).or_default().clone()
    };
    // get_or_try_init ensures only one caller runs the setup at a time and that
    // the others wait for it to finish; a failure is not cached so it can be
    // retried by the next file rather than poisoning all formatting
    match cell
      .get_or_try_init(|| run_setup_command(cwd, setup_command, config, token))
      .await
    {
      Ok(()) => Ok(SetupRun::Completed),
      Err(SetupInitError::Cancelled) => Ok(SetupRun::Cancelled),
      Err(SetupInitError::Failed(err)) => Err(err),
    }
  }
}

async fn run_setup_command(
  cwd: &Path,
  setup_command: &SetupCommand,
  config: &Configuration,
  token: &Arc<dyn CancellationToken>,
) -> Result<(), SetupInitError> {
  let mut child = ChildKillOnDrop(
    Command::new(&setup_command.executable)
      .current_dir(cwd)
      .stdin(Stdio::null())
      // a plugin must not write to stdout (it's the protocol channel)
      .stdout(Stdio::null())
      .stderr(Stdio::piped())
      .args(&setup_command.args)
      .spawn()
      .map_err(|e| SetupInitError::Failed(anyhow!("Cannot start setup command process: {}", e)))?,
  );

  // capture stderr to surface it if the command fails
  let (err_tx, err_rx) = oneshot::channel();
  let mut handles = Vec::with_capacity(1);
  if let Some(stderr) = child.stderr.take() {
    handles.push(dprint_core::async_runtime::spawn_blocking(|| {
      read_stream_lines(stderr, err_tx)
    }));
  }

  let child_completed = dprint_core::async_runtime::spawn_blocking(move || {
    child
      .wait()
      .map_err(|e| anyhow!("Error while waiting for setup command to complete: {}", e))
  });

  let result_future = async {
    let handles_future = dprint_core::async_runtime::future::join_all(handles);
    let (child_rs, handle_results) = tokio::join!(child_completed, handles_future);
    let exit_status = child_rs??;
    for handle_result in handle_results {
      handle_result??; // surface any errors capturing
    }
    Ok::<_, Error>(exit_status)
  };

  tokio::select! {
    _ = token.wait_cancellation() => Err(SetupInitError::Cancelled),
    _ = tokio::time::sleep(Duration::from_secs(config.timeout as u64)) => {
      Err(SetupInitError::Failed(anyhow!(
        "Setup command has not returned a result within {} seconds.",
        config.timeout,
      )))
    }
    result = result_future => match result {
      Ok(exit_status) if exit_status.success() => Ok(()),
      Ok(exit_status) => Err(SetupInitError::Failed(anyhow!(
        "Setup command '{}' exited with code {}: {}",
        setup_command.executable,
        exit_status
          .code()
          .map(|code| code.to_string())
          .unwrap_or_else(|| "unknown".to_string()),
        String::from_utf8_lossy(&err_rx.await.unwrap_or_default())
      ))),
      Err(err) => Err(SetupInitError::Failed(err)),
    }
  }
}

fn read_stream_lines<R>(mut readable: R, sender: Sender<Vec<u8>>) -> Result<(), Error>
where
  R: std::io::Read + Unpin,
{
  let mut bytes = Vec::new();
  readable.read_to_end(&mut bytes)?;
  let _ignore = sender.send(bytes); // ignore error as that means the other end is closed
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

  use super::SetupState;
  use crate::configuration::Configuration;
  use crate::format_bytes;

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
    let result = format_bytes(
      PathBuf::from("path.txt"),
      "1".repeat(101).into_bytes(),
      Arc::new(config),
      token,
      &SetupState::default(),
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

  #[tokio::test]
  async fn runs_setup_command_once_across_formats() {
    // forward slashes work cross-platform for these tools and avoid splitty
    // treating backslashes in Windows paths as escapes
    fn to_arg(path: &std::path::Path) -> String {
      path.to_string_lossy().replace('\\', "/")
    }

    let marker = std::env::temp_dir().join(format!(
      "dprint-exec-setup-marker-{}.txt",
      std::process::id()
    ));
    let _ = std::fs::remove_file(&marker);
    let script = std::env::current_dir()
      .unwrap()
      .join("tests/resources/append-marker.js");

    let unresolved_config = serde_json::json!({
      "commands": [{
        "command": "deno run -A ./tests/fold.ts -w 30",
        "setupCommand": format!("deno run -A {} {}", to_arg(&script), to_arg(&marker)),
        "exts": ["txt"]
      }]
    });
    let unresolved_config = serde_json::from_value(unresolved_config).unwrap();
    let config = Arc::new(Configuration::resolve(unresolved_config, &Default::default()).config);
    let setup_state = SetupState::default();

    // format two different files sharing the same setup state
    for file_name in ["a.txt", "b.txt"] {
      let result = format_bytes(
        PathBuf::from(file_name),
        b"hello world".to_vec(),
        config.clone(),
        Arc::new(NullCancellationToken),
        &setup_state,
      )
      .await;
      assert!(result.is_ok(), "{:?}", result.err());
    }

    // the setup command should have run exactly once
    let marker_contents = std::fs::read_to_string(&marker).unwrap();
    let _ = std::fs::remove_file(&marker);
    assert_eq!(marker_contents, "x");
  }
}
