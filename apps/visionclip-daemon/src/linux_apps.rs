use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use which::which;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationLaunchResult {
    pub resolved_app: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopApp {
    desktop_id: String,
    path: PathBuf,
    name: String,
    generic_name: String,
    exec: String,
    try_exec: String,
    keywords: Vec<String>,
    categories: Vec<String>,
    no_display: bool,
    hidden: bool,
}

pub fn open_application(app_name: &str) -> Result<ApplicationLaunchResult> {
    let query = app_name.trim();
    if query.is_empty() {
        anyhow::bail!("application name is empty");
    }

    if let Some(result) = launch_well_known_app(query)? {
        return Ok(result);
    }

    let apps = installed_desktop_apps();
    let Some(app) = resolve_desktop_app(query, &apps) else {
        anyhow::bail!("no installed application matched `{query}`");
    };

    launch_desktop_app(&app)?;
    Ok(ApplicationLaunchResult {
        resolved_app: app.name.clone(),
        message: format!("Abrindo {}.", app.name),
    })
}

fn launch_well_known_app(query: &str) -> Result<Option<ApplicationLaunchResult>> {
    let normalized = normalize(query);
    if matches!(
        normalized.as_str(),
        "terminal" | "terminalemulator" | "console" | "shell"
    ) {
        let command = first_available(&[
            "x-terminal-emulator",
            "gnome-terminal",
            "kgx",
            "konsole",
            "xfce4-terminal",
            "alacritty",
            "kitty",
        ])
        .context("no supported terminal emulator found")?;
        spawn_detached(&command, &[])?;
        return Ok(Some(ApplicationLaunchResult {
            resolved_app: command.clone(),
            message: "Abrindo o terminal.".into(),
        }));
    }

    if matches!(normalized.as_str(), "browser" | "navegador" | "webbrowser") {
        if let Some(command) = first_available(&["xdg-open"]) {
            spawn_detached(&command, &["about:blank"])?;
            return Ok(Some(ApplicationLaunchResult {
                resolved_app: "default browser".into(),
                message: "Abrindo o navegador.".into(),
            }));
        }
    }

    if matches!(
        normalized.as_str(),
        "settings" | "configuracoes" | "ajustes" | "gnomesettings"
    ) {
        let command = first_available(&[
            "gnome-control-center",
            "systemsettings",
            "xfce4-settings-manager",
        ])
        .context("no supported settings application found")?;
        spawn_detached(&command, &[])?;
        return Ok(Some(ApplicationLaunchResult {
            resolved_app: command.clone(),
            message: "Abrindo as configurações.".into(),
        }));
    }

    Ok(None)
}

fn launch_desktop_app(app: &DesktopApp) -> Result<()> {
    if let Some(command) = first_available(&["gtk-launch", "gtk4-launch"]) {
        spawn_detached(&command, &[app.desktop_id.as_str()])
            .with_context(|| format!("failed to launch desktop app `{}`", app.desktop_id))?;
        return Ok(());
    }

    if let Some(command) = first_available(&["gio"]) {
        let path = app.path.display().to_string();
        spawn_detached(&command, &["launch", path.as_str()])
            .with_context(|| format!("failed to launch desktop file `{}`", app.path.display()))?;
        return Ok(());
    }

    anyhow::bail!("no safe desktop launcher found; install `gtk-launch` or `gio`");
}

fn spawn_detached(program: &str, args: &[&str]) -> Result<()> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to spawn `{}`", render_command(program, args)))?;
    Ok(())
}

fn first_available(commands: &[&str]) -> Option<String> {
    commands
        .iter()
        .find(|command| which(command).is_ok())
        .map(|command| (*command).to_string())
}

fn installed_desktop_apps() -> Vec<DesktopApp> {
    let mut apps = Vec::new();
    for dir in application_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            if let Some(app) = parse_desktop_app(&path, &raw) {
                if app.hidden
                    || app.no_display
                    || app.name.trim().is_empty()
                    || !try_exec_is_available(&app)
                {
                    continue;
                }
                apps.push(app);
            }
        }
    }
    apps
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];

    if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }

    dirs
}

