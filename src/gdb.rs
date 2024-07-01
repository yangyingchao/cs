use crate::{
    args::Cli,
    uniquify::{simplify_stack, uniquify_gdb},
    utils::execute_command,
};

fn do_run_gdb(args: Vec<&str>, unique: bool, raw: bool) -> Result<(), String> {
    match execute_command("gdb", args) {
        Ok(result) => {
            let (code, out, err) = result;
            if code <= 1 {
                let out = if raw { out } else { simplify_stack(out) };
                if unique {
                    return uniquify_gdb(&out);
                } else {
                    println!("{}", out);
                }
                if !err.is_empty() {
                    eprintln!("Warnings reported: {err}");
                }
                Ok(())
            } else {
                Err(err)
            }
        }
        Err(err) => Err(err.to_string()),
    }
}

pub fn run_gdb(cli: &Cli) -> Result<(), String> {
    if let Some(_corefile) = &cli.core {
        // let mut args = vec![];
        // args.push("--core".into());
        // ensure_file_exists(corefile);
        // args.push(corefile.to_owned());
        // if let Some(executable) = &cli.executable {
        //     args.push("-e".to_owned());
        //     ensure_file_exists(executable);
        //     args.push(executable.to_owned());
        // };

        // return do_run_gdb(args, cli.unique_mode);
        panic!("not impl");
    }

    if let Some(pids) = &cli.pids {
        let mut has_error = false;
        for pid in pids {
            let args = vec!["--batch", "-p", pid, "-ex", "thread apply all backtrace"];
            println!("Run for process: {:?}", pid);
            match do_run_gdb(args, cli.unique_mode, cli.raw_mode) {
                Ok(_) => {}
                Err(err) => {
                    has_error = true;
                    eprintln!("{err}")
                }
            }
        }
        if has_error {
            return Err("error detected".to_owned());
        } else {
            return Ok(());
        }
    }

    panic!("Needs pid or core file.");
}
