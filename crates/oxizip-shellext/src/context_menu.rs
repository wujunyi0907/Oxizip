//! Classic `IShellExtInit` + `IContextMenu3` (Win7–Win11 “Show more options”).

use std::cell::RefCell;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{
    IBindCtx, IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL,
};
use windows::Win32::System::Ole::{ReleaseStgMedium, CF_HDROP};
use windows::Win32::System::Registry::HKEY;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::explorer_cmd::ExplorerCommand;
use crate::launch::{
    child_actions, child_actions_sparse_all, run_action, selection_kind, title_zh_short,
    SelectionKind,
};

const MENU_OFFSET: u32 = 0;

#[implement(
    IShellExtInit,
    IContextMenu,
    IContextMenu2,
    IContextMenu3,
    IExplorerCommand
)]
pub struct OxizipShellExt {
    paths: RefCell<Vec<String>>,
    kind: RefCell<SelectionKind>,
    actions: RefCell<Vec<&'static str>>,
}

impl OxizipShellExt {
    pub fn new() -> Self {
        Self {
            paths: RefCell::new(Vec::new()),
            kind: RefCell::new(SelectionKind::FilesOrDirs),
            actions: RefCell::new(Vec::new()),
        }
    }

    fn rebuild_actions(&self) {
        let paths = self.paths.borrow();
        let kind = selection_kind(&paths);
        *self.kind.borrow_mut() = kind;
        *self.actions.borrow_mut() = child_actions(kind).to_vec();
    }
}

impl IShellExtInit_Impl for OxizipShellExt_Impl {
    fn Initialize(
        &self,
        pidlfolder: *const ITEMIDLIST,
        pdtobj: Option<&IDataObject>,
        _hkeyprogid: HKEY,
    ) -> Result<()> {
        let mut paths = Vec::new();
        if let Some(data) = pdtobj {
            let format = FORMATETC {
                cfFormat: CF_HDROP.0,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            };
            unsafe {
                if let Ok(mut medium) = data.GetData(&format) {
                    let hdrop = HDROP(medium.u.hGlobal.0);
                    let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
                    for i in 0..count {
                        let mut buf = [0u16; 520];
                        let n = DragQueryFileW(hdrop, i, Some(&mut buf));
                        if n > 0 {
                            paths.push(String::from_utf16_lossy(&buf[..n as usize]));
                        }
                    }
                    ReleaseStgMedium(&mut medium);
                }
            }
        }
        if paths.is_empty() && !pidlfolder.is_null() {
            let mut buf = [0u16; 260];
            let ok = unsafe { SHGetPathFromIDListW(pidlfolder, &mut buf) };
            if ok.as_bool() {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                if len > 0 {
                    paths.push(String::from_utf16_lossy(&buf[..len]));
                }
            }
        }
        if paths.is_empty() {
            *self.kind.borrow_mut() = SelectionKind::Background;
            *self.actions.borrow_mut() = child_actions(SelectionKind::Background).to_vec();
            *self.paths.borrow_mut() = paths;
        } else {
            *self.paths.borrow_mut() = paths;
            self.rebuild_actions();
        }
        Ok(())
    }
}

impl IContextMenu_Impl for OxizipShellExt_Impl {
    fn QueryContextMenu(
        &self,
        hmenu: HMENU,
        indexmenu: u32,
        idcmdfirst: u32,
        _idcmdlast: u32,
        uflags: u32,
    ) -> Result<()> {
        if uflags & CMF_DEFAULTONLY != 0 {
            return Ok(());
        }
        let actions = self.actions.borrow();
        if actions.is_empty() {
            return Ok(());
        }
        // Classic path should not cascade; insert flat items directly (kept for M4).
        for (i, action) in actions.iter().enumerate() {
            let title = format!("Oxizip {}", title_zh_short(action));
            let mut title_wide: Vec<u16> =
                title.encode_utf16().chain(std::iter::once(0)).collect();
            let item = MENUITEMINFOW {
                cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_ID | MIIM_STRING | MIIM_FTYPE,
                fType: MFT_STRING,
                wID: idcmdfirst + MENU_OFFSET + i as u32,
                dwTypeData: PWSTR(title_wide.as_mut_ptr()),
                ..Default::default()
            };
            unsafe {
                InsertMenuItemW(hmenu, indexmenu + i as u32, true, &item)?;
            }
        }
        let codes = MENU_OFFSET + actions.len() as u32;
        Err(Error::from(HRESULT(codes as i32)))
    }

