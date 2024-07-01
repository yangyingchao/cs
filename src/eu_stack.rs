use crate::{
    args::Cli,
    uniquify::uniquify_eustack,
    utils::{ensure_file_exists, execute_command},
};

fn do_run_eustack(args: Vec<String>, unique: bool) -> Result<(), String> {
    match execute_command("eu-stack", args) {
        Ok(result) => {
            let (code, out, err) = result;
            if code <= 1 {
                if unique {
                    uniquify_eustack(&out).expect("Uniquify fail..");
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

pub fn run_eustack(cli: &Cli) -> Result<(), String> {
    if let Some(corefile) = &cli.core {
        let mut args = vec![];
        args.push("--core".into());
        ensure_file_exists(corefile);
        args.push(corefile.to_owned());
        if let Some(executable) = &cli.executable {
            args.push("-e".to_owned());
            ensure_file_exists(executable);
            args.push(executable.to_owned());
        };

        return do_run_eustack(args, cli.unique_mode);
    }

    if let Some(pids) = &cli.pids {
        let mut has_error = false;
        for pid in pids {
            let args = vec!["-p".to_string(), pid.to_string()];
            println!("Run for process: {:?}", pid);
            match do_run_eustack(args, cli.unique_mode) {
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
