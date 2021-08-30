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

#define UNICODE

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
#define RES_FD_IN 2
#define RES_FD_OUT 3
#define RES_PTY 4
#define RES_LEN 5

HRESULT _conpty_create_process(LPTSTR cmdline, LPTSTR cwd, value result) {
  HRESULT hr = S_OK;

  /*
   * Create pseudoconsole.
   */

  // Close these after CreateProcess of child application with pseudoconsole
  // object.
  // TODO: Close these
  HANDLE inputReadSide, outputWriteSide;

  // Hold onto these and use them for communication with the child through the
  // pseudoconsole.
  HANDLE outputReadSide, inputWriteSide;

  if (!CreatePipe(&inputReadSide, &inputWriteSide, NULL, 0)) {
    return HRESULT_FROM_WIN32(GetLastError());
  }

  if (!CreatePipe(&outputReadSide, &outputWriteSide, NULL, 0)) {
    return HRESULT_FROM_WIN32(GetLastError());
  }

  HPCON hpc;
  COORD size = {40, 40};
  hr = CreatePseudoConsole(size, inputReadSide, outputWriteSide, 0, &hpc);
  if (FAILED(hr)) {
    return hr;
  }

  Store_field(result, RES_FD_IN, win_alloc_handle(inputWriteSide));
  Store_field(result, RES_FD_OUT, win_alloc_handle(outputReadSide));
  Store_field(result, RES_PTY, caml_copy_nativeint((intnat)hpc));

  /*
   * Start.
   */

  STARTUPINFOEX si;
  hr = prepare_startup_info(hpc, &si);
  if (FAILED(hr)) {
    return hr;
  }

  PROCESS_INFORMATION pi = {0};

  printf("b4\n");
  if (!CreateProcess(NULL, cmdline, NULL, NULL, FALSE,
                     EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                     NULL, cwd, &si.StartupInfo, &pi)) {
    return HRESULT_FROM_WIN32(GetLastError());
  }
  printf("after\n");

  Store_field(result, RES_PID, Val_int(1337));
  Store_field(result, RES_HANDLE, win_alloc_handle(pi.hProcess));

  return S_OK;
}

CAMLprim value conpty_create_process(value vCmdline, value vCwd) {
  CAMLparam2(vCmdline, vCwd);
  CAMLlocal1(result);

  LPTSTR cmdline = caml_stat_strdup_to_os(String_val(vCmdline));
  LPTSTR cwd = caml_stat_strdup_to_os(String_val(vCwd));

  result = caml_alloc_tuple(RES_LEN);

  HRESULT hr = _conpty_create_process(cmdline, cwd, result);

  // free memory
  caml_stat_free(cmdline);
  caml_stat_free(cwd);

  if (FAILED(hr)) {
    // raise error
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

#else // _WIN32

CAMLprim value conpty_create_process(value vCmdline, value vCwd) {
  caml_failwith("Not implemented: conpty_create_process.");
}

CAMLprim value conpty_process_wait_job(value handle) {
  caml_failwith("Not implemented: conpty_process_wait_job.");
}

#endif // _WIN32
