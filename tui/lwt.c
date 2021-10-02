#include <caml/alloc.h>
#include <caml/mlvalues.h>
#include <lwt_unix.h>

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

void *tui_events_read_rs();

struct job_event {
  struct lwt_unix_job job;
  void *event;
};

static void worker_event(struct job_event *job) {
  job->event = tui_events_read_rs();
}

static value result_event(struct job_event *job) {
  void *event = job->event;

  value v = caml_alloc(1, Abstract_tag);
  *((void **)Data_abstract_val(v)) = event;

  lwt_unix_free_job(&job->job);

  return v;
}

CAMLprim value tui_event_job() {
  LWT_UNIX_INIT_JOB(job, event, 0);
  job->event = NULL;
  return lwt_unix_alloc_job(&(job->job));
}
