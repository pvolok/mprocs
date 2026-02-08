use std::{
  env,
  io::{self},
  iter::once,
  mem::{size_of, zeroed},
  os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, OwnedHandle},
  ptr::null,
};

use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::windows::named_pipe::NamedPipeServer,
};
use windows::{
  core::{PCWSTR, PWSTR},
  Win32::{
    Foundation::{CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
      CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE,
      FILE_FLAG_OVERLAPPED, FILE_SHARE_NONE, OPEN_EXISTING,
      PIPE_ACCESS_INBOUND, PIPE_ACCESS_OUTBOUND,
    },
    System::{
      Console::{
        ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD,
        HPCON,
      },
      Pipes::{
        CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE,
      },
      Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
        InitializeProcThreadAttributeList, RegisterWaitForSingleObject,
        TerminateProcess, UnregisterWait, UpdateProcThreadAttribute,
        CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
        LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
        PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, STARTUPINFOEXW,
        WT_EXECUTEONLYONCE,
      },
    },
  },
};

use crate::{
  error::ResultLogger, kernel::proc::ProcId, process::process::Process,
  term_types::winsize::Winsize,
};

use super::process_spec::ProcessSpec;

const SIGKILL: i32 = 9;

pub struct WinProcess {
  pub pid: i32,
  reader: NamedPipeServer,
  writer: NamedPipeServer,
  conpty: HPCON,
  process_handle: OwnedHandle,
  wait_handle: HANDLE,
}
unsafe impl Send for WinProcess {}

type OnWaitReturned = Box<dyn Fn(Option<i32>) + Send + Sync>;

impl WinProcess {
  pub fn spawn(
    id: ProcId,
    spec: &ProcessSpec,
    size: Winsize,
    on_wait_returned: OnWaitReturned,
  ) -> io::Result<Self> {
    unsafe {
      let (host_write, conpty_input) = {
        let input_pipe_name = format!("\\\\.\\pipe\\conpty-input-{}", id.0);
        let input_pipe_name_wide: Vec<u16> =
          input_pipe_name.encode_utf16().chain(once(0)).collect();
        let input_pipe_name_ptr = PCWSTR(input_pipe_name_wide.as_ptr());

        let host_write = CreateNamedPipeW(
          input_pipe_name_ptr,
          PIPE_ACCESS_OUTBOUND
            | FILE_FLAG_OVERLAPPED
            | FILE_FLAG_FIRST_PIPE_INSTANCE,
          PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
          1,
          0,
          0,
          0,
          None,
        );
        if host_write == INVALID_HANDLE_VALUE {
          return Err(io::Error::last_os_error());
        }
        let host_write = OwnedHandle::from_raw_handle(host_write.0);

        let conpty_input = CreateFileW(
          input_pipe_name_ptr,
          windows::Win32::Foundation::GENERIC_READ.0,
          windows::Win32::Storage::FileSystem::FILE_SHARE_NONE,
          None,
          windows::Win32::Storage::FileSystem::OPEN_EXISTING,
          windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL
            | FILE_FLAG_OVERLAPPED,
          None,
        )?;
        let conpty_input = OwnedHandle::from_raw_handle(conpty_input.0);

        (host_write, conpty_input)
      };

      let (host_read, conpty_output) = {
        let output_pipe_name = format!("\\\\.\\pipe\\conpty-output-{}", id.0);
        let output_pipe_name_wide: Vec<u16> =
          output_pipe_name.encode_utf16().chain(once(0)).collect();
        let output_pipe_name_ptr = PCWSTR(output_pipe_name_wide.as_ptr());

        let host_read = CreateNamedPipeW(
          output_pipe_name_ptr,
          PIPE_ACCESS_INBOUND
            | FILE_FLAG_OVERLAPPED
            | FILE_FLAG_FIRST_PIPE_INSTANCE,
          PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
          1,
          0,
          0,
          0,
          None,
        );
        if host_read == INVALID_HANDLE_VALUE {
          return Err(io::Error::last_os_error());
        }
        let host_read = OwnedHandle::from_raw_handle(host_read.0);

        let conpty_output = CreateFileW(
          output_pipe_name_ptr,
          GENERIC_WRITE.0,
          FILE_SHARE_NONE,
          None,
          OPEN_EXISTING,
          FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
          None,
        )?;
        let conpty_output = OwnedHandle::from_raw_handle(conpty_output.0);

        (host_read, conpty_output)
      };

      // Create pseudo console
      let coord = COORD {
        X: size.x as i16,
        Y: size.y as i16,
      };
      let conpty = CreatePseudoConsole(
        coord,
        HANDLE(conpty_input.as_raw_handle()),
        HANDLE(conpty_output.as_raw_handle()),
        0,
      )?;
      drop(conpty_input);
      drop(conpty_output);

      let mut startup_info_ex: STARTUPINFOEXW = zeroed();
      startup_info_ex.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;

      let mut attr_list_size: usize = 0;
      // Note: This initial call will return an error by design. This is
      // expected behavior.
      // https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-initializeprocthreadattributelist#remarks
      let _: Result<(), windows::core::Error> =
        InitializeProcThreadAttributeList(None, 1, None, &mut attr_list_size);

      let mut attr_list: Vec<u8> = vec![0; attr_list_size];
      startup_info_ex.lpAttributeList =
        LPPROC_THREAD_ATTRIBUTE_LIST(attr_list.as_mut_ptr() as _);

      InitializeProcThreadAttributeList(
        Some(startup_info_ex.lpAttributeList),
        1,
        None,
        &mut attr_list_size,
      )?;

      UpdateProcThreadAttribute(
        startup_info_ex.lpAttributeList,
        0,
        PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
        Some(conpty.0 as _),
        size_of::<HPCON>(),
        None,
        None,
      )?;

      // Build environment block
      let mut env_map: std::collections::HashMap<String, String> =
        env::vars().collect();
      for (key, value) in &spec.env {
        if let Some(val) = value {
          env_map.insert(key.clone(), val.clone());
        } else {
          env_map.remove(key);
        }
      }
      let mut env_block: Vec<u16> = Vec::new();
      let mut env_pairs: Vec<(&String, &String)> = env_map.iter().collect();
      env_pairs.sort_by_key(|pair| pair.0);
      for (key, value) in env_pairs {
        env_block.extend(key.encode_utf16());
        env_block.push('=' as u16);
        env_block.extend(value.encode_utf16());
        env_block.push(0);
      }
      env_block.push(0);
      let env_block_ptr = if env_block.is_empty() {
        None
      } else {
        Some(env_block.as_mut_ptr() as _)
      };

      // Build command line
      fn quote_arg(arg: &str) -> String {
        if !arg.chars().any(|c| c == ' ' || c == '\t' || c == '"') {
          arg.to_string()
        } else {
          let mut s = String::new();
          s.push('"');
          for c in arg.chars() {
            if c == '"' {
              s.push('\\');
            }
            s.push(c);
          }
          s.push('"');
          s
        }
      }
      let mut cmdline = quote_arg(&spec.prog);
      for arg in &spec.args {
        cmdline.push(' ');
        cmdline.push_str(&quote_arg(arg));
      }
      let cmdline_wide: Vec<u16> =
        cmdline.encode_utf16().chain(once(0)).collect();
      let cmdline_ptr = cmdline_wide.as_ptr() as *mut u16;

      // CWD
      let cwd = spec.get_cwd().as_ref();
      let cwd_wide =
        cwd.map(|s| s.encode_utf16().chain(once(0)).collect::<Vec<u16>>());
      let cwd_ptr = cwd_wide.as_ref().map_or(null(), |v| v.as_ptr());

      let mut process_info: PROCESS_INFORMATION = zeroed();
      CreateProcessW(
        None,
        Some(PWSTR::from_raw(cmdline_ptr)),
        None,
        None,
        false,
        EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
        env_block_ptr,
        PCWSTR::from_raw(cwd_ptr),
        &startup_info_ex.StartupInfo,
        &mut process_info,
      )?;
      DeleteProcThreadAttributeList(startup_info_ex.lpAttributeList);

      let process_handle =
        OwnedHandle::from_raw_handle(process_info.hProcess.0);
      let pid = process_info.dwProcessId as i32;
      CloseHandle(process_info.hThread)?;

      struct WaitContext {
        callback: OnWaitReturned,
        process_handle: HANDLE,
      }
      unsafe extern "system" fn wait_callback(
        context: *mut std::ffi::c_void,
        _: bool,
      ) {
        let context = unsafe { Box::from_raw(context.cast::<WaitContext>()) };
        let mut exit_code = 0;
        let exit_code = if unsafe {
          GetExitCodeProcess(context.process_handle, &mut exit_code)
        }
        .is_ok()
        {
          Some(exit_code as i32)
        } else {
          None
        };
        (context.callback)(exit_code);
      }

      let mut wait_handle = HANDLE::default();
      RegisterWaitForSingleObject(
        &mut wait_handle,
        HANDLE(process_handle.as_raw_handle()),
        Some(wait_callback),
        Some(Box::into_raw(Box::new(WaitContext {
          callback: on_wait_returned,
          process_handle: HANDLE(process_handle.as_raw_handle()),
        })) as _),
        u32::MAX,
        WT_EXECUTEONLYONCE,
      )?;

      let reader =
        NamedPipeServer::from_raw_handle(host_read.into_raw_handle())?;
      let writer =
        NamedPipeServer::from_raw_handle(host_write.into_raw_handle())?;

      Ok(WinProcess {
        pid,
        reader,
        writer,
        conpty,
        process_handle,
        wait_handle,
      })
    }
  }
}