    fn InvokeCommand(&self, pici: *const CMINVOKECOMMANDINFO) -> Result<()> {
        if pici.is_null() {
            return Err(E_POINTER.into());
        }
        let info = unsafe { &*pici };
        let verb = info.lpVerb.0 as usize;
        if verb > 0xFFFF {
            return Ok(());
        }
        let idx = verb as u32;
        if idx < MENU_OFFSET {
            return Ok(());
        }
        let i = (idx - MENU_OFFSET) as usize;
        let actions = self.actions.borrow();
        let Some(action) = actions.get(i).copied() else {
            return Ok(());
        };
        let paths = self.paths.borrow().clone();
        let paths = if paths.is_empty() {
            std::env::current_dir()
                .map(|p| vec![p.display().to_string()])
                .unwrap_or_default()
        } else {
            paths
        };
        run_action(action, &paths)
    }

    fn GetCommandString(
        &self,
        _idcmd: usize,
        _utype: u32,
        _preserved: *const u32,
        _pszname: PSTR,
        _cchmax: u32,
    ) -> Result<()> {
        Ok(())
    }
}

impl IContextMenu2_Impl for OxizipShellExt_Impl {
    fn HandleMenuMsg(&self, _umsg: u32, _wparam: WPARAM, _lparam: LPARAM) -> Result<()> {
        Ok(())
    }
}

impl IContextMenu3_Impl for OxizipShellExt_Impl {
    fn HandleMenuMsg2(
        &self,
        _umsg: u32,
        _wparam: WPARAM,
        _lparam: LPARAM,
        plresult: *mut LRESULT,
    ) -> Result<()> {
        if !plresult.is_null() {
            unsafe {
                *plresult = LRESULT(0);
            }
        }
        Ok(())
    }
}

impl IExplorerCommand_Impl for OxizipShellExt_Impl {
    fn GetTitle(&self, _psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        let s = title_zh_short("Parent");
        let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        let ptr =
            unsafe { windows::Win32::System::Com::CoTaskMemAlloc(wide.len() * 2) as *mut u16 };
        if ptr.is_null() {
            return Err(E_OUTOFMEMORY.into());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        }
        Ok(PWSTR(ptr))
    }

    fn GetIcon(&self, _psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetToolTip(&self, _psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(crate::win::CLSID_OXIZIP_CTX)
    }

    fn GetState(
        &self,
        _psiitemarray: Option<&IShellItemArray>,
        _foktobeslow: BOOL,
    ) -> Result<u32> {
        Ok(ECS_ENABLED.0 as u32)
    }

    fn Invoke(
        &self,
        _psiitemarray: Option<&IShellItemArray>,
        _pbc: Option<&IBindCtx>,
    ) -> Result<()> {
        Ok(())
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_HASSUBCOMMANDS.0 as u32)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        // Sparse parent has no selection at enum time — list all; children GetState filter.
        let _ = child_actions_sparse_all;
        let paths = self.paths.borrow().clone();
        let kind = if paths.is_empty() {
            SelectionKind::FilesOrDirs
        } else {
            *self.kind.borrow()
        };
        let parent = ExplorerCommand::parent(kind, paths);
        let cmd: IExplorerCommand = parent.into();
        unsafe { cmd.EnumSubCommands() }
    }
}
