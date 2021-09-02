#define CAML_NAME_SPACE
#define CAML_INTERNALS
#include <caml/alloc.h>
#include <caml/bigarray.h>
#include <caml/callback.h>
#include <caml/custom.h>
#include <caml/fail.h>
#include <caml/memory.h>
#include <caml/misc.h>
#include <caml/mlvalues.h>
#include <caml/osdeps.h>
#include <caml/threads.h>

#ifdef _WIN32

#ifndef UNICODE
#define UNICODE
#endif

// Resize doesn't work without this.
#define _WIN32_WINNT 0x600

#include <winsock2.h>

#include <windows.h>

#include <WinCon.h>

#include <lwt_unix.h>

// Taken from the RS5 Windows SDK, but redefined here in case we're targeting <=
// 17134
#ifndef PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE
#define PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE                                    \
  ProcThreadAttributeValue(22, FALSE, TRUE, FALSE)

typedef VOID *HPCON;
typedef HRESULT(__stdcall *PFNCREATEPSEUDOCONSOLE)(COORD c, HANDLE hIn,
                                                   HANDLE hOut, DWORD dwFlags,
                                                   HPCON *phpcon);
typedef HRESULT(__stdcall *PFNRESIZEPSEUDOCONSOLE)(HPCON hpc, COORD newSize);
typedef void(__stdcall *PFNCLOSEPSEUDOCONSOLE)(HPCON hpc);

#endif

static value val_of_hpcon(HPCON hpc) {
  return caml_copy_nativeint((intnat)hpc);
}

static HPCON hpcon_of_val(value v) { return (HPCON)Nativeint_val(v); }

/*
 * Create process.
 */

HRESULT prepare_startup_info(HPCON hpc, STARTUPINFOEX *psi) {
  STARTUPINFOEX si = {0};
  si.StartupInfo.cb = sizeof(STARTUPINFOEX);

  // Discover the size required for the list
  size_t bytesRequired;
  InitializeProcThreadAttributeList(NULL, 1, 0, &bytesRequired);

  // Allocate memory to represent the list
  si.lpAttributeList = (PPROC_THREAD_ATTRIBUTE_LIST)malloc(bytesRequired);
  if (!si.lpAttributeList) {
    return E_OUTOFMEMORY;
  }

  // Initialize the list memory location
  if (!InitializeProcThreadAttributeList(si.lpAttributeList, 1, 0,
                                         &bytesRequired)) {
    free(si.lpAttributeList);
    return HRESULT_FROM_WIN32(GetLastError());
  }

  // Set the pseudoconsole information into the list
  if (!UpdateProcThreadAttribute(si.lpAttributeList, 0,
                                 PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, hpc,
                                 sizeof(hpc), NULL, NULL)) {
    free(si.lpAttributeList);
    return HRESULT_FROM_WIN32(GetLastError());
  }

  *psi = si;

  return S_OK;
}

#define RES_PID 0
#define RES_HANDLE 1
#define RES_STDIN 2
#define RES_STDOUT 3
#define RES_HPC 4
#define RES_LEN 5

HRESULT _conpty_create_process(LPCTSTR prog, LPTSTR cmdline, LPVOID env,
                               LPTSTR cwd, COORD size, value result) {
  HRESULT hr = S_OK;

  /*
   * Create pseudoconsole.
   */

  // Close these after CreateProcess of child application with pseudoconsole
  // object.
  HANDLE stdinCons, stdoutCons;

  // Hold onto these and use them for communication with the child through the
  // pseudoconsole.
  HANDLE stdoutOur, stdinOur;

  if (!CreatePipe(&stdinCons, &stdinOur, NULL, 0)) {
    return HRESULT_FROM_WIN32(GetLastError());
  }

  if (!CreatePipe(&stdoutOur, &stdoutCons, NULL, 0)) {
    return HRESULT_FROM_WIN32(GetLastError());
  }

  HPCON hpc;
  hr = CreatePseudoConsole(size, stdinCons, stdoutCons, 0, &hpc);
  if (FAILED(hr)) {
    return hr;
  }

  Store_field(result, RES_STDIN, win_alloc_handle(stdinOur));
  Store_field(result, RES_STDOUT, win_alloc_handle(stdoutOur));
  Store_field(result, RES_HPC, caml_copy_nativeint((intnat)hpc));

  /*
   * Start.
   */

  DWORD dwCreationFlags = EXTENDED_STARTUPINFO_PRESENT;
  if (env == NULL) {
    dwCreationFlags |= CREATE_UNICODE_ENVIRONMENT;
  }

  STARTUPINFOEX si;
  hr = prepare_startup_info(hpc, &si);
  if (FAILED(hr)) {
    return hr;
  }

  PROCESS_INFORMATION pi = {0};

  if (!CreateProcessW(prog, cmdline, NULL, NULL, FALSE, dwCreationFlags, env,
                      cwd, &si.StartupInfo, &pi)) {
    return HRESULT_FROM_WIN32(GetLastError());
  }

  CloseHandle(pi.hThread);

  CloseHandle(stdinCons);
  CloseHandle(stdoutCons);

  Store_field(result, RES_PID, Val_int(pi.dwProcessId));
  Store_field(result, RES_HANDLE, win_alloc_handle(pi.hProcess));

  return S_OK;
}

