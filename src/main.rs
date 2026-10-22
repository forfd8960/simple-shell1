use std::env;

use rustyline::{DefaultEditor, error::ReadlineError};
use simple_shell::execute::ShlExecutor;

fn main() -> anyhow::Result<()> {
    println!("Welcome to Rush - A Rust Shell!");
    println!("type exit to quit");

    let mut rl = DefaultEditor::new()?;
    let _ = rl.load_history(".rush_history");

    let cur_dir = match env::current_dir()?.into_string() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("failed to get current dir: {:?}", e);
            return Ok(());
        }
    };

    let home = env::var("HOME")
        .ok()
        .unwrap_or_else(|| "/Users/zzz".to_string());

    let mut executor = ShlExecutor::new(cur_dir, home);
    let mut cmd_count = 0;

    loop {
        let s = format!("rush[{cmd_count}]> ");
        let line_res = rl.readline(&s);

        match line_res {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                rl.add_history_entry(line)?;
                if line == "exit" {
                    break;
                }

                executor.execute(line)?;
                cmd_count += 1;
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D
                println!("exit");
                break;
            }
            Err(err) => {
                eprintln!("read line: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}
