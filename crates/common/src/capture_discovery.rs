use crate::{
    current_desktops, screenshot_portal_backends_for_current_desktop, summarize_portal_backends,
    SessionType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureBackendKind {
    Portal,
    GnomeShellScreenshot,
    GnomeScreenshot,
    Grim,
    Maim,
    Spectacle,
    Flameshot,
    Scrot,
    ImageMagickImport,
}

impl CaptureBackendKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Portal => "xdg-desktop-portal Screenshot",
            Self::GnomeShellScreenshot => "GNOME Shell Screenshot D-Bus",
            Self::GnomeScreenshot => "gnome-screenshot",
            Self::Grim => "grim",
            Self::Maim => "maim",
            Self::Spectacle => "spectacle",
            Self::Flameshot => "flameshot",
            Self::Scrot => "scrot",
            Self::ImageMagickImport => "ImageMagick import",
        }
    }

    pub fn command(self) -> Option<&'static str> {
        match self {
            Self::Portal | Self::GnomeShellScreenshot => Some("gdbus"),
            Self::GnomeScreenshot => Some("gnome-screenshot"),
            Self::Grim => Some("grim"),
            Self::Maim => Some("maim"),
            Self::Spectacle => Some("spectacle"),
            Self::Flameshot => Some("flameshot"),
            Self::Scrot => Some("scrot"),
            Self::ImageMagickImport => Some("import"),
        }
    }

    pub fn supports_passive_fullscreen(self) -> bool {
        matches!(
            self,
            Self::GnomeShellScreenshot | Self::GnomeScreenshot | Self::Grim | Self::Maim
        )
    }

    pub fn supports_interactive_area(self) -> bool {
        matches!(
            self,
            Self::Portal
                | Self::GnomeShellScreenshot
                | Self::GnomeScreenshot
                | Self::Grim
                | Self::Maim
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureBackendDescriptor {
    pub kind: CaptureBackendKind,
    pub detail: String,
}

impl CaptureBackendDescriptor {
    pub fn summary(&self) -> String {
        if self.detail.is_empty() {
            self.kind.label().to_string()
        } else {
            format!("{} ({})", self.kind.label(), self.detail)
        }
    }
}

pub fn likely_gnome_shell_screenshot_available(command_exists: impl Fn(&str) -> bool) -> bool {
    command_exists("gdbus")
        && current_desktops()
            .iter()
            .any(|desktop| desktop.eq_ignore_ascii_case("gnome"))
}

pub fn discover_capture_backends(
    session_type: SessionType,
    command_exists: impl Fn(&str) -> bool,
    gnome_shell_screenshot_available: bool,
) -> Vec<CaptureBackendDescriptor> {
    let portal_summary = portal_screenshot_summary(&command_exists);
    let mut backends = Vec::new();

    if let Some(detail) = portal_summary {
        backends.push(CaptureBackendDescriptor {
            kind: CaptureBackendKind::Portal,
            detail,
        });
    }

    for kind in preferred_backend_order(session_type) {
        if backend_available(kind, &command_exists, gnome_shell_screenshot_available) {
            push_unique(
                &mut backends,
                CaptureBackendDescriptor {
                    kind,
                    detail: String::new(),
                },
            );
        }
    }

    backends
}

pub fn discover_rendered_capture_backends(
    session_type: SessionType,
    command_exists: impl Fn(&str) -> bool,
    gnome_shell_screenshot_available: bool,
) -> Vec<CaptureBackendDescriptor> {
    discover_capture_backends(
        session_type,
        command_exists,
        gnome_shell_screenshot_available,
    )
    .into_iter()
    .filter(|backend| backend.kind.supports_passive_fullscreen())
    .collect()
}

pub fn summarize_capture_backends(backends: &[CaptureBackendDescriptor]) -> String {
    if backends.is_empty() {
        "none detected".into()
    } else {
        backends
            .iter()
            .map(CaptureBackendDescriptor::summary)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn preferred_backend_order(session_type: SessionType) -> Vec<CaptureBackendKind> {
    match session_type {
        SessionType::Wayland => vec![
            CaptureBackendKind::GnomeShellScreenshot,
            CaptureBackendKind::GnomeScreenshot,
            CaptureBackendKind::Grim,
            CaptureBackendKind::Spectacle,
            CaptureBackendKind::Flameshot,
        ],
        SessionType::X11 => vec![
            CaptureBackendKind::GnomeShellScreenshot,
            CaptureBackendKind::GnomeScreenshot,
            CaptureBackendKind::Maim,
            CaptureBackendKind::Scrot,
            CaptureBackendKind::ImageMagickImport,
            CaptureBackendKind::Spectacle,
            CaptureBackendKind::Flameshot,
        ],
        SessionType::Unknown => vec![
            CaptureBackendKind::GnomeShellScreenshot,
            CaptureBackendKind::GnomeScreenshot,
            CaptureBackendKind::Grim,
            CaptureBackendKind::Maim,
            CaptureBackendKind::Spectacle,
            CaptureBackendKind::Flameshot,
            CaptureBackendKind::Scrot,
            CaptureBackendKind::ImageMagickImport,
        ],
    }
}

fn backend_available(
    kind: CaptureBackendKind,
    command_exists: &impl Fn(&str) -> bool,
    gnome_shell_screenshot_available: bool,
) -> bool {
    match kind {
        CaptureBackendKind::Portal => portal_screenshot_summary(command_exists).is_some(),
        CaptureBackendKind::GnomeShellScreenshot => gnome_shell_screenshot_available,
        other => other.command().is_some_and(command_exists),
    }
}

fn portal_screenshot_summary(command_exists: &impl Fn(&str) -> bool) -> Option<String> {
    if !command_exists("gdbus") {
        return None;
    }

    let backends = screenshot_portal_backends_for_current_desktop();
    (!backends.is_empty()).then(|| summarize_portal_backends(&backends))
}

fn push_unique(backends: &mut Vec<CaptureBackendDescriptor>, backend: CaptureBackendDescriptor) {
    if !backends
        .iter()
        .any(|existing| existing.kind == backend.kind)
    {
        backends.push(backend);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_gnome_prefers_gnome_shell_before_cli_backends() {
        let backends =
            discover_capture_backends(SessionType::Wayland, |command| command == "grim", true);

        assert_eq!(backends[0].kind, CaptureBackendKind::GnomeShellScreenshot);
        assert_eq!(backends[1].kind, CaptureBackendKind::Grim);
    }

    #[test]
    fn rendered_backends_exclude_interactive_portal_only_backend() {
        let backends = discover_rendered_capture_backends(
            SessionType::Wayland,
            |command| command == "gdbus",
            false,
        );

        assert!(backends.is_empty());
    }

    #[test]
    fn x11_detects_classic_screenshot_tools() {
        let backends = discover_capture_backends(
            SessionType::X11,
            |command| matches!(command, "maim" | "scrot" | "import"),
            false,
        );

        assert_eq!(
            backends
                .iter()
                .map(|backend| backend.kind)
                .collect::<Vec<_>>(),
            vec![
                CaptureBackendKind::Maim,
                CaptureBackendKind::Scrot,
                CaptureBackendKind::ImageMagickImport,
            ]
        );
    }
}