fn parse_desktop_app(path: &Path, raw: &str) -> Option<DesktopApp> {
    let desktop_id = path.file_stem()?.to_string_lossy().to_string();
    let mut in_desktop_entry = false;
    let mut fields = HashMap::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        fields.entry(key.to_string()).or_insert(value.to_string());
    }

    let app_type = fields
        .get("Type")
        .map(String::as_str)
        .unwrap_or("Application");
    if app_type != "Application" {
        return None;
    }

    Some(DesktopApp {
        desktop_id,
        path: path.to_path_buf(),
        name: localized_field(&fields, "Name").unwrap_or_default(),
        generic_name: localized_field(&fields, "GenericName").unwrap_or_default(),
        exec: fields.get("Exec").cloned().unwrap_or_default(),
        try_exec: fields.get("TryExec").cloned().unwrap_or_default(),
        keywords: split_desktop_list(
            localized_field(&fields, "Keywords")
                .as_deref()
                .unwrap_or(""),
        ),
        categories: split_desktop_list(fields.get("Categories").map(String::as_str).unwrap_or("")),
        no_display: parse_bool(
            fields
                .get("NoDisplay")
                .map(String::as_str)
                .unwrap_or("false"),
        ),
        hidden: parse_bool(fields.get("Hidden").map(String::as_str).unwrap_or("false")),
    })
}

fn split_desktop_list(input: &str) -> Vec<String> {
    input
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn localized_field(fields: &HashMap<String, String>, key: &str) -> Option<String> {
    for locale in locale_candidates() {
        let localized_key = format!("{key}[{locale}]");
        if let Some(value) = fields.get(&localized_key).filter(|value| !value.is_empty()) {
            return Some(value.clone());
        }
    }

    fields.get(key).filter(|value| !value.is_empty()).cloned()
}

fn locale_candidates() -> Vec<String> {
    let mut values = Vec::new();
    for var_name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        let Ok(raw) = env::var(var_name) else {
            continue;
        };
        let locale = raw
            .split('.')
            .next()
            .unwrap_or("")
            .split('@')
            .next()
            .unwrap_or("")
            .trim();
        if locale.is_empty() || locale == "C" || locale == "POSIX" {
            continue;
        }
        values.push(locale.to_string());
        if let Some((language, _)) = locale.split_once('_') {
            values.push(language.to_string());
        }
    }
    values.sort();
    values.dedup();
    values
}

fn try_exec_is_available(app: &DesktopApp) -> bool {
    let try_exec = app.try_exec.trim();
    if try_exec.is_empty() {
        return true;
    }
    if try_exec.contains('/') {
        Path::new(try_exec).is_file()
    } else {
        which(try_exec).is_ok()
    }
}

fn parse_bool(input: &str) -> bool {
    input.eq_ignore_ascii_case("true") || input == "1"
}

fn resolve_desktop_app(query: &str, apps: &[DesktopApp]) -> Option<DesktopApp> {
    let query_norm = normalize(query);
    if query_norm.is_empty() {
        return None;
    }

    let aliases = app_aliases(&query_norm);
    apps.iter()
        .filter_map(|app| {
            let score = score_app(&query_norm, &aliases, app);
            (score >= MIN_DESKTOP_APP_SCORE).then_some((score, app.clone()))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, app)| app)
}

const MIN_DESKTOP_APP_SCORE: u16 = 55;

