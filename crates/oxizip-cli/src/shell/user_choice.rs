//! Per-user default handler via classic `FileExts\...\UserChoice` (Bandizip-aligned).
//!
//! Bandizip.exe (strings / registry behavior) writes only:
//! `Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\%s\UserChoice`
//! with `ProgId` + `Hash` (Firefox / SetUserFTA algorithm). It has **no**
//! `UserChoiceLatest` strings and does not touch that key.
//!
//! Oxizip matches that path: rewrite classic `UserChoice` only; do not create or
//! delete `UserChoiceLatest` (leave Win11 Settings-managed Latest alone).
//!
//! `set_latest_user_choice` remains available for experiments but is unused by install.

use std::mem::MaybeUninit;
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use base64::Engine;
use md5::{Digest, Md5};
use winreg::enums::*;
use winreg::RegKey;

const USER_EXPERIENCE: &str =
    "User Choice set via Windows User Experience {D18B6DD5-6124-4341-9318-804003BAFA0B}";

const SALT_3822: &str =
    "Copyright (C) Microsoft. All rights reserved {3822B7CA-C2F4-4889-B8CC-4CE39A8FB81C}";
const SALT_D185: &str =
    "Copyright (C) Microsoft. All rights reserved {D185E0A1-E265-4724-AA21-3A17B038D72E}";
const SALT_97B6: &str =
    "Copyright (C) Microsoft. All rights reserved {97B6BCF4-C367-4577-95BE-73BD3053A5E0}";

#[link(name = "oxizip_user_choice_latest", kind = "static")]
extern "C" {
    fn oxizip_ucl_compute_hash(canonical: *const u16, out_hash: *mut u16, out_chars: usize) -> i32;
}

fn file_exts_path(ext: &str) -> String {
    let e = ext.trim_start_matches('.');
    format!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.{e}")
}

fn current_user_sid() -> Result<String, String> {
    use std::os::windows::raw::HANDLE;
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: HANDLE, access: u32, token: *mut HANDLE) -> i32;
        fn GetTokenInformation(
            token: HANDLE,
            class: u32,
            info: *mut u8,
            len: u32,
            ret_len: *mut u32,
        ) -> i32;
        fn ConvertSidToStringSidW(sid: *mut u8, string: *mut *mut u16) -> i32;
        fn LocalFree(mem: *mut u8) -> *mut u8;
        fn GetCurrentProcess() -> HANDLE;
        fn CloseHandle(handle: HANDLE) -> i32;
    }
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_USER: u32 = 1;

    unsafe {
        let mut token: HANDLE = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err("OpenProcessToken failed".into());
        }
        let mut needed = 0u32;
        GetTokenInformation(token, TOKEN_USER, ptr::null_mut(), 0, &mut needed);
        let mut buf = vec![0u8; needed as usize];
        if GetTokenInformation(token, TOKEN_USER, buf.as_mut_ptr(), needed, &mut needed) == 0 {
            CloseHandle(token);
            return Err("GetTokenInformation failed".into());
        }
        let sid_ptr = *(buf.as_ptr() as *const *mut u8);
        let mut sid_str: *mut u16 = ptr::null_mut();
        if ConvertSidToStringSidW(sid_ptr, &mut sid_str) == 0 || sid_str.is_null() {
            CloseHandle(token);
            return Err("ConvertSidToStringSidW failed".into());
        }
        let mut len = 0usize;
        while *sid_str.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(sid_str, len);
        let s = String::from_utf16_lossy(slice);
        LocalFree(sid_str as *mut u8);
        CloseHandle(token);
        Ok(s)
    }
}

fn word_swap(v: u32) -> u32 {
    v.rotate_left(16)
}

