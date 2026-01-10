pub mod process;
pub mod process_spec;
#[cfg(unix)]
pub mod unix_process;
#[cfg(unix)]
pub mod unix_processes_waiter;

#[cfg(unix)]
pub type NativeProcess = unix_process::UnixProcess;
