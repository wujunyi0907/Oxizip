// Thin C ABI around the MIT UserChoiceLatestHash codec (see LICENSE in this folder).

#include "oxizip_ucl_api.h"

#include "HashCommon.h"

#include <cstring>

extern "C" int oxizip_ucl_compute_hash(const wchar_t *canonical, wchar_t *out_hash, size_t out_chars)
{
    if (canonical == nullptr || out_hash == nullptr || out_chars == 0)
    {
        return 1;
    }

    UserChoiceLatestHash::WorkingSeeds seeds;
    ZeroMemory(&seeds, sizeof(seeds));
    UserChoiceLatestHash::LoadProvidedSeeds(&seeds);

    std::wstring hash;
    if (!UserChoiceLatestHash::ComputeHash(canonical, seeds, false, &hash, nullptr))
    {
        return 2;
    }
    if (hash.size() + 1U > out_chars)
    {
        return 3;
    }

    wmemcpy(out_hash, hash.c_str(), hash.size() + 1U);
    return 0;
}
