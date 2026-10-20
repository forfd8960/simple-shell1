use std::io::{Write, stdin, stdout};

use simple_shell::execute::run_command;

fn main() -> anyhow::Result<()> {
    loop {
        print!("> ");

        stdout().flush()?;

        let mut input = String::new();

        let stdin = stdin();
        stdin.read_line(&mut input)?;

        let input = input.trim();
        if input == "exit" {
            break;
        }
        println!("{input}");

        run_command(input)?;
    }

    Ok(())
}
