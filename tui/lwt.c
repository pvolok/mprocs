#include <caml/mlvalues.h>
#include <lwt_unix.h>

struct job_rs {
  struct lwt_unix_job job;
  void *data;
};

value tui_lwt_create_job(lwt_unix_job_worker worker_rs,
                         lwt_unix_job_result result_rs, void *data) {
  LWT_UNIX_INIT_JOB(job, rs, 0);
  job->data = data;
  return lwt_unix_alloc_job(&(job->job));
}

void *tui_lwt_get_data(struct job_rs *job) { return job->data; }

#ifdef _WIN32
static int check_align(size_t align) {
  for (size_t i = sizeof(void *); i != 0; i *= 2)
    if (align == i)
      return 0;
  return EINVAL;
}

int posix_memalign(void **ptr, size_t align, size_t size) {
  if (check_align(align))
    return EINVAL;

  int saved_errno = errno;
  void *p = _aligned_malloc(size, align);
  if (p == NULL) {
    errno = saved_errno;
    return ENOMEM;
  }

  *ptr = p;
  return 0;
}

//

#pragma comment(lib, "userenv.lib")
#pragma comment(lib, "ws2_32.lib")

#endif