#define os_str_opt_val(opt)                                                    \
  (Is_block(opt) ? caml_stat_strdup_to_os(String_val(Field(opt, 0))) : NULL)

CAMLprim value conpty_create_process(value vProg, value vCmdline, value vEnv,
                                     value vCwd, value vSize) {
  CAMLparam5(vProg, vCmdline, vEnv, vCwd, vSize);
  CAMLlocal1(result);

  result = caml_alloc_tuple(RES_LEN);

  LPTSTR prog = os_str_opt_val(vProg);
  LPTSTR cmdline = caml_stat_strdup_to_os(String_val(vCmdline));
  LPTSTR cwd = os_str_opt_val(vCwd);

  LPVOID env = NULL;
  if (Is_some(vEnv)) {
    env = String_val(Some_val(vEnv));
  }

  COORD size;
  size.Y = Int_val(Field(vSize, 0)); // rows
  size.X = Int_val(Field(vSize, 1)); // cols

  HRESULT hr = _conpty_create_process(prog /* prog */, cmdline, env /* env */,
                                      cwd, size, result);

  if (prog)
    caml_stat_free(prog);
  caml_stat_free(cmdline);
  if (cwd)
    caml_stat_free(cwd);

  if (FAILED(hr)) {
    win32_maperr(GetLastError());
    uerror("conpty_create_process", Nothing);
  }

  CAMLreturn(result);
}

/*
 * Wait job.
 */

struct job_wait {
  struct lwt_unix_job job;
  HANDLE handle;
};

static void worker_wait(struct job_wait *job) {
  WaitForSingleObject(job->handle, INFINITE);
}

static value result_wait(struct job_wait *job) {
  DWORD code, error;
  if (!GetExitCodeProcess(job->handle, &code)) {
    error = GetLastError();
    CloseHandle(job->handle);
    lwt_unix_free_job(&job->job);
    win32_maperr(error);
    uerror("GetExitCodeProcess", Nothing);
  }
  CloseHandle(job->handle);
  lwt_unix_free_job(&job->job);
  return Val_int(code);
}

CAMLprim value conpty_process_wait_job(value handle) {
  LWT_UNIX_INIT_JOB(job, wait, 0);
  job->handle = Handle_val(handle);
  return lwt_unix_alloc_job(&(job->job));
}

/*
 * Kill
 */

CAMLprim value conpty_kill(value vConpty) {
  CAMLparam1(vConpty);

  HPCON hpc = hpcon_of_val(Field(vConpty, RES_HPC));
  ClosePseudoConsole(hpc);

  CAMLreturn(0);
}

/*
 * Resize
 */

CAMLprim value conpty_resize(value vConpty, value vRows, value vCols) {
  CAMLparam2(vRows, vCols);

  HPCON hpc = hpcon_of_val(Field(vConpty, RES_HPC));
  COORD size;
  size.X = Int_val(vCols);
  size.Y = Int_val(vRows);
  ResizePseudoConsole(hpc, size);

  CAMLreturn(0);
}

#else // _WIN32

CAMLprim value conpty_create_process(value vCmdline, value vCwd) {
  caml_failwith("Not implemented: conpty_create_process.");
}

CAMLprim value conpty_process_wait_job(value handle) {
  caml_failwith("Not implemented: conpty_process_wait_job.");
}

#endif // _WIN32
