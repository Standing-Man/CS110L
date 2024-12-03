use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::convert::TryInto;
use std::os::unix::process::CommandExt;
use std::process::Child;
use std::process::Command;
use crate::debugger::Breakpoint;
use crate::dwarf_data::DwarfData;

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

fn align_addr_to_word(addr: usize) -> usize {
    addr & (-(size_of::<usize>() as isize) as usize)
}


pub struct Inferior {
    child: Child,
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>, breakpoints: &mut HashMap<usize, Option<Breakpoint>>) -> Option<Inferior> {
        let mut binding = Command::new(target);
        let process = binding.args(args);
        unsafe {
            process.pre_exec(|| {
                child_traceme()
            });
        }
        let child_process = process.spawn().ok()?;
        let mut inferior = Inferior{child: child_process};
        match inferior.wait(None) {
            Ok(status) => {
                match status {
                    Status::Stopped(signal, _) => {
                        if signal == Signal::SIGTRAP {
                            // install these breakpoint into process
                            inferior.install(breakpoints);
                            return Some(inferior);
                        }
                    },
                    _ => {
                        eprintln!("Other status happened!");
                        return None;
                    },
                }
            },
            Err(e) => {
                eprintln!("check failed: {}", e);
                return None;
            },
        }
        None
    }

    // install these breakpoint into process
    fn install(&mut self, breakpoints: &mut HashMap<usize, Option<Breakpoint>>) {
        let interrupt_instruction: u8 = 0xcc;
        for (addr, _) in breakpoints.clone() {
            let orig_byte = self.write_byte(addr.clone(), interrupt_instruction).unwrap();
            
            breakpoints.insert(addr, Some(Breakpoint{addr, orig_byte}));
        }
    }

    pub fn write_byte(&mut self, addr: usize, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        unsafe {
            ptrace::write(
                self.pid(),
                aligned_addr as ptrace::AddressType,
                updated_word as *mut std::ffi::c_void,
            )?;
        }
        Ok(orig_byte as u8)
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    // wake up the inferior and run it until it stops or terminates
    pub fn cont(&mut self, breakpoints: &HashMap<usize, Option<Breakpoint>>) -> Result<Status, nix::Error> {
        let mut regs = ptrace::getregs(self.pid()).unwrap();
        let rip: usize = regs.rip.try_into().unwrap(); 
        if breakpoints.contains_key(&(rip-1)) {
            // restore the first byte of the instruction we replaced
            let orig_byte = breakpoints[&(rip-1)].clone().unwrap().orig_byte;
            self.write_byte(rip-1, orig_byte).unwrap();
            // set %rip = %rip - 1 to rewind the instruction pointer
            regs.rip = (rip - 1) as u64;
            ptrace::setregs(self.pid(), regs)?;
            // ptrace::step to go to next instruction
            ptrace::step(self.pid(), None)?;
            match self.wait(None).unwrap() {
                Status::Stopped(_, _) => {
                    // restore 0xcc in the breakpoint location
                    self.write_byte(rip-1, 0xcc)?;
                },
                Status::Exited(exit_code) => return Ok(Status::Exited(exit_code)),
                Status::Signaled(signal) => return Ok(Status::Signaled(signal)),
            }
    
        }
        // contiune execute child process
        ptrace::cont(self.pid(), None)?;
        // wait the statue of child process
        self.wait(None)


    }

    pub fn print_backtrace(&self, debug_data: &DwarfData) -> Result<(), nix::Error> {
        let regs = ptrace::getregs(self.pid()).unwrap();
        let mut instruction_ptr = regs.rip as usize;
        let mut base_ptr = regs.rbp as usize;
        loop {
            let func_name = debug_data.get_function_from_addr(instruction_ptr).unwrap();
            let line = debug_data.get_line_from_addr(instruction_ptr).unwrap();
            println!("{func_name} ({line})");
            if func_name == "main" {
                break;
            }
            instruction_ptr = ptrace::read(self.pid(), (base_ptr + 8) as ptrace::AddressType)? as usize;
            base_ptr = ptrace::read(self.pid(), base_ptr as ptrace::AddressType)? as usize;
        }
        Ok(())
    }


    pub fn kill(&mut self) -> Result<Status, nix::Error> {
        let _ = Child::kill(&mut self.child);
        
        // Note: wait the statue of child process, make sure the child process quit successful
        self.wait(None)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }
}
