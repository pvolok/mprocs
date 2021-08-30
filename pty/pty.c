#include <caml/alloc.h>
#include <caml/bigarray.h>
#include <caml/callback.h>
#include <caml/fail.h>
#include <caml/memory.h>
#include <caml/mlvalues.h>
#include <caml/threads.h>

#if !defined(__CYGWIN__) && !defined (__MINGW32__)

#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <sys/types.h>
#include <sys/stat.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <fcntl.h>
#include <signal.h>

/* forkpty */
/* http://www.gnu.org/software/gnulib/manual/html_node/forkpty.html */
#if defined(__GLIBC__) || defined(__CYGWIN__)
#include <pty.h>
#elif defined(__APPLE__) || defined(__OpenBSD__) || defined(__NetBSD__)
#include <util.h>
#elif defined(__FreeBSD__)
#include <libutil.h>
#elif defined(__sun)
#include <stropts.h> /* for I_PUSH */
#else
#include <pty.h>
#endif

#include <termios.h> /* tcgetattr, tty_ioctl */

/* Some platforms name VWERASE and VDISCARD differently */
#if !defined(VWERASE) && defined(VWERSE)
#define VWERASE	VWERSE
#endif
#if !defined(VDISCARD) && defined(VDISCRD)
#define VDISCARD	VDISCRD
#endif

/* NSIG - macro for highest signal + 1, should be defined */
#ifndef NSIG
#define NSIG 32
#endif

CAMLprim value ocaml_pty_ioctl_set_size(value vFd, value vWidth,
                                        value vHeight) {
  CAMLparam3(vFd, vWidth, vHeight);

  int fd = Int_val(vFd);
  int rows = Int_val(vHeight);
  int cols = Int_val(vWidth);

#ifdef WIN32
  // TODO
  int result = 0;
#else
  struct winsize sz;
  sz.ws_col = cols;
  sz.ws_row = rows;
  sz.ws_xpixel = 0;
  sz.ws_ypixel = 0;

  int result = ioctl(fd, TIOCSWINSZ, &sz);
#endif

  CAMLreturn(Val_int(result));
}

CAMLprim value ocaml_pty_fork(value vWidth, value vHeight) {
  CAMLparam2(vWidth, vHeight);
  CAMLlocal2(ret_tuple, ret);

  int rows = Int_val(vHeight);
  int cols = Int_val(vWidth);

  // size
  struct winsize winp;
  winp.ws_col = rows;
  winp.ws_row = cols;
  winp.ws_xpixel = 0;
  winp.ws_ypixel = 0;

  // termios
  struct termios t;
  struct termios *term = &t;
  term->c_iflag = ICRNL | IXON | IXANY | IMAXBEL | BRKINT;
  if (1) {
#if defined(IUTF8)
    term->c_iflag |= IUTF8;
#endif
  }
  term->c_oflag = OPOST | ONLCR;
  term->c_cflag = CREAD | CS8 | HUPCL;
  term->c_lflag =
      ICANON | ISIG | IEXTEN | ECHO | ECHOE | ECHOK | ECHOKE | ECHOCTL;

  term->c_cc[VEOF] = 4;
  term->c_cc[VEOL] = -1;
  term->c_cc[VEOL2] = -1;
  term->c_cc[VERASE] = 0x7f;
  term->c_cc[VWERASE] = 23;
  term->c_cc[VKILL] = 21;
  term->c_cc[VREPRINT] = 18;
  term->c_cc[VINTR] = 3;
  term->c_cc[VQUIT] = 0x1c;
  term->c_cc[VSUSP] = 26;
  term->c_cc[VSTART] = 17;
  term->c_cc[VSTOP] = 19;
  term->c_cc[VLNEXT] = 22;
  term->c_cc[VDISCARD] = 15;
  term->c_cc[VMIN] = 1;
  term->c_cc[VTIME] = 0;

#if (__APPLE__)
  term->c_cc[VDSUSP] = 25;
  term->c_cc[VSTATUS] = 20;
#endif

  cfsetispeed(term, B38400);
  cfsetospeed(term, B38400);

  // fork the pty
  int master = -1;

  sigset_t newmask, oldmask;
  struct sigaction sig_action;

  // temporarily block all signals
  // this is needed due to a race condition in openpty
  // and to avoid running signal handlers in the child
  // before exec* happened
  sigfillset(&newmask);
  pthread_sigmask(SIG_SETMASK, &newmask, &oldmask);

  pid_t pid = forkpty(&master, NULL, term, &winp);

  if (!pid) {
    // remove all signal handler from child
    sig_action.sa_handler = SIG_DFL;
    sig_action.sa_flags = 0;
    sigemptyset(&sig_action.sa_mask);
    for (int i = 0; i < NSIG; i++) { // NSIG is a macro for all signals + 1
      sigaction(i, &sig_action, NULL);
    }
  }
  // reenable signals
  pthread_sigmask(SIG_SETMASK, &oldmask, NULL);

  switch (pid) {
  case -1:
    caml_failwith("forkpty(3) failed.");
    break;
  case 0:
    CAMLreturn(Val_none);
  default:
    ret_tuple = caml_alloc_tuple(2);
    Store_field(ret_tuple, 0, Val_int(master));
    Store_field(ret_tuple, 1, Val_int(pid));
    ret = caml_alloc_some(ret_tuple);
    CAMLreturn(ret);
  }

  CAMLreturn(Val_none);
}

#else // !defined(__CYGWIN__) && !defined (__MINGW32__)

CAMLprim value ocaml_pty_fork(value vWidth, value vHeight) {
  // TODO: throw error
}

CAMLprim value ocaml_pty_ioctl_set_size(value vFd, value vWidth,
                                        value vHeight) {
  // TODO: throw error
}

#endif // !defined(__CYGWIN__) && !defined (__MINGW32__)
