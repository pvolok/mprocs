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
