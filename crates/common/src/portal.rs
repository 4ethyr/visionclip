use std::{
    env, fs,
    path::{Path, PathBuf},
};

const SCREENSHOT_INTERFACE: &str = "org.freedesktop.impl.portal.Screenshot";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalBackendDescriptor {
    pub file_name: String,
    pub dbus_name: String,
    pub interfaces: Vec<String>,
    pub use_in: Vec<String>,
}

impl PortalBackendDescriptor {
    pub fn supports_screenshot(&self) -> bool {
        self.interfaces
            .iter()
            .any(|interface| interface == SCREENSHOT_INTERFACE)
    }

    pub fn matches_any_desktop(&self, desktops: &[String]) -> bool {
        self.use_in.is_empty()
            || desktops.is_empty()
            || self.use_in.iter().any(|desktop| {
                desktops
                    .iter()
                    .any(|current| current.eq_ignore_ascii_case(desktop))
            })
    }

    pub fn summary(&self) -> String {
        if self.use_in.is_empty() {
            self.file_name.clone()
        } else {
            format!("{} (UseIn={})", self.file_name, self.use_in.join(":"))
        }
    }
}

pub fn current_desktops() -> Vec<String> {
    let current = env::var("XDG_CURRENT_DESKTOP")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("XDG_SESSION_DESKTOP")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            env::var("DESKTOP_SESSION")
                .ok()
                .filter(|value| !value.trim().is_empty())
        });

    current
        .unwrap_or_default()
        .split(':')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

pub fn screenshot_portal_backends_for_current_desktop() -> Vec<PortalBackendDescriptor> {
    let desktops = current_desktops();
    let mut backends = discover_portal_backends()
        .into_iter()
        .filter(|backend| backend.supports_screenshot() && backend.matches_any_desktop(&desktops))
        .collect::<Vec<_>>();
    backends.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    backends
}

pub fn summarize_portal_backends(backends: &[PortalBackendDescriptor]) -> String {
    if backends.is_empty() {
        "none detected".into()
    } else {
        backends
            .iter()
            .map(PortalBackendDescriptor::summary)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn discover_portal_backends() -> Vec<PortalBackendDescriptor> {
    portal_search_paths()
        .into_iter()
        .filter_map(|path| read_portal_descriptors(&path).ok())
        .flatten()
        .collect()
}

fn portal_search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/usr/share/xdg-desktop-portal/portals")];

    if let Ok(data_home) = env::var("XDG_DATA_HOME") {
        if !data_home.trim().is_empty() {
            paths.push(PathBuf::from(data_home).join("xdg-desktop-portal/portals"));
        }
    } else if let Ok(home) = env::var("HOME") {
        if !home.trim().is_empty() {
            paths.push(PathBuf::from(home).join(".local/share/xdg-desktop-portal/portals"));
        }
    }

    paths
}

fn read_portal_descriptors(path: &Path) -> std::io::Result<Vec<PortalBackendDescriptor>> {
    let mut descriptors = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();

        if entry_path.extension().and_then(|value| value.to_str()) != Some("portal") {
            continue;
        }

        let contents = fs::read_to_string(&entry_path)?;
        if let Some(descriptor) = parse_portal_descriptor(
            &contents,
            entry_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
        ) {
            descriptors.push(descriptor);
        }
    }

    Ok(descriptors)
}

fn parse_portal_descriptor(contents: &str, file_name: &str) -> Option<PortalBackendDescriptor> {
    let mut in_portal_section = false;
    let mut dbus_name = None;
    let mut interfaces = Vec::new();
    let mut use_in = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_portal_section = line.eq_ignore_ascii_case("[portal]");
            continue;
        }

        if !in_portal_section {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();

        match key.trim() {
            "DBusName" => dbus_name = Some(value.to_string()),
            "Interfaces" => {
                interfaces = value
                    .split(';')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect();
            }
            "UseIn" => {
                use_in = value
                    .split(';')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_ascii_lowercase())
                    .collect();
            }
            _ => {}
        }
    }

    Some(PortalBackendDescriptor {
        file_name: file_name.to_string(),
        dbus_name: dbus_name?,
        interfaces,
        use_in,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_portal_descriptor_reads_interfaces_and_use_in() {
        let descriptor = parse_portal_descriptor(
            r#"
            [portal]
            DBusName=org.freedesktop.impl.portal.desktop.gnome
            Interfaces=org.freedesktop.impl.portal.Settings;org.freedesktop.impl.portal.Screenshot;
            UseIn=gnome;ubuntu
            "#,
            "gnome.portal",
        )
        .unwrap();

        assert_eq!(descriptor.file_name, "gnome.portal");
        assert!(descriptor.supports_screenshot());
        assert_eq!(descriptor.use_in, vec!["gnome", "ubuntu"]);
    }

    #[test]
    fn summarize_portal_backends_formats_use_in() {
        let summary = summarize_portal_backends(&[PortalBackendDescriptor {
            file_name: "gnome.portal".into(),
            dbus_name: "org.freedesktop.impl.portal.desktop.gnome".into(),
            interfaces: vec![SCREENSHOT_INTERFACE.into()],
            use_in: vec!["gnome".into()],
        }]);

        assert_eq!(summary, "gnome.portal (UseIn=gnome)");
    }

    #[test]
    fn backend_matches_any_desktop_when_use_in_is_empty() {
        let descriptor = PortalBackendDescriptor {
            file_name: "test.portal".into(),
            dbus_name: "org.freedesktop.impl.portal.desktop.test".into(),
            interfaces: vec![SCREENSHOT_INTERFACE.into()],
            use_in: Vec::new(),
        };

        assert!(descriptor.matches_any_desktop(&["gnome".into()]));
    }
}
