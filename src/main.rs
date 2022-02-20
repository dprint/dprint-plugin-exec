use anyhow::Result;
use dprint_core::plugins::process::{
  handle_process_stdio_messages, start_parent_process_checker_thread,
};
use dprint_plugin_exec::handler::ExecHandler;

fn main() -> Result<()> {
  let args: Vec<String> = std::env::args().collect();
  let parent_process_id = get_parent_process_id_from_args(&args);
  start_parent_process_checker_thread(parent_process_id);

  handle_process_stdio_messages(ExecHandler)
}

fn get_parent_process_id_from_args(args: &[String]) -> u32 {
  for i in 0..args.len() {
    if args[i] == "--parent-pid" {
      if let Some(parent_pid) = args.get(i + 1) {
        return parent_pid
          .parse::<u32>()
          .expect("could not parse the parent process id");
      }
    }
  }

  panic!("please provide a --parent-pid <id> flag")
}
