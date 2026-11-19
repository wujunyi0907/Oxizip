#pragma once

#include <stddef.h>
#include <wchar.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Compute a UserChoiceLatest Hash (Base64) for the canonical input string.
/// `out_hash` receives a NUL-terminated wide string; `out_chars` is the buffer capacity
/// in wchar_t units (including space for NUL). Returns 0 on success.
int oxizip_ucl_compute_hash(const wchar_t *canonical, wchar_t *out_hash, size_t out_chars);

#ifdef __cplusplus
}
#endif
