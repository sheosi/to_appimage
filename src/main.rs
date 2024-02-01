use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

use image::imageops::resize;
use serde::Serialize;
use thiserror::Error;

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

fn extract_icon_from_exe(dir: &Path, file: &str) {
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

fn is_archive(path: &Path) -> bool {
    ["zip", "tar", "tar.gz", "tar.bz2", "7z"]
        .contains(&path.extension().unwrap_or_default().to_str().unwrap_or(""))
}

#[derive(Debug, Error)]
enum Error {
    #[error("unsupported archive format '{0}'")]
    ArchiveFormatNotSupported(String),
}

fn unarchive_input<P2>(input: &Path, output: P2) -> Result<(), Error>
where
    P2: AsRef<Path>,
{
    match input.extension().unwrap_or_default().to_str().unwrap_or("") {
        "zip" => {
            assert!(Command::new("/usr/bin/unzip")
                .arg(input)
                .arg("-d")
                .arg(output.as_ref())
                .output()
                .unwrap()
                .status
                .success());
            Ok(())
        }
        a => Err(Error::ArchiveFormatNotSupported(a.to_string())),
    }
}

fn resize_img(input: &Path, output: &Path) -> image::ImageResult<()> {
    use image::io::Reader as ImageReader;

    let img = ImageReader::open(input)?.decode()?;
    resize(&img, 256, 256, image::imageops::FilterType::Lanczos3).save(output)
}

fn main() {
    use dialog::DialogBox;

    let input = PathBuf::from_str(&std::env::args().nth(1).unwrap())
        .unwrap()
        .canonicalize()
        .unwrap();

    let (actual_input, is_temp) = if is_archive(&input) {
        let tmp_path = Path::new("/tmp/to_appimage").join(
            input
                .file_stem()
                .map(|s| s.to_str().unwrap_or(""))
                .unwrap_or(""),
        );

        // Clean any leftover temporary files, this makes using unarchiver
        // way easier
        if tmp_path.exists() {
            std::fs::remove_dir_all(&tmp_path).unwrap();
        }
        fs::create_dir_all(&tmp_path).unwrap();

        unarchive_input(&input, &tmp_path).unwrap();

        let tmp_path = if fs::read_dir(&tmp_path).unwrap().count() == 1 {
            // Count consumes the whole iterator and ReadDir can't be cloned,
            // so we need to read the directory
            if let Some(Ok(first_item)) = fs::read_dir(&tmp_path).unwrap().next() {
                first_item.path()
            } else {
                tmp_path
            }
        } else {
            tmp_path
        };
        (tmp_path, true)
    } else {
        (input, false)
    };

    let icon = if actual_input.join("AppIcon.png").exists() {
        Some("AppIcon".to_string())
    } else if let Some(exe_name) = look_for_ext(&actual_input, "exe") {
        extract_icon_from_exe(&actual_input, exe_name.to_str().unwrap());
        Some("AppIcon".to_string())
    } else {
        dialog::Message::new(
            "No suitable icon could be found, appoint to a path where one can be found",
        ).show().expect("Couldn't show message");
        let icon_path = dialog::FileSelection::new("Select icon")
            .title("Select icon")
            .show()
            .expect("Couldn't show dialog");

        resize_img(
            Path::new(&icon_path.unwrap()),
            &actual_input.join("AppIcon.png"),
        )
        .unwrap();
        Some("AppIcon".to_string())
    };

    let executable = if let Some(shell_file) = look_for_ext(&actual_input, "sh") {
        shell_file
    } else if let Some(linux_exe) = look_for_ext(&actual_input, "x86_64") {
        linux_exe
    } else {
        panic!("Couldn't find any suitable executable")
    };

    let entry = DesktopFile::new(
        executable
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string(),
        icon,
        vec!["Game".to_string()],
    );

    let app_desktop = File::create(actual_input.join("app.desktop")).unwrap();
    desktop_entry::to_writer(app_desktop, &entry).unwrap();
    std::fs::copy(executable, actual_input.join("AppRun")).unwrap();

    let appimagetool_name = "gearlever_appimagetool_d3afa1.appimage";
    let output = Command::new(which::which(appimagetool_name).unwrap())
        .arg(&actual_input)
        .output()
        .unwrap();
    println!("{}", String::from_utf8(output.stderr).unwrap());
    assert!(output.status.success());

    if is_temp {
        fs::remove_dir_all(actual_input).unwrap();
    }
}
