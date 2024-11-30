use std::process::Child;

use crate::debugger_command::DebuggerCommand;
use crate::inferior::{self, Inferior, Status};
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitPidFlag, WaitStatus};
use rustyline::error::ReadlineError;
use rustyline::Editor;

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        // TODO (milestone 3): initialize the DwarfData

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
        }
    }

    fn print_status(&mut self) {
        let inferior_mut = self.inferior.as_mut().unwrap();
        match inferior_mut.cont() {
            Ok(status) => {
                match status {
                    Status::Stopped(signal, _) => {
                        println!("Child stopped (signal {})", signal);

                    },
                    Status::Exited(signal_code) => {
                        println!("Child exited (status {})", signal_code);
                    },
                    Status::Signaled(signal) => {
                        println!("Child exited exited due to signal {}", signal);
                    },
                }

            },
            Err(e) => {
                eprintln!("{}", e);
            },
        }
    }

    fn kill(&mut self) {
        let inferior_mut = self.inferior.as_mut().unwrap();
        let pid = inferior_mut.pid();
        match inferior_mut.kill() {
            Ok(_) => {
                println!("Killing running inferior (pid {})", &pid);
            },
            Err(_) => {
                eprintln!("Error: failed to kill running inferior (pid {})", &pid);
            },
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    // check if any existing inferiors before run new one
                    if self.inferior.is_some() {
                        self.kill();
                    }
                    if let Some(inferior) = Inferior::new(&self.target, &args) {
                        // Create the inferior
                        self.inferior = Some(inferior);
                        self.print_status();
                       
                    } else {
                        println!("Error starting subprocess");
                    }
                },
                DebuggerCommand::Continue => {
                    // check if there have inferior to debug
                    if self.inferior.is_none() {
                        println!("Nothing is being debugged!");
                        continue;
                    }
                    self.print_status();
                }
                DebuggerCommand::Quit => {
                    if self.inferior.is_some() {
                        self.kill();
                    }
                    return;
                }
            }
        }
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().len() == 0 {
                        continue;
                    }
                    self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}
