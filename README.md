# Building a Shell from Scratch with Rust

## 🎯 Project Overview

This guide will walk you through building a functional shell (like bash or zsh) from scratch using Rust. You'll learn about process management, I/O redirection, pipes, job control, and how shells work internally.

## 📚 What You'll Learn

- **Process Spawning**: Creating and managing child processes
- **Shell Parsing**: Breaking down command strings into executable components
- **Pipes**: Connecting the output of one command to the input of another
- **Redirects**: Controlling where input comes from and output goes to
- **Job Control**: Managing background/foreground processes
- **Signal Handling**: Responding to Ctrl+C, Ctrl+Z, and other signals

## 🛤️ Learning Path

This guide is organized into progressive phases, each building on the previous one:

### Phase 1: The Basics (01-basic-repl/)
Build a simple Re           ad-Eval-Print Loop (REPL) that can execute basic commands.

### Phase 2: Command Parsing (02-parsing/)
Learn to parse complex command lines with arguments and quotes.

### Phase 3: Built-in Commands (03-builtins/)
Implement essential built-in commands like `cd`, `exit`, and `pwd`.

### Phase 4: I/O Redirection (04-redirection/)
Add support for `>`, `<`, `>>`, and `2>` operators.

### Phase 5: Pipes (05-pipes/)
Implement the `|` operator to chain commands together.

### Phase 6: Job Control (06-job-control/)
Add background execution (`&`) and process management.

### Phase 7: Advanced Features (07-advanced/)
Add environment variables, command history, and signal handling.

## 🚀 Prerequisites

### Required Knowledge
- Basic Rust syntax (variables, functions, structs, enums)
- Basic terminal/shell usage
- Understanding of what commands are

### Tools You'll Need
```bash
# Install Rust (if you haven't already)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify installation
rustc --version
cargo --version
```

## 📖 Core Concepts Overview

### What is a Shell?

A shell is a program that:
1. **Reads** user input from the terminal
2. **Parses** the input into commands and arguments
3. **Executes** the commands
4. **Displays** the results back to the user
5. **Repeats** this cycle (REPL: Read-Eval-Print Loop)

### Process Model

In Unix-like systems (Linux, macOS):
- Every program runs as a **process**
- Processes are identified by a **Process ID (PID)**
- A shell creates new processes using `fork()` and `exec()`
- The shell is the **parent process**, commands are **child processes**

### How Commands Work

When you type `ls -la`:
1. Shell parses: command=`ls`, arguments=`["-la"]`
2. Shell forks a new process
3. Child process executes the `ls` program
4. Parent (shell) waits for child to complete
5. Shell displays the prompt again

### Pipes and Redirection

**Pipes** (`|`): Connect the output of one command to the input of another
```bash
ls -la | grep .txt | wc -l
# Output of ls → input of grep → input of wc
```

**Redirection**: Control where input comes from or output goes to
```bash
ls > files.txt          # Redirect stdout to file
cat < input.txt         # Redirect file to stdin
command 2> errors.txt   # Redirect stderr to file
```

## 🏗️ Project Structure

```
Build-Shell-Clone/
├── GUIDE.md                    # This file
├── 01-basic-repl/             # Phase 1: Basic REPL
│   ├── README.md
│   └── src/main.rs
├── 02-parsing/                # Phase 2: Command parsing
│   ├── README.md
│   └── src/
├── 03-builtins/               # Phase 3: Built-in commands
│   ├── README.md
│   └── src/
├── 04-redirection/            # Phase 4: I/O redirection
│   ├── README.md
│   └── src/
├── 05-pipes/                  # Phase 5: Pipes
│   ├── README.md
│   └── src/
├── 06-job-control/            # Phase 6: Job control
│   ├── README.md
│   └── src/
└── 07-advanced/               # Phase 7: Advanced features
    ├── README.md
    └── src/
```

## 🎓 How to Use This Guide

1. **Start with Phase 1** and work through each phase sequentially
2. **Read the README.md** in each directory for detailed instructions
3. **Type out the code** rather than copy-pasting (better learning)
4. **Experiment** with the code at each phase
5. **Test thoroughly** before moving to the next phase

## 🔑 Key Rust Concepts You'll Use

### Standard Library Modules

- `std::process::Command`: For spawning processes
- `std::io`: For reading input and writing output
- `std::fs`: For file operations
- `std::env`: For environment variables
- `std::os::unix::process`: For Unix-specific process operations

### Important Types

- `Result<T, E>`: For error handling
- `Option<T>`: For optional values
- `Vec<String>`: For storing command arguments
- `HashMap`: For environment variables and job tracking

## 📚 Additional Resources

### Rust Resources
- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Rust Standard Library Docs](https://doc.rust-lang.org/std/)

### Shell Resources
- [POSIX Shell Specification](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html)
- [Write a Shell in C (excellent tutorial, adaptable to Rust)](https://brennan.io/2015/01/16/write-a-shell-in-c/)

## 🎯 Ready to Start?

Head to [01-basic-repl/README.md](01-basic-repl/README.md) to begin your journey!

## 🐛 Debugging Tips

1. **Use `println!` debugging**: Add print statements to see what's happening
2. **Check exit codes**: `echo $?` shows the exit code of the last command
3. **Test with simple commands first**: Get `echo hello` working before trying complex pipes
4. **Read error messages carefully**: Rust's compiler errors are very helpful
5. **Test each phase thoroughly**: Don't move forward until the current phase works

## 🎉 Final Goal

By the end of this guide, you'll have a working shell that can:
- Execute external commands
- Parse complex command lines
- Handle pipes and redirection
- Manage background jobs
- Respond to signals
- Support built-in commands
- Maintain environment variables