fn app_aliases(query_norm: &str) -> Vec<String> {
    let mut aliases = vec![query_norm.to_string()];
    match query_norm {
        "vscode" | "visualstudiocode" | "code" => {
            aliases.extend(["code", "visualstudiocode", "vscode"].map(str::to_string));
        }
        "firefox" | "mozillafirefox" => {
            aliases.extend(["firefox", "mozillafirefox"].map(str::to_string));
        }
        "chrome" | "googlechrome" => {
            aliases.extend(["chrome", "googlechrome"].map(str::to_string));
        }
        "burp" | "burpsuite" | "burpsuitecommunity" => {
            aliases.extend(["burp", "burpsuite", "burpsuitecommunity"].map(str::to_string));
        }
        "settings" | "configuracoes" | "ajustes" | "gnomesettings" => {
            aliases.extend(
                [
                    "settings",
                    "configuracoes",
                    "preferences",
                    "gnomecontrolcenter",
                ]
                .map(str::to_string),
            );
        }
        _ => {}
    }
    aliases.sort();
    aliases.dedup();
    aliases
}

fn score_app(query_norm: &str, aliases: &[String], app: &DesktopApp) -> u16 {
    let name = normalize(&app.name);
    let generic_name = normalize(&app.generic_name);
    let desktop_id = normalize(&app.desktop_id);
    let exec = normalize(&app.exec);
    let keywords = app
        .keywords
        .iter()
        .map(|item| normalize(item))
        .collect::<Vec<_>>();
    let categories = app
        .categories
        .iter()
        .map(|item| normalize(item))
        .collect::<Vec<_>>();

    let mut best = 0;
    for alias in aliases {
        best = best.max(match_field(alias, &name, 100));
        best = best.max(match_field(alias, &desktop_id, 94));
        best = best.max(match_field(alias, &exec, 88));
        best = best.max(match_field(alias, &generic_name, 72));
        for keyword in &keywords {
            best = best.max(match_field(alias, keyword, 82));
        }
        for category in &categories {
            best = best.max(match_field(alias, category, 48));
        }
    }

    if best == 0
        && query_norm
            .split_whitespace()
            .all(|token| name.contains(token))
    {
        best = 55;
    }

    best
}

fn match_field(query: &str, field: &str, exact_score: u16) -> u16 {
    if field.is_empty() {
        0
    } else if field == query {
        exact_score
    } else if query.chars().count() <= 2 {
        0
    } else if field.starts_with(query) {
        exact_score.saturating_sub(12)
    } else if field.contains(query) {
        exact_score.saturating_sub(24)
    } else {
        0
    }
}