/// Public Windows UserChoice scramble (Firefox / SetUserFTA algorithm).
fn hash_user_choice_string(input: &str) -> Result<String, String> {
    let mut bytes: Vec<u8> = input.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    bytes.extend_from_slice(&[0, 0]); // NUL terminator

    let md5 = Md5::digest(&bytes);
    let md5_dwords = [
        u32::from_le_bytes(md5[0..4].try_into().unwrap()),
        u32::from_le_bytes(md5[4..8].try_into().unwrap()),
    ];

    let block_count = bytes.len() / 8;
    if block_count == 0 {
        return Err("user choice input too short".into());
    }

    let c0s: [[u32; 5]; 2] = [
        [
            md5_dwords[0] | 1,
            0xCF98_B111,
            0x8708_5B9F,
            0x12CE_B96D,
            0x257E_1D83,
        ],
        [
            md5_dwords[1] | 1,
            0xA274_16F5,
            0xD383_96FF,
            0x7C93_2B89,
            0xBFA4_9F69,
        ],
    ];
    let c1s: [[u32; 5]; 2] = [
        [
            md5_dwords[0] | 1,
            0xEF05_69FB,
            0x689B_6B9F,
            0x79F8_A395,
            0xC3EF_EA97,
        ],
        [
            md5_dwords[1] | 1,
            0xC317_13DB,
            0xDDCD_1F0F,
            0x59C3_AF2D,
            0x35BD_1EC9,
        ],
    ];

    let mut h0 = 0u32;
    let mut h1 = 0u32;
    let mut h0_acc = 0u32;
    let mut h1_acc = 0u32;

    for i in 0..block_count {
        for j in 0..2 {
            let off = (i * 2 + j) * 4;
            let input = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
            let c0 = &c0s[j];
            let c1 = &c1s[j];

            h0 = h0.wrapping_add(input);
            h0 = h0.wrapping_mul(c0[0]);
            h0 = word_swap(h0).wrapping_mul(c0[1]);
            h0 = word_swap(h0).wrapping_mul(c0[2]);
            h0 = word_swap(h0).wrapping_mul(c0[3]);
            h0 = word_swap(h0).wrapping_mul(c0[4]);
            h0_acc = h0_acc.wrapping_add(h0);

            h1 = h1.wrapping_add(input);
            h1 = word_swap(h1)
                .wrapping_mul(c1[1])
                .wrapping_add(h1.wrapping_mul(c1[0]));
            h1 = (h1 >> 16)
                .wrapping_mul(c1[2])
                .wrapping_add(h1.wrapping_mul(c1[3]));
            h1 = word_swap(h1).wrapping_mul(c1[4]).wrapping_add(h1);
            h1_acc = h1_acc.wrapping_add(h1);
        }
    }

    let hash = [(h0 ^ h1).to_le_bytes(), (h0_acc ^ h1_acc).to_le_bytes()].concat();
    Ok(base64::engine::general_purpose::STANDARD.encode(hash))
}

fn format_classic_hash_input(ext_with_dot: &str, sid: &str, progid: &str, filetime: u64) -> String {
    let hi = ((filetime >> 32) as u32) as u64;
    let lo = (filetime & 0xFFFF_FFFF) as u32;
    let raw = format!("{ext_with_dot}{sid}{progid}{hi:08x}{lo:08x}{USER_EXPERIENCE}");
    raw.to_lowercase()
}

fn query_key_filetime(key: &RegKey) -> Result<u64, String> {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegQueryInfoKeyW(
            key: isize,
            class: *mut u16,
            class_len: *mut u32,
            reserved: *mut u32,
            subkeys: *mut u32,
            max_subkey: *mut u32,
            max_class: *mut u32,
            values: *mut u32,
            max_value_name: *mut u32,
            max_value_len: *mut u32,
            security_len: *mut u32,
            last_write: *mut u64,
        ) -> i32;
    }
    let handle = key.raw_handle() as isize;
    let mut ft = MaybeUninit::<u64>::uninit();
    let rc = unsafe {
        RegQueryInfoKeyW(
            handle,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ft.as_mut_ptr(),
        )
    };
    if rc != 0 {
        return Err(format!("RegQueryInfoKeyW failed: {rc}"));
    }
    Ok(unsafe { ft.assume_init() })
}

fn clamp_filetime_to_minute(ft: u64) -> u64 {
    const TICKS_PER_MINUTE: u64 = 600_000_000;
    ft - (ft % TICKS_PER_MINUTE)
}

