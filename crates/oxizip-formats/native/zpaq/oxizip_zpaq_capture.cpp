/* Capture stdout/stderr while running embedded zpaq (for list parsing). */
#include "oxizip_zpaq.h"

#include <stdio.h>
#include <string.h>

#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#define OX_DUP(fd) _dup(fd)
#define OX_DUP2(a, b) _dup2(a, b)
#define OX_CLOSE(fd) _close(fd)
#define OX_FILENO(s) _fileno(s)
#else
#include <unistd.h>
#define OX_DUP(fd) dup(fd)
#define OX_DUP2(a, b) dup2(a, b)
#define OX_CLOSE(fd) close(fd)
#define OX_FILENO(s) fileno(s)
#endif

extern "C" int oxizip_zpaq_run_capture(
    int argc,
    const char** argv,
    const char* stdout_path,
    const char* stderr_path) {
  int saved_out = -1;
  int saved_err = -1;
  int code = 2;

  if (stdout_path && stdout_path[0]) {
    fflush(stdout);
    saved_out = OX_DUP(OX_FILENO(stdout));
    if (saved_out < 0) {
      return 2;
    }
    if (!freopen(stdout_path, "w", stdout)) {
      OX_CLOSE(saved_out);
      return 2;
    }
  }
  if (stderr_path && stderr_path[0]) {
    fflush(stderr);
    saved_err = OX_DUP(OX_FILENO(stderr));
    if (saved_err < 0) {
      if (saved_out >= 0) {
        fflush(stdout);
        OX_DUP2(saved_out, OX_FILENO(stdout));
        OX_CLOSE(saved_out);
      }
      return 2;
    }
    if (!freopen(stderr_path, "w", stderr)) {
      OX_CLOSE(saved_err);
      if (saved_out >= 0) {
        fflush(stdout);
        OX_DUP2(saved_out, OX_FILENO(stdout));
        OX_CLOSE(saved_out);
      }
      return 2;
    }
  }

  code = oxizip_zpaq_run(argc, argv);
  fflush(stdout);
  fflush(stderr);

  if (saved_out >= 0) {
    OX_DUP2(saved_out, OX_FILENO(stdout));
    OX_CLOSE(saved_out);
  }
  if (saved_err >= 0) {
    OX_DUP2(saved_err, OX_FILENO(stderr));
    OX_CLOSE(saved_err);
  }
  return code;
}