fn normalize(input: &str) -> String {
    input
        .chars()
        .filter_map(|ch| match ch {
            'á' | 'à' | 'ã' | 'â' | 'ä' | 'Á' | 'À' | 'Ã' | 'Â' | 'Ä' => Some('a'),
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => Some('e'),
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => Some('i'),
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Õ' | 'Ô' | 'Ö' => Some('o'),
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => Some('u'),
            'ç' | 'Ç' => Some('c'),
            ch if ch.is_ascii_alphanumeric() => Some(ch.to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

fn render_command(program: &str, args: &[&str]) -> String {
    if args.is_empty() {
        program.to_string()
    } else {
        format!("{program} {}", args.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_desktop_entry_fields() {
        let app = parse_desktop_app(
            Path::new("/usr/share/applications/code.desktop"),
            r#"
            [Desktop Entry]
            Type=Application
            Name=Visual Studio Code
            GenericName=Text Editor
            Exec=/usr/share/code/code --unity-launch %F
            Keywords=vscode;code;editor;
            Categories=Development;IDE;
            "#,
        )
        .expect("desktop app");

        assert_eq!(app.desktop_id, "code");
        assert_eq!(app.name, "Visual Studio Code");
        assert!(app.keywords.contains(&"vscode".to_string()));
    }

    #[test]
    fn parses_localized_desktop_entry_fields() {
        let app = parse_desktop_app(
            Path::new("/usr/share/applications/browser.desktop"),
            r#"
            [Desktop Entry]
            Type=Application
            Name=Browser
            Name[pt_BR]=Navegador
            Exec=browser
            Keywords=web;
            Keywords[pt_BR]=internet;navegador;
            "#,
        )
        .expect("desktop app");

        if locale_candidates().iter().any(|locale| locale == "pt_BR") {
            assert_eq!(app.name, "Navegador");
            assert!(app.keywords.contains(&"navegador".to_string()));
        } else {
            assert_eq!(app.name, "Browser");
        }
    }

    #[test]
    fn resolves_vscode_alias_to_code_desktop_entry() {
        let apps = vec![parse_desktop_app(
            Path::new("/usr/share/applications/code.desktop"),
            r#"
            [Desktop Entry]
            Type=Application
            Name=Visual Studio Code
            Exec=code %F
            Keywords=vscode;code;
            "#,
        )
        .unwrap()];

        let resolved = resolve_desktop_app("vscode", &apps).expect("resolved app");
        assert_eq!(resolved.desktop_id, "code");
    }

    #[test]
    fn resolves_security_and_system_app_aliases() {
        let apps = vec![
            parse_desktop_app(
                Path::new("/usr/share/applications/burpsuite.desktop"),
                r#"
                [Desktop Entry]
                Type=Application
                Name=Burp Suite Community Edition
                Exec=burpsuite
                Keywords=burp;proxy;security;
                "#,
            )
            .unwrap(),
            parse_desktop_app(
                Path::new("/usr/share/applications/org.wireshark.Wireshark.desktop"),
                r#"
                [Desktop Entry]
                Type=Application
                Name=Wireshark
                Exec=wireshark %f
                Keywords=network;packet;
                "#,
            )
            .unwrap(),
            parse_desktop_app(
                Path::new("/usr/share/applications/org.gnome.Settings.desktop"),
                r#"
                [Desktop Entry]
                Type=Application
                Name=Settings
                Name[pt_BR]=Configurações
                Exec=gnome-control-center
                Keywords=settings;preferences;configuracoes;
                "#,
            )
            .unwrap(),
        ];

        assert_eq!(
            resolve_desktop_app("BurpSuite", &apps)
                .expect("burp suite")
                .desktop_id,
            "burpsuite"
        );
        assert_eq!(
            resolve_desktop_app("wireshark", &apps)
                .expect("wireshark")
                .desktop_id,
            "org.wireshark.Wireshark"
        );
        assert_eq!(
            resolve_desktop_app("configurações", &apps)
                .expect("settings")
                .desktop_id,
            "org.gnome.Settings"
        );
    }

    #[test]
    fn rejects_low_confidence_desktop_app_matches() {
        let apps = vec![parse_desktop_app(
            Path::new("/usr/share/applications/netmask.desktop"),
            r#"
            [Desktop Entry]
            Type=Application
            Name=Netmask
            Exec=netmask
            Keywords=network;internet;
            Categories=Network;
            "#,
        )
        .unwrap()];

        assert!(resolve_desktop_app("te mną", &apps).is_none());
        assert!(resolve_desktop_app("te", &apps).is_none());
        assert_eq!(
            resolve_desktop_app("netmask", &apps)
                .expect("exact app")
                .desktop_id,
            "netmask"
        );
    }

    #[test]
    fn ignores_hidden_desktop_entries() {
        let app = parse_desktop_app(
            Path::new("/usr/share/applications/hidden.desktop"),
            r#"
            [Desktop Entry]
            Type=Application
            Name=Hidden
            Hidden=true
            "#,
        )
        .unwrap();

        assert!(app.hidden);
    }

    #[test]
    fn rejects_missing_try_exec() {
        let app = parse_desktop_app(
            Path::new("/usr/share/applications/missing.desktop"),
            r#"
            [Desktop Entry]
            Type=Application
            Name=Missing
            Exec=missing-app
            TryExec=/definitely/missing/visionclip-test-binary
            "#,
        )
        .unwrap();

        assert!(!try_exec_is_available(&app));
    }
}
