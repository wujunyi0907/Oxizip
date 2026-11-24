//! Oxizip Explorer shell extension (clean-room).
//!
//! - Win11 modern menu: `IExplorerCommand` via Sparse Package / SurrogateServer
//! - Classic menu: `IShellExtInit` + `IContextMenu3` via ContextMenuHandlers
//!
//! Never links third-party proprietary shell DLLs (`ark*` etc.). All actions spawn sibling `oxizip-cli.exe`.

#![allow(non_snake_case)]

#[cfg(windows)]
#[allow(dead_code)]
mod context_menu;
#[cfg(windows)]
mod explorer_cmd;
#[cfg(windows)]
mod launch;
#[cfg(windows)]
mod register;

#[cfg(windows)]
pub(crate) mod win {
    use std::sync::atomic::{AtomicU32, Ordering};

    use windows::core::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::System::Com::*;
    use windows::Win32::System::LibraryLoader::DisableThreadLibraryCalls;
    use windows::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

    use crate::explorer_cmd;
    use crate::register;

    /// File / directory modern + classic context menu CLSID.
    pub const CLSID_OXIZIP_CTX: GUID =
        GUID::from_u128(0xC00A_1001_0A12_4E11_9A11_C0DE_0001_0001);
    /// Directory background menu CLSID.
    pub const CLSID_OXIZIP_BKG: GUID =
        GUID::from_u128(0xC00A_1002_0A12_4E11_9A11_C0DE_0001_0002);

    static LOCK_COUNT: AtomicU32 = AtomicU32::new(0);
    static mut MODULE: HMODULE = HMODULE(std::ptr::null_mut());

    pub(crate) fn dll_module() -> HMODULE {
        unsafe { MODULE }
    }

    pub(crate) fn lock_server(increment: bool) {
        if increment {
            LOCK_COUNT.fetch_add(1, Ordering::SeqCst);
        } else {
            LOCK_COUNT.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[no_mangle]
    pub extern "system" fn DllMain(
        module: HMODULE,
        reason: u32,
        _reserved: *mut std::ffi::c_void,
    ) -> BOOL {
        match reason {
            DLL_PROCESS_ATTACH => unsafe {
                MODULE = module;
                let _ = DisableThreadLibraryCalls(module);
            },
            DLL_PROCESS_DETACH => {}
            _ => {}
        }
        TRUE
    }

    #[no_mangle]
    pub extern "system" fn DllCanUnloadNow() -> HRESULT {
        if LOCK_COUNT.load(Ordering::SeqCst) == 0 {
            S_OK
        } else {
            S_FALSE
        }
    }

    /// # Safety
    /// Non-null input pointers must reference valid GUIDs, and `ppv` must be writable.
    #[no_mangle]
    pub unsafe extern "system" fn DllGetClassObject(
        rclsid: *const GUID,
        riid: *const GUID,
        ppv: *mut *mut std::ffi::c_void,
    ) -> HRESULT {
        if rclsid.is_null() || riid.is_null() || ppv.is_null() {
            return E_POINTER;
        }
        *ppv = std::ptr::null_mut();
        let clsid = *rclsid;
        let background = clsid == CLSID_OXIZIP_BKG;
        if clsid != CLSID_OXIZIP_CTX && clsid != CLSID_OXIZIP_BKG {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = ClassFactory { background }.into();
        factory.query(riid, ppv)
    }

    #[no_mangle]
    pub extern "system" fn DllRegisterServer() -> HRESULT {
        match register::register_server() {
            Ok(()) => S_OK,
            Err(e) => e.code(),
        }
    }

    #[no_mangle]
    pub extern "system" fn DllUnregisterServer() -> HRESULT {
        match register::unregister_server() {
            Ok(()) => S_OK,
            Err(e) => e.code(),
        }
    }

    #[implement(IClassFactory)]
    struct ClassFactory {
        background: bool,
    }

    impl IClassFactory_Impl for ClassFactory_Impl {
        fn CreateInstance(
            &self,
            punkouter: Option<&IUnknown>,
            riid: *const GUID,
            ppvobject: *mut *mut std::ffi::c_void,
        ) -> Result<()> {
            if punkouter.is_some() {
                return Err(CLASS_E_NOAGGREGATION.into());
            }
            if riid.is_null() || ppvobject.is_null() {
                return Err(E_POINTER.into());
            }
            unsafe {
                *ppvobject = std::ptr::null_mut();
            }
            // Sparse SurrogateServer asks for IExplorerCommand only.
            // Do not expose classic IContextMenu here (static flat verbs cover classic UX).
            let hr = if self.background {
                let unk: IUnknown = explorer_cmd::BackgroundRoot::new().into();
                unsafe { unk.query(riid, ppvobject) }
            } else {
                let unk: IUnknown = explorer_cmd::ExplorerCommand::parent(
                    crate::launch::SelectionKind::FilesOrDirs,
                    Vec::new(),
                )
                .into();
                unsafe { unk.query(riid, ppvobject) }
            };
            hr.ok()
        }

        fn LockServer(&self, flock: BOOL) -> Result<()> {
            lock_server(flock.as_bool());
            Ok(())
        }
    }
}

#[cfg(windows)]
pub use win::{
    DllCanUnloadNow, DllGetClassObject, DllMain, DllRegisterServer, DllUnregisterServer,
    CLSID_OXIZIP_BKG, CLSID_OXIZIP_CTX,
};

#[cfg(not(windows))]
/// Placeholder so the crate can be listed in the workspace on non-Windows hosts.
#[allow(dead_code)]
pub fn oxizip_shellext_non_windows_stub() {}