/// FILETIME → SYSTEMTIME → FILETIME (UTC round-trip used by UserChoiceLatest).
fn normalize_filetime(ft: u64) -> Result<u64, String> {
    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn FileTimeToSystemTime(ft: *const u64, st: *mut SystemTime) -> i32;
        fn SystemTimeToFileTime(st: *const SystemTime, ft: *mut u64) -> i32;
    }
    unsafe {
        let mut st = MaybeUninit::<SystemTime>::uninit();
        if FileTimeToSystemTime(&ft, st.as_mut_ptr()) == 0 {
            return Err("FileTimeToSystemTime failed".into());
        }
        let st = st.assume_init();
        let mut out = 0u64;
        if SystemTimeToFileTime(&st, &mut out) == 0 {
            return Err("SystemTimeToFileTime failed".into());
        }
        Ok(out)
    }
}

fn delete_named_user_choice_tree(parent_rel: &str, sub_name_nul: &str) {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            key: isize,
            sub: *const u16,
            options: u32,
            sam: u32,
            result: *mut isize,
        ) -> i32;
        fn RegDeleteTreeW(key: isize, sub: *const u16) -> i32;
        fn RegCloseKey(key: isize) -> i32;
    }
    const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as isize;
    const KEY_READ: u32 = 0x20019;

    let wide: Vec<u16> = std::ffi::OsStr::new(parent_rel)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let sub: Vec<u16> = sub_name_nul.encode_utf16().collect();
    unsafe {
        let mut h = 0isize;
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide.as_ptr(), 0, KEY_READ, &mut h) == 0 {
            let _ = RegDeleteTreeW(h, sub.as_ptr());
            RegCloseKey(h);
        }
    }
}

/// Delete classic `UserChoice` only (Bandizip-style). Does not touch `UserChoiceLatest`.
pub fn delete_classic_user_choice_key(parent_rel: &str) {
    delete_named_user_choice_tree(parent_rel, "UserChoice\0");
}

/// Delete `UserChoiceLatest` only (legacy Oxizip cleanup when ProgId was ours).
pub fn delete_latest_user_choice_key(parent_rel: &str) {
    delete_named_user_choice_tree(parent_rel, "UserChoiceLatest\0");
}

/// Delete both trees (legacy full wipe). Prefer the classic/latest-specific helpers.
#[allow(dead_code)]
pub fn delete_user_choice_key(parent_rel: &str) {
    delete_classic_user_choice_key(parent_rel);
    delete_latest_user_choice_key(parent_rel);
}

fn read_machine_id() -> Result<String, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey("SOFTWARE\\Microsoft\\SQMClient")
        .map_err(|e| format!("open SQMClient: {e}"))?;
    let raw: String = key
        .get_value("MachineId")
        .map_err(|e| format!("read MachineId: {e}"))?;
    Ok(raw.trim_matches(|c| c == '{' || c == '}').to_string())
}

/// `AppDefaults\\HashVersion` under the current user's SystemProtectedUserData.
#[allow(dead_code)]
fn associations_hash_version(sid: &str) -> u32 {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let path = format!(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\SystemProtectedUserData\\{sid}\\AnyoneRead\\AppDefaults"
    );
    hklm.open_subkey(&path)
        .ok()
        .and_then(|k| k.get_value::<u32, _>("HashVersion").ok())
        .unwrap_or(0)
}

fn primary_salt(mod_class: u32) -> &'static str {
    match mod_class {
        0 => SALT_3822,
        1 => SALT_D185,
        _ => SALT_97B6,
    }
}

fn build_latest_canonical(
    ext_dot: &str,
    sid: &str,
    progid: &str,
    machine_id: &str,
    timestamp_hex: &str,
) -> String {
    let mod_class = machine_id
        .chars()
        .last()
        .map(|c| (c as u32) % 3)
        .unwrap_or(0);
    let salt = primary_salt(mod_class);
    match mod_class {
        0 => format!("{salt}{ext_dot}{timestamp_hex}{machine_id}{progid}{sid}"),
        1 => format!("{salt}{timestamp_hex}{ext_dot}{sid}{machine_id}{progid}"),
        _ => format!("{sid}{timestamp_hex}{salt}{ext_dot}{machine_id}{progid}"),
    }
}

