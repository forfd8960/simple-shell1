use std::{
    env,
    io::{Write, stdin, stdout},
};

use simple_shell::execute::ShlExecutor;

fn main() -> anyhow::Result<()> {
    println!("Welcome to Rush - A Rust Shell!");
    println!("type exit to quit");

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
        print!("{}", s);

        stdout().flush()?;

        let mut input = String::new();

        let stdin = stdin();
        stdin.read_line(&mut input)?;

        let input = input.trim();
        if input == "exit" {
            break;
        }
        if input.is_empty() {
            continue;
        }

        println!("{input}");

        executor.execute(input)?;
        cmd_count += 1;
    }

    Ok(())
}
