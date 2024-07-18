mod utils;

mod args;
mod eu_stack;
mod gdb;
mod uniquify;

use crate::args::parse_args;
use crate::eu_stack::run_eustack;
use gdb::run_gdb;
use uniquify::uniquify_stack_files;
use utils::list_process;

#[tokio::main]
async fn main() {
    let _ = utils::get_terminal_size(); // must be done before setup pager
    let cli = parse_args(std::env::args());

    if cli.list {
        list_process(cli).await;
    } else if !cli.files.is_empty() {
        uniquify_stack_files(cli).await;
    } else if cli.gdb_mode {
        run_gdb(&cli).await;
    } else {
        run_eustack(&cli).await;
    }
}
