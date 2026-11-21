//! Win11 `IExplorerCommand` + `IEnumExplorerCommand` for Sparse Package menus.

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::IBindCtx;
use windows::Win32::UI::Shell::*;

use crate::launch::{
    child_actions, child_actions_sparse_all, paths_from_array, run_action, selection_kind,
    title_zh_short, SelectionKind,
};

fn alloc_title(s: &str) -> Result<PWSTR> {
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

#[implement(IExplorerCommand)]
pub struct BackgroundRoot;

impl BackgroundRoot {
    pub fn new() -> Self {
        Self
    }
}

impl IExplorerCommand_Impl for BackgroundRoot_Impl {
    fn GetTitle(&self, _psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        alloc_title(title_zh_short("Parent"))
    }

    fn GetIcon(&self, _psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetToolTip(&self, _psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(crate::win::CLSID_OXIZIP_BKG)
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
        Ok(EnumCommands::sparse_all(SelectionKind::Background, Vec::new()).into())
    }
}

#[implement(IExplorerCommand)]
pub struct ExplorerCommand {
    action: String,
    kind: SelectionKind,
    paths: Vec<String>,
    is_parent: bool,
}

impl ExplorerCommand {
    pub fn parent(kind: SelectionKind, paths: Vec<String>) -> Self {
        Self {
            action: "Parent".into(),
            kind,
            paths,
            is_parent: true,
        }
    }

    pub fn child(action: &str, kind: SelectionKind, paths: Vec<String>) -> Self {
        Self {
            action: action.into(),
            kind,
            paths,
            is_parent: false,
        }
    }
}

impl IExplorerCommand_Impl for ExplorerCommand_Impl {
    fn GetTitle(&self, _psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        alloc_title(title_zh_short(&self.action))
    }

    fn GetIcon(&self, _psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetToolTip(&self, _psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        // Root verb must return the Sparse SurrogateServer CLSID; children may be zero.
        if self.is_parent {
            Ok(crate::win::CLSID_OXIZIP_CTX)
        } else {
            Ok(GUID::zeroed())
        }
    }

    fn GetState(
        &self,
        psiitemarray: Option<&IShellItemArray>,
        _foktobeslow: BOOL,
    ) -> Result<u32> {
        let paths = if self.paths.is_empty() {
            paths_from_array(psiitemarray).unwrap_or_default()
        } else {
            self.paths.clone()
        };
        let kind = if paths.is_empty() {
            self.kind
        } else {
            selection_kind(&paths)
        };
        if self.is_parent {
            return Ok(ECS_ENABLED.0 as u32);
        }
        let allowed = child_actions(kind);
        if allowed.contains(&self.action.as_str()) {
            Ok(ECS_ENABLED.0 as u32)
        } else {
            Ok(ECS_HIDDEN.0 as u32)
        }
    }

    fn Invoke(
        &self,
        psiitemarray: Option<&IShellItemArray>,
        _pbc: Option<&IBindCtx>,
    ) -> Result<()> {
        if self.is_parent {
            return Ok(());
        }
        let mut paths = paths_from_array(psiitemarray).unwrap_or_default();
        if paths.is_empty() {
            paths = self.paths.clone();
        }
        if paths.is_empty() {
            return Ok(());
        }
        run_action(&self.action, &paths)
    }

    fn GetFlags(&self) -> Result<u32> {
        if self.is_parent {
            Ok(ECF_HASSUBCOMMANDS.0 as u32)
        } else {
            Ok(ECF_DEFAULT.0 as u32)
        }
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        if !self.is_parent {
            return Err(E_NOTIMPL.into());
        }
        Ok(EnumCommands::sparse_all(self.kind, self.paths.clone()).into())
    }
}

#[implement(IEnumExplorerCommand)]
struct EnumCommands {
    actions: Vec<&'static str>,
    kind: SelectionKind,
    paths: Vec<String>,
    index: std::cell::Cell<usize>,
}

impl EnumCommands {
    fn sparse_all(kind: SelectionKind, paths: Vec<String>) -> Self {
        let actions = if kind == SelectionKind::Background {
            child_actions(SelectionKind::Background).to_vec()
        } else {
            child_actions_sparse_all().to_vec()
        };
        Self {
            actions,
            kind,
            paths,
            index: std::cell::Cell::new(0),
        }
    }
}

impl IEnumExplorerCommand_Impl for EnumCommands_Impl {
    fn Next(
        &self,
        celt: u32,
        pulelt: *mut Option<IExplorerCommand>,
        pceltfetched: *mut u32,
    ) -> HRESULT {
        if pulelt.is_null() {
            return E_POINTER;
        }
        let mut fetched = 0u32;
        let start = self.index.get();
        while fetched < celt && start + (fetched as usize) < self.actions.len() {
            let action = self.actions[start + fetched as usize];
            let cmd = ExplorerCommand::child(action, self.kind, self.paths.clone());
            let iface: IExplorerCommand = cmd.into();
            unsafe {
                *pulelt.add(fetched as usize) = Some(iface);
            }
            fetched += 1;
        }
        self.index.set(start + fetched as usize);
        if !pceltfetched.is_null() {
            unsafe {
                *pceltfetched = fetched;
            }
        }
        if fetched == celt {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, celt: u32) -> Result<()> {
        let next = self.index.get().saturating_add(celt as usize);
        self.index.set(next.min(self.actions.len()));
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        self.index.set(0);
        Ok(())
    }

    fn Clone(&self) -> Result<IEnumExplorerCommand> {
        let cloned = EnumCommands {
            actions: self.actions.clone(),
            kind: self.kind,
            paths: self.paths.clone(),
            index: std::cell::Cell::new(self.index.get()),
        };
        Ok(cloned.into())
    }
}
