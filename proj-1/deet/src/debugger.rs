use std::collections::HashMap;
use crate::debugger_command::DebuggerCommand;
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use crate::inferior::{Inferior, Status};
use rustyline::error::ReadlineError;
use rustyline::Editor;
use rustyline::history::FileHistory;


#[derive(Clone)]
pub struct Breakpoint {
    pub addr: usize,
    pub orig_byte: u8,
}


// impl Breakpoint {
//     pub fn new(addr: usize, orig_byte: u8) -> Breakpoint {
//         Breakpoint{addr, orig_byte}
//     }
// }

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<(), FileHistory>,
    inferior: Option<Inferior>,
    breakpoints: HashMap<usize, Option<Breakpoint>>,
    debug_data: DwarfData,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        // (milestone 3): initialize the DwarfData
        let debug_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not load debugging symbols from {}: {:?}", target, err);
                std::process::exit(1);
            }
        };

        debug_data.print();

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<(), FileHistory>::new().expect("Create Editor fail");
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        let breakpoints = HashMap::new();

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            debug_data,
            breakpoints,
        }
    }

    fn parse_address(&self, address: &str) -> Option<usize> {
        let mut addr = "";
        if address.to_lowercase().starts_with("0x") {
            // b 0x123456
            addr = &address[2..];
        } else if address.to_lowercase().starts_with("*0x") {
             // b *0x123456
             addr = &address[3..];
        } else if address.parse::<usize>().is_ok() {
            // b line_number
            let line_number = address.parse::<usize>().unwrap();
            return self.debug_data.get_addr_for_line(None, line_number);
        } else {
            // b function_name
            return self.debug_data.get_addr_for_function(None, &address);
        }
        usize::from_str_radix(&addr, 16).ok()
    }

    fn print_status(&mut self) {
        let inferior_mut = self.inferior.as_mut().unwrap();
        match inferior_mut.cont(&self.breakpoints) {
            Ok(status) => {
                match status {
                    Status::Stopped(signal, stop_address) => {
                        println!("Child stopped (signal {})", signal);
                        println!("Stopped at {}",self.debug_data.get_line_from_addr(stop_address).unwrap())
                    },
                    Status::Exited(signal_code) => {
                        println!("Child exited (status {})", signal_code);
                        self.inferior = None;
                    },
                    Status::Signaled(signal) => {
                        println!("Child exited exited due to signal {}", signal);
                        self.inferior = None;
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
                    if let Some(inferior) = Inferior::new(&self.target, &args, &mut self.breakpoints) {
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
                },
                DebuggerCommand::Backtrace => {
                    let inferior = self.inferior.as_ref().unwrap();
                    inferior.print_backtrace(&self.debug_data).unwrap();
                },
                DebuggerCommand::Break(address) => {
                    let addr = self.parse_address(&address).unwrap();
                    println!("Set breakpoint {} at {:x}", self.breakpoints.len(), &addr);

                    if let Some(inferior) = &mut self.inferior {
                        match inferior.write_byte(addr, 0xcc) {
                            Ok(orig_byte) => {
                                self.breakpoints
                                    .insert(addr, Some(Breakpoint { addr, orig_byte }));
                            }
                            Err(err) => {
                                println!("Debugger::new breakpoint write_byte: {}", err)
                            }
                        }
                    } else {
                        self.breakpoints.insert(addr, None);
                    }
                },
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
                    let _ = self.readline.add_history_entry(line.as_str());
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
