use anyhow::Result;
use dprint_core::plugins::process::get_parent_process_id_from_cli_args;
use dprint_core::plugins::process::handle_process_stdio_messages;
use dprint_core::plugins::process::start_parent_process_checker_task;
use dprint_plugin_exec::handler::ExecHandler;

#[tokio::main]
async fn main() -> Result<()> {
  if let Some(parent_process_id) = get_parent_process_id_from_cli_args() {
    start_parent_process_checker_task(parent_process_id);
  }

  handle_process_stdio_messages(ExecHandler).await
}
