//! Optional Windows Action Center toasts for in-app notifications.
//!
//! Unpackaged Win32 toasts need a process AppUserModelID and a Start Menu
//! shortcut that carries the same ID. Failures stay in the in-app center.

/// AppUserModelID shared by the process and the Start Menu shortcut.
pub(crate) const AUMID: &str = "IonPmp.Orchid";

/// Bind this process to [`AUMID`] so later toasts can find the app.
pub(crate) fn prepare() {
    #[cfg(windows)]
    if let Err(e) = set_process_aumid() {
        tracing::warn!(?e, "could not set process AppUserModelID for OS toasts");
    }
}

/// Create or refresh the Start Menu shortcut when OS toasts are enabled.
pub(crate) fn sync(enabled: bool) {
    if !enabled {
        return;
    }
    #[cfg(windows)]
    if let Err(e) = ensure_start_menu_shortcut() {
        tracing::warn!(?e, "could not register Start Menu shortcut for OS toasts");
    }
    #[cfg(not(windows))]
    tracing::debug!("os_notifications shortcut sync is a no-op on this platform");
}

/// Show a toast when the setting is on. Errors are logged only.
pub(crate) fn show_if_enabled(enabled: bool, title: &str, body: &str) {
    if !enabled {
        return;
    }
    #[cfg(windows)]
    if let Err(e) = show_windows_toast(title, body) {
        tracing::debug!(?e, "Windows toast failed; in-app center still has the item");
    }
    #[cfg(not(windows))]
    {
        let _ = (title, body);
        tracing::debug!("os_notifications toast is a no-op on this platform");
    }
}

#[must_use]
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

#[must_use]
fn toast_xml(title: &str, body: &str) -> String {
    format!(
        concat!(
            "<toast><visual><binding template=\"ToastGeneric\">",
            "<text>{title}</text><text>{body}</text>",
            "</binding></visual></toast>"
        ),
        title = xml_escape(title),
        body = xml_escape(body)
    )
}

#[cfg(windows)]
fn set_process_aumid() -> windows::core::Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let id = wide(AUMID);
    // Must run before the first toast; safe to call more than once.
    unsafe { SetCurrentProcessExplicitAppUserModelID(PCWSTR(id.as_ptr())) }
}

#[cfg(windows)]
fn ensure_com() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

    // S_OK / S_FALSE / RPC_E_CHANGED_MODE are all fine: we only need COM up.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
}

#[cfg(windows)]
fn wide(path: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn start_menu_shortcut_path() -> Option<std::path::PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    Some(
        dirs.data_dir()
            .join(r"Microsoft\Windows\Start Menu\Programs\Orchid.lnk"),
    )
}

#[cfg(windows)]
fn ensure_start_menu_shortcut() -> windows::core::Result<()> {
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    ensure_com();

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!(?e, "os_notifications: could not resolve current executable");
            return Ok(());
        }
    };
    let Some(lnk) = start_menu_shortcut_path() else {
        tracing::warn!("os_notifications: no roaming AppData for Start Menu shortcut");
        return Ok(());
    };
    if let Some(parent) = lnk.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(?e, path = %parent.display(), "os_notifications: Start Menu dir");
            return Ok(());
        }
    }

    const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 5,
    };

    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let exe_w = wide(&exe);
        link.SetPath(PCWSTR(exe_w.as_ptr()))?;
        if let Some(dir) = exe.parent() {
            let dir_w = wide(dir);
            link.SetWorkingDirectory(PCWSTR(dir_w.as_ptr()))?;
        }
        let store: IPropertyStore = link.cast()?;
        let value = PROPVARIANT::from(AUMID);
        store.SetValue(&PKEY_APP_USER_MODEL_ID, &value)?;
        store.Commit()?;
        let file: IPersistFile = link.cast()?;
        let lnk_w = wide(&lnk);
        file.Save(PCWSTR(lnk_w.as_ptr()), true)?;
    }
    Ok(())
}

#[cfg(windows)]
fn show_windows_toast(title: &str, body: &str) -> windows::core::Result<()> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(toast_xml(title, body)))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?;
    notifier.Show(&toast)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_encodes_markup() {
        assert_eq!(xml_escape(r#"<a & "b">"#), "&lt;a &amp; &quot;b&quot;&gt;");
    }

    #[test]
    fn toast_xml_embeds_escaped_text() {
        let xml = toast_xml("Hi & bye", "1 < 2");
        assert!(xml.contains("<text>Hi &amp; bye</text>"));
        assert!(xml.contains("<text>1 &lt; 2</text>"));
        assert!(!xml.contains("Hi & bye"));
    }

    #[test]
    fn aumid_is_company_product() {
        assert!(AUMID.contains('.'));
        assert!(!AUMID.is_empty());
    }

    #[test]
    fn disabled_show_is_a_noop() {
        show_if_enabled(false, "title", "body");
    }

    #[test]
    fn prepare_does_not_panic() {
        prepare();
    }
}