fn compute_latest_hash(canonical: &str) -> Result<String, String> {
    let wide: Vec<u16> = canonical.encode_utf16().chain(std::iter::once(0)).collect();
    let mut out = vec![0u16; 64];
    let rc = unsafe { oxizip_ucl_compute_hash(wide.as_ptr(), out.as_mut_ptr(), out.len()) };
    if rc != 0 {
        return Err(format!("UserChoiceLatest hash failed: {rc}"));
    }
    let len = out.iter().position(|&c| c == 0).unwrap_or(out.len());
    Ok(String::from_utf16_lossy(&out[..len]))
}

fn set_classic_user_choice(
    parent: &str,
    ext_dot: &str,
    sid: &str,
    progid: &str,
) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (user_choice, _) = hkcu
        .create_subkey(format!("{parent}\\UserChoice"))
        .map_err(|e| format!("create UserChoice: {e}"))?;

    let ft = clamp_filetime_to_minute(query_key_filetime(&user_choice)?);
    let input = format_classic_hash_input(ext_dot, sid, progid, ft);
    let hash = hash_user_choice_string(&input)?;

    user_choice
        .set_value("Hash", &hash)
        .map_err(|e| format!("set Hash: {e}"))?;
    user_choice
        .set_value("ProgId", &progid)
        .map_err(|e| format!("set ProgId: {e}"))?;
    Ok(())
}

#[allow(dead_code)]
fn set_latest_user_choice(
    parent: &str,
    ext_dot: &str,
    sid: &str,
    progid: &str,
) -> Result<(), String> {
    let machine_id = read_machine_id()?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (latest, _) = hkcu
        .create_subkey(format!("{parent}\\UserChoiceLatest"))
        .map_err(|e| format!("create UserChoiceLatest: {e}"))?;
    let (prog_key, _) = latest
        .create_subkey("ProgId")
        .map_err(|e| format!("create UserChoiceLatest\\ProgId: {e}"))?;
    prog_key
        .set_value("ProgId", &progid)
        .map_err(|e| format!("set UserChoiceLatest ProgId: {e}"))?;

    let ft = normalize_filetime(query_key_filetime(&prog_key)?)?;
    let hi = (ft >> 32) as u32;
    let lo = ft as u32;
    let timestamp_hex = format!("{hi:08x}{lo:08x}");
    let canonical = build_latest_canonical(ext_dot, sid, progid, &machine_id, &timestamp_hex);
    let hash = compute_latest_hash(&canonical)?;

    latest
        .set_value("Hash", &hash)
        .map_err(|e| format!("set UserChoiceLatest Hash: {e}"))?;
    Ok(())
}

/// Set ProgID via classic `UserChoice` only — same surface as Bandizip.
///
/// Does **not** create/update/delete `UserChoiceLatest` (Win11 Settings owns that).
pub fn set_user_default(ext: &str, progid: &str) -> Result<(), String> {
    let ext_dot = if ext.starts_with('.') {
        ext.to_string()
    } else {
        format!(".{ext}")
    };
    let parent = file_exts_path(&ext_dot);
    let sid = current_user_sid()?;

    delete_classic_user_choice_key(&parent);
    set_classic_user_choice(&parent, &ext_dot, &sid, progid)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_hash_is_stable_for_fixed_input() {
        let input = format_classic_hash_input(
            ".7z",
            "S-1-5-21-1-2-3-1001",
            "cn.wujunyi.oxizip.7z",
            0x01d3442a_29887400,
        );
        let h = hash_user_choice_string(&input).unwrap();
        assert!(!h.is_empty());
        assert!(h.len() >= 8);
    }

    #[test]
    fn latest_canonical_order_mod1() {
        let c = build_latest_canonical(
            ".7z",
            "S-1-5-21-1-2-3-1001",
            "cn.wujunyi.oxizip.7z",
            "5DE713F0-855B-46C8-94C5-F402F600EAB7", // ends with 7 → mod 1
            "01dd77f0e9e2e380",
        );
        assert!(c.starts_with(SALT_D185));
        assert!(c.contains("01dd77f0e9e2e380.7zS-1-5-21"));
    }
}
