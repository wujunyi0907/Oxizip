/* C ABI for embedded official zpaq (lib/zpaq). */
#ifndef OXIZIP_ZPAQ_H
#define OXIZIP_ZPAQ_H

#ifdef __cplusplus
extern "C" {
#endif

/* Same semantics as zpaq CLI main: 0=OK, 1=warnings, 2=errors.
 * argv must be UTF-8; argv[0] is a dummy program name. */
int oxizip_zpaq_run(int argc, const char** argv);

/* Like oxizip_zpaq_run, but redirects stdout/stderr to paths (nullable). */
int oxizip_zpaq_run_capture(
    int argc,
    const char** argv,
    const char* stdout_path,
    const char* stderr_path);

#ifdef __cplusplus
}
#endif

#endif /* OXIZIP_ZPAQ_H */