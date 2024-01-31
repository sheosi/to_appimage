use std::{fs::File, path::{Path, PathBuf}, process::Command, str::FromStr};

use serde::Serialize;

mod desktop_entry;

#[derive(Serialize)]
struct DesktopFile {
    #[serde(rename = "Desktop Entry")]
    file: DesktopEntry,
}

#[derive(Serialize)]
struct DesktopEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Exec")]
    exec: String,
    #[serde(rename = "Icon")]
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(rename = "Type")]
    d_type: String,
    #[serde(rename = "Categories")]
    categories: Vec<String>,
}

impl DesktopFile {
    pub fn new(name: String, icon: Option<String>, categories: Vec<String>) -> Self {
        Self {
            file: DesktopEntry {
                name,
                exec: "./AppRun".to_string(),
                d_type: "Application".to_string(),
                icon,
                categories,
            },
        }
    }
}

fn call_through_toolbox(container: &str, command: &str) -> Command {
    let mut c = Command::new("/usr/bin/toolbox");
    c.arg("run").arg("-c").arg(container).arg(command);
    c
}

fn extract_icon_from_exe(dir:&Path, file: &str) {
    let toolbox_distro = "ubuntu-toolbox-22.04";
    let output = call_through_toolbox(toolbox_distro, "wrestool")
        .arg("-x")
        .arg("--output=icon.ico")
        .arg("-t")
        .arg("14")
        .arg(file)
        .output()
        .unwrap();
    assert!(output.status.success());

    println!("{:?}",dir.join("AppIcon.png"));
    let output = call_through_toolbox(toolbox_distro, "icotool")
        .arg("-x")
        .arg("icon.ico")
        .arg("-h")
        .arg("256")
        .arg("-o")
        .arg(dir.join("AppIcon.png"))
        .output()
        .unwrap();
    println!("{}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());

    std::fs::remove_file("icon.ico").unwrap();
}

fn look_for_ext(path: &PathBuf, ext: &str) -> Option<PathBuf> {
    std::fs::read_dir(path)
        .unwrap()
        .filter(Result::is_ok)
        .map(Result::unwrap)
        .map(|d| d.path())
        .find(|p| {
            p.is_file()
                && p.extension()
                    .map(|e| e.to_str().unwrap_or(""))
                    .unwrap_or("")
                    == ext
        })
}

fn main() {
    let input = PathBuf::from_str(&std::env::args().nth(1).unwrap())
        .unwrap()
        .canonicalize()
        .unwrap();
    
    let icon = 
    if input.join("AppIcon.png").exists() {
        Some("AppIcon".to_string())
    } 
    else if let Some(exe_name) = look_for_ext(&input, "exe") {
        extract_icon_from_exe(&input, exe_name.to_str().unwrap());
        Some("AppIcon".to_string())
    }
    else {None};

    let sh_name = look_for_ext(&input, "sh").unwrap();

    let entry = DesktopFile::new(
        sh_name.file_stem().unwrap().to_str().unwrap().to_string(),
        icon,
        vec!["Game".to_string()],
    );

    let app_desktop = File::create(input.join("app.desktop")).unwrap();
    desktop_entry::to_writer(app_desktop, &entry).unwrap();
    std::fs::copy(sh_name, input.join("AppRun")).unwrap();

    let appimagetool_name = "gearlever_appimagetool_d3afa1.appimage";
    let output = Command::new(which::which(appimagetool_name).unwrap())
        .arg(input)
        .output()
        .unwrap();
    println!("{}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());
}
