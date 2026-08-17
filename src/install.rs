use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const SERVICE_NAME: &str = "claude-usage-tray.service";
const DESKTOP_NAME: &str = "claude-usage-tray.desktop";
const ICON_NAME: &str = "claude-usage-tray.png";

const SERVICE_UNIT: &str = include_str!("../dist/claude-usage-tray.service");
const DESKTOP_ENTRY: &str = include_str!("../dist/claude-usage-tray.desktop");
const ICON_PNG: &[u8] = include_bytes!("../assets/default-icon.png");

fn config_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = match env::var("XDG_CONFIG_HOME") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => PathBuf::from(env::var("HOME")?).join(".config"),
    };

    Ok(dir)
}

fn data_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = match env::var("XDG_DATA_HOME") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => PathBuf::from(env::var("HOME")?).join(".local/share"),
    };

    Ok(dir)
}

pub fn install() -> Result<(), Box<dyn std::error::Error>> {
    let unit_dir = config_home()?.join("systemd/user");
    let apps_dir = data_home()?.join("applications");
    let icon_dir = data_home()?.join("icons/hicolor/128x128/apps");

    fs::create_dir_all(&unit_dir)?;
    fs::create_dir_all(&apps_dir)?;
    fs::create_dir_all(&icon_dir)?;

    fs::write(unit_dir.join(SERVICE_NAME), SERVICE_UNIT)?;
    fs::write(apps_dir.join(DESKTOP_NAME), DESKTOP_ENTRY)?;
    fs::write(icon_dir.join(ICON_NAME), ICON_PNG)?;

    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    let _ = Command::new("update-desktop-database")
        .arg(&apps_dir)
        .status();

    println!("Installed:");
    println!("  {}", unit_dir.join(SERVICE_NAME).display());
    println!("  {}", apps_dir.join(DESKTOP_NAME).display());
    println!("  {}", icon_dir.join(ICON_NAME).display());
    println!();
    println!("Not enabled. To autostart on login, run:");
    println!("  systemctl --user enable --now {SERVICE_NAME}");
    Ok(())
}

pub fn uninstall() -> Result<(), Box<dyn std::error::Error>> {
    let unit_dir = config_home()?.join("systemd/user");
    let apps_dir = data_home()?.join("applications");
    let icon_dir = data_home()?.join("icons/hicolor/128x128/apps");

    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", SERVICE_NAME])
        .status();

    let _ = fs::remove_file(unit_dir.join(SERVICE_NAME));
    let _ = fs::remove_file(apps_dir.join(DESKTOP_NAME));
    let _ = fs::remove_file(icon_dir.join(ICON_NAME));

    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    
    let _ = Command::new("update-desktop-database")
        .arg(&apps_dir)
        .status();

    println!("Uninstalled.");
    Ok(())
}
