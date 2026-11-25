//! Native "choose a folder" dialog on its own thread, so the UI keeps painting.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// What the chosen folder is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickFor {
    NewSession,
}

pub struct FolderPick {
    pub purpose: PickFor,
    rx: Receiver<Option<PathBuf>>,
}

impl FolderPick {
    pub fn open(ctx: &egui::Context, purpose: PickFor, title: &str, start: Option<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel();
        let (title, ctx) = (title.to_owned(), ctx.clone());
        let spawned = std::thread::Builder::new().name("ankar-folder-pick".into()).spawn(move || {
            let _ = tx.send(browse(&title, start.as_deref()));
            ctx.request_repaint();
        });
        if let Err(e) = spawned {
            log::warn!("folder picker thread: {e}");
        }
        Self { purpose, rx }
    }

    /// `Some(choice)` once the dialog has closed; the choice is `None` when cancelled.
    pub fn poll(&self) -> Option<Option<PathBuf>> {
        match self.rx.try_recv() {
            Ok(choice) => Some(choice),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(None),
        }
    }
}

#[cfg(windows)]
fn browse(title: &str, start: Option<&Path>) -> Option<PathBuf> {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};
    use windows_sys::Win32::UI::Shell::{
        SHBrowseForFolderW, SHGetPathFromIDListW, BFFM_INITIALIZED, BFFM_SETSELECTIONW, BIF_NEWDIALOGSTYLE,
        BIF_RETURNONLYFSDIRS, BROWSEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;

    unsafe extern "system" fn on_event(hwnd: HWND, msg: u32, _lparam: LPARAM, data: LPARAM) -> i32 {
        if msg == BFFM_INITIALIZED && data != 0 {
            // SAFETY: `data` is the NUL-terminated wide start path owned by `browse`, which is
            // blocked in SHBrowseForFolderW while this runs.
            unsafe { SendMessageW(hwnd, BFFM_SETSELECTIONW, 1, data) };
        }
        0
    }

    let wide = |s: &OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let title_w = wide(OsStr::new(title));
    let start_w = start.map(|p| wide(p.as_os_str()));
    let mut name = [0u16; 260];
    let info = BROWSEINFOW {
        hwndOwner: std::ptr::null_mut(),
        pidlRoot: std::ptr::null_mut(),
        pszDisplayName: name.as_mut_ptr(),
        lpszTitle: title_w.as_ptr(),
        ulFlags: BIF_NEWDIALOGSTYLE | BIF_RETURNONLYFSDIRS,
        lpfn: Some(on_event),
        lParam: start_w.as_ref().map_or(0, |w| w.as_ptr() as LPARAM),
        iImage: 0,
    };
    // SAFETY: every buffer outlives the calls that use it; the dialog thread initialises COM
    // (apartment-threaded, as BIF_NEWDIALOGSTYLE requires) and frees the returned PIDL.
    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let pidl = SHBrowseForFolderW(&info);
        if pidl.is_null() {
            return None;
        }
        let mut path = [0u16; 260];
        let ok = SHGetPathFromIDListW(pidl, path.as_mut_ptr());
        CoTaskMemFree(pidl as *const _);
        if ok == 0 {
            return None;
        }
        let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
        Some(PathBuf::from(OsString::from_wide(&path[..len])))
    }
}

#[cfg(not(windows))]
fn browse(_title: &str, _start: Option<&Path>) -> Option<PathBuf> {
    None
}

/// The line typed into the shell for the footer's "cd": quoted, so spaces survive.
pub fn cd_command(path: &Path) -> String {
    format!("cd \"{}\"\r", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cd_quotes_the_path_and_presses_enter() {
        assert_eq!(cd_command(Path::new(r"C:\Program Files\x")), "cd \"C:\\Program Files\\x\"\r");
    }
}
