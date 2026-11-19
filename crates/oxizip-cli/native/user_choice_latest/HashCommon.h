#ifndef USERCHOICELATESTHASH_COMMON_H
#define USERCHOICELATESTHASH_COMMON_H

#include <windows.h>
#include <wincrypt.h>
#include <sddl.h>

#include <cstring>
#include <cwchar>
#include <cwctype>
#include <string>
#include <vector>

#pragma comment(lib, "advapi32.lib")
#pragma comment(lib, "crypt32.lib")

namespace UserChoiceLatestHash
{
typedef unsigned char U8;
typedef unsigned short U16;
typedef unsigned int U32;
typedef unsigned __int64 U64;

extern const U64 kRepeatSentinel;
extern const U64 kRepeatTokenA;
extern const U64 kRepeatTokenB;
extern const U64 kRepeatTokenC;
extern const U64 kRepeatTokenD;
extern const U64 kRepeatTokenE;
extern const U32 kSeenModulo;
extern const U32 kSeenQwordCount;

struct WorkingSeeds
{
    U32 a[4];
    U32 b[4];
};

enum RunMode
{
    kModeHash,
    kModeVerify,
    kModeDebug,
    kModeDerive
};

struct DebugData
{
    std::vector<U16> packed_words;
    U32 md5_words[4];
    U32 pair_a[2];
    U32 pair_b[2];

    DebugData();
};

struct Tables
{
    std::vector<U8> byte_class_table;
    std::vector<U8> token_region;
    std::vector<U8> fallback_token_code_table;
    std::vector<U8> token_bit_width_table;
    std::vector<U8> short_rewind_table;
    std::vector<U8> long_rewind_table;
};

struct EncoderState
{
    std::vector<U64> entries;
    U64 seen[161];
    U64 count;
    U64 last;
    U32 repeat;
    U32 total_units;

    EncoderState();
};

U32 ReadLe32(const U8 *bytes);
bool ParseHexBytes(const wchar_t *text, std::vector<U8> *out);
bool ParseHexSeed128(const wchar_t *text, U32 out[4]);
void LoadProvidedSeeds(WorkingSeeds *out);
std::wstring ToLowerWide(const std::wstring &value);
const Tables &GetTables();

bool ComputeHash(const std::wstring &canonical_input,
                 const WorkingSeeds &seeds,
                 bool lowercase_output,
                 std::wstring *out_hash,
                 DebugData *debug_data);

void ApplyLicenseNameShaMix(const U32 base_a[4],
                            const U32 base_b[4],
                            const std::wstring &license_name,
                            WorkingSeeds *out);
} // namespace UserChoiceLatestHash

#endif