impl Process for WinProcess {
  fn on_exited(&mut self) {
    unsafe {
      ClosePseudoConsole(self.conpty);
      self.conpty = HPCON::default();

      UnregisterWait(self.wait_handle).log_ignore();
      self.wait_handle = HANDLE::default();
    };
  }

  async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    let count = self.reader.read(buf).await?;
    log::debug!("read ({})", count);
    Ok(count)
  }

  async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.writer.write(buf).await
  }

  async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
    self.writer.write_all(buf).await
  }

  fn send_signal(&mut self, sig: i32) -> io::Result<()> {
    if sig == SIGKILL {
      unsafe {
        TerminateProcess(HANDLE(self.process_handle.as_raw_handle()), 1)?
      };
    } else {
      // Only SIGKILL is supported on Windows
    }
    Ok(())
  }

  async fn kill(&mut self) -> io::Result<()> {
    self.send_signal(SIGKILL)
  }

  fn resize(&mut self, size: Winsize) -> io::Result<()> {
    unsafe {
      ResizePseudoConsole(
        self.conpty,
        COORD {
          X: size.x as i16,
          Y: size.y as i16,
        },
      )?
    };
    Ok(())
  }
}

impl Drop for WinProcess {
  fn drop(&mut self) {
    unsafe {
      if !self.conpty.is_invalid() {
        log::warn!("`self.conpty` is still open in `WinProcess::drop()`.");
        ClosePseudoConsole(self.conpty);
      }
      if !self.wait_handle.is_invalid() {
        log::warn!("`self.wait_handle` is still open in `WinProcess::drop()`.");
        UnregisterWait(self.wait_handle).log_ignore();
      }
    }
  }
}
