use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

use appstream::{
    AppStream, AppStreamComponent, ComponentType, Launchable, LaunchableType, Provides,
};
use clap::Parser;
use cmd::RunExt;
use image::imageops::resize;
use itertools::Itertools;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml::to_writer;
use thiserror::Error;

#[derive(Parser, Debug)]
struct AppImageArgs {
    #[arg(short, long, default_value_t = false)]
    terminal: bool,

    #[arg(short, long)]
    categories: Vec<String>,

    target: String,
}

mod appstream;
mod desktop_entry;

#[derive(Serialize)]
struct DesktopFile {
    #[serde(rename = "Desktop Entry")]
    file: DesktopEntry,
}

// Just here for use with skip_serializing_if
fn is_false(val: &bool) -> bool {
    *val
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
    #[serde(rename = "Terminal")]
    #[serde(skip_serializing_if = "is_false")]
    terminal: bool,
}

#[derive(Serialize)]
struct Pkg2AppimageDescriptor {
    app: String,
    ingredients: Pkg2AppimageDescriptorIngredients,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    script: Vec<String>,
}

#[derive(Default, Serialize)]
struct Pkg2AppimageDescriptorIngredients {
    dist: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    sources: Vec<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    packages: Vec<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    script: Vec<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    debs: Vec<String>,
}

impl DesktopFile {
    pub fn new(
        name: String,
        icon: Option<String>,
        categories: Vec<String>,
        terminal: bool,
    ) -> Self {
        Self {
            file: DesktopEntry {
                name,
                exec: "./AppRun".to_string(),
                d_type: "Application".to_string(),
                icon,
                categories,
                terminal,
            },
        }
    }
}

#[derive(Copy, Clone, Default, Deserialize)]
enum CliKind {
    Native,
    #[default]
    Toolbox,
}

#[derive(Deserialize)]
struct CliConf {
    #[serde(default)]
    kind: CliKind,

    #[serde(default = "default_container_name")]
    container_name: String,
}

fn default_container_name() -> String {
    "ubuntu-toolbox-22.04".to_string()
}

impl Default for CliConf {
    fn default() -> CliConf {
        CliConf {
            kind: CliKind::Toolbox,
            container_name: default_container_name(),
        }
    }
}

fn extract_icon_from_exe(conf: &CliConf, dir: &Path, file: &str) {
    cmd::app_from("wrestool", conf.kind, Some(&conf.container_name))
        .unwrap()
        .arg("-x")
        .arg("--output=icon.ico")
        .arg("-t")
        .arg("14")
        .arg(file)
        .run()
        .unwrap();

    cmd::app_from("icotool", CliKind::Native, Some(&conf.container_name))
        .unwrap()
        .arg("-x")
        .arg("icon.ico")
        .arg("-h")
        .arg("256")
        .arg("-o")
        .arg(dir.join("AppIcon.png"))
        .run_outerr()
        .unwrap();

    std::fs::remove_file("icon.ico").unwrap();
}

fn look_for_ext(path: &PathBuf, ext: &str) -> Option<PathBuf> {
    std::fs::read_dir(path)
        .unwrap()
        .flatten()
        .map(|d| d.path())
        .find(|p| {
            p.is_file()
                && p.extension()
                    .map(|e| e.to_str().unwrap_or(""))
                    .unwrap_or("")
                    == ext
        })
}

fn look_for_no_exts(path: &PathBuf) -> Vec<PathBuf> {
    #[allow(clippy::ptr_arg)]
    fn is_exe_no_ext(p: &PathBuf) -> bool {
        let file_name_lower = p
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
            .to_lowercase();
        p.is_file()
            && p.extension().is_none()
            && !["legal_details", "license", "readme"].contains(&file_name_lower.as_str())
    }
    std::fs::read_dir(path)
        .unwrap()
        .flatten()
        .map(|d| d.path())
        .filter(is_exe_no_ext)
        .collect()
}

#[derive(Debug, Error)]
enum Error {
    #[error("unsupported archive format '{0}'")]
    ArchiveFormatNotSupported(String),
}

mod archive {
    use crate::{cmd, cmd::RunExt, Error};
    use itertools::Itertools;
    use path_utils::PathExt;
    use std::path::Path;

    pub fn is_archive(path: &Path) -> bool {
        // Due to how this works, the extensions are reversed, that's why they
        // are written this way
        ["zip", "tar", "gz.tar", "gz2.tar", "7z"]
            .contains(&path.extensions_lossy().join(".").as_str())
    }

    enum Archive {
        Zip,
        Tar, // Everything can be processed by the tar tool, so we are making no distinctions
    }

    impl Archive {
        fn guess<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
            match path.as_ref().extensions_lossy().join(".").as_str() {
                "zip" => Ok(Archive::Zip),
                "gz.tar" | "tar" => Ok(Archive::Tar),
                a => Err(Error::ArchiveFormatNotSupported(a.to_string())),
            }
        }
    }

    pub fn unarchive<P2>(input: &Path, output: P2) -> Result<(), Error>
    where
        P2: AsRef<Path>,
    {
        match Archive::guess(input)? {
            Archive::Zip => {
                cmd::app("unzip")
                    .unwrap()
                    .arg(input)
                    .arg("-d")
                    .arg(output.as_ref())
                    .run()
                    .unwrap();
                Ok(())
            }
            Archive::Tar => {
                cmd::app("tar")
                    .unwrap()
                    .arg("-xf")
                    .arg(input)
                    .arg("-C")
                    .arg(output.as_ref())
                    .run()
                    .unwrap();
                Ok(())
            }
        }
    }
}

fn resize_img(input: &Path, output: &Path) -> image::ImageResult<()> {
    use image::io::Reader as ImageReader;

    let img = ImageReader::open(input)?.decode()?;
    resize(&img, 256, 256, image::imageops::FilterType::Lanczos3).save(output)
}

trait ExtUtils {
    fn get_ext(&self) -> &str;
    fn is_ext(&self, ext: &str) -> bool;
}

impl ExtUtils for &Path {
    fn get_ext(&self) -> &str {
        self.extension().unwrap_or_default().to_str().unwrap_or("")
    }
    fn is_ext(&self, ext: &str) -> bool {
        self.get_ext() == ext
    }
}

impl ExtUtils for PathBuf {
    fn get_ext(&self) -> &str {
        self.extension().unwrap_or_default().to_str().unwrap_or("")
    }
    fn is_ext(&self, ext: &str) -> bool {
        self.get_ext() == ext
    }
}

fn download_file(url: &str, file_name: &str) {
    if let Some(mut curl) = cmd::app("curl") {
        curl.args(["-L", url, "-o", file_name]).run().unwrap();
    } else if let Some(mut wget) = cmd::app("wget") {
        wget.args([url, "-O", file_name]).run().unwrap();
    } else {
        panic!("There's no available program for downloading files!")
    }
}

fn download_to_temp(tmp_path: &Path, url: &str) -> String {
    let tmp_path_str = tmp_path.to_str().unwrap();
    if let Some(mut curl) = cmd::app("curl") {
        curl.args(["-O", "-L", "--output-dir", tmp_path_str, url])
            .run()
            .unwrap();
    } else if let Some(mut wget) = cmd::app("wget") {
        wget.args([url, "-P", tmp_path_str]).run().unwrap();
    } else {
        panic!("There's no available program for downloading files!")
    }

    tmp_path
        .read_dir()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .to_str()
        .unwrap()
        .to_owned()
}

enum PkgType {
    Deb(PathBuf),
    Yaml(PathBuf),
    Other(PathBuf),
}

mod temp {
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    fn get_common() -> PathBuf {
        Path::new("/tmp/to_appimage").into()
    }
    fn get_base() -> PathBuf {
        get_common().join(std::process::id().to_string())
    }

    pub fn get(identifier: &str) -> PathBuf {
        get_base().join(identifier)
    }

    pub fn try_create(identifier: &str) -> PathBuf {
        let tmp_path = get(identifier);
        if !tmp_path.exists() {
            fs::create_dir_all(&tmp_path).unwrap();
        }
        tmp_path
    }

    pub fn clean_everything() {
        let tmp_path = get_base();
        if tmp_path.exists() {
            fs::remove_dir_all(&tmp_path).unwrap();
        }

        // Erase /tmp/to_appimage if it's empty
        let common = get_common();
        if common.read_dir().unwrap().next().is_none() {
            std::fs::remove_dir(common).unwrap();
        }
    }
}

impl PkgType {
    fn guess(input: &str) -> Self {
        if input.starts_with("http") {
            let temp = temp::try_create("download");
            let temp_data = download_to_temp(&temp, input);
            Self::guess_local(&temp_data)
        } else {
            Self::guess_local(input)
        }
    }

    fn guess_local(input: &str) -> Self {
        println!("{}", input);
        let path = PathBuf::from_str(input).unwrap().canonicalize().unwrap();

        if path.is_ext("deb") {
            PkgType::Deb(path)
        } else if path.is_ext("yaml") {
            PkgType::Yaml(path)
        } else {
            PkgType::Other(path)
        }
    }
}

fn run_pkgtoappimage(yml: &Path) {
    let status = Command::new("gearlever_pkg2appimage_02a375.appimage")
        .arg(yml)
        .output()
        .unwrap();

    if !status.status.success() {
        dialog::Message::new(String::from_utf8(status.stderr).unwrap());
    }
}

mod cmd {
    use std::process::Command;

    use crate::{CliKind, Error};

    pub fn app(name: &str) -> Option<Command> {
        if let Ok(app_path) = which::which(name) {
            Some(Command::new(app_path))
        } else {
            None
        }
    }
    pub fn app_from(name: &str, kind: CliKind, container: Option<&str>) -> Option<Command> {
        if matches!(kind, CliKind::Native) {
            app(name)
        } else {
            Some(app_from_toolbox(container.unwrap(), name))
        }
    }

    fn app_from_toolbox(container: &str, command: &str) -> Command {
        let mut c = Command::new("/usr/bin/toolbox");
        c.arg("run").arg("-c").arg(container).arg(command);
        c
    }

    pub trait RunExt {
        fn run(&mut self) -> Result<(), Error>;
        fn run_outerr(&mut self) -> Result<(), Error>;
    }

    impl RunExt for &mut Command {
        // TODO: Actually produce errors from this
        fn run(&mut self) -> Result<(), Error> {
            let out = self.status().unwrap();
            assert!(out.success());
            Ok(())
        }

        fn run_outerr(&mut self) -> Result<(), Error> {
            let out = self.output().unwrap();
            if !out.status.success() {
                println!("{}", String::from_utf8(out.stderr).unwrap());
            }

            assert!(out.status.success());
            Ok(())
        }
    }
}

fn main() {
    use dialog::DialogBox;

    let conf = CliConf::default();
    let args = AppImageArgs::parse();

    match PkgType::guess(&args.target) {
        PkgType::Deb(input) => {
            let name_reg = Regex::new("^[A-Za-z-0-9]*").unwrap();
            let name = name_reg
                .captures(input.file_name().unwrap().to_str().unwrap())
                .unwrap()
                .get(0)
                .unwrap()
                .as_str();

            let descriptor = Pkg2AppimageDescriptor {
                app: name.to_string(),
                ingredients: Pkg2AppimageDescriptorIngredients {
                    dist: Some("trusty".to_string()),
                    packages: vec![name.replace(' ', "-").to_lowercase()],
                    sources: vec![
                        "deb http://archive.ubuntu.com/ubuntu/ trusty main universe".to_string()
                    ],
                    debs: vec![input.to_str().unwrap().to_string()],
                    ..Default::default()
                },
                script: vec!["ls".to_string()],
            };

            let with_yaml_ext = input.with_extension("yaml");
            let p_descriptor = with_yaml_ext.file_name().unwrap();
            let f_descriptor = File::create(p_descriptor).unwrap();
            to_writer(&f_descriptor, &descriptor).unwrap();
            run_pkgtoappimage(Path::new(p_descriptor));
        }
        PkgType::Yaml(input) => {
            run_pkgtoappimage(&input);
        }
        PkgType::Other(input) => {
            let actual_input = if archive::is_archive(&input) {
                let tmp_path = temp::try_create(
                    input
                        .file_stem()
                        .map(|s| s.to_str().unwrap_or(""))
                        .unwrap_or("archive_out"),
                );

                // Clean any leftover temporary files, this makes using unarchiver
                // way easier
                if tmp_path.exists() {
                    std::fs::remove_dir_all(&tmp_path).unwrap();
                }
                fs::create_dir_all(&tmp_path).unwrap();

                archive::unarchive(&input, &tmp_path).unwrap();

                if fs::read_dir(&tmp_path).unwrap().count() == 1 {
                    // Count consumes the whole iterator and ReadDir can't be cloned,
                    // so we need to read the directory
                    if let Some(Ok(first_item)) = fs::read_dir(&tmp_path).unwrap().next() {
                        first_item.path()
                    } else {
                        tmp_path
                    }
                } else {
                    tmp_path
                }
            } else {
                input
            };

            // Due to how the pkg2appimagetool works we NEED an icon, that's why it isn't an
            // option
            let icon = if actual_input.join("AppIcon.png").exists() {
                "AppIcon".to_string()
            } else if let Some(exe_name) = look_for_ext(&actual_input, "exe") {
                extract_icon_from_exe(&conf, &actual_input, exe_name.to_str().unwrap());
                "AppIcon".to_string()
            } else {
                dialog::Message::new(
                    "No suitable icon could be found, appoint to a path where one can be found",
                )
                .show()
                .expect("Couldn't show message");
                let icon_path = dialog::FileSelection::new("Select icon")
                    .title("Select icon")
                    .show()
                    .expect("Couldn't show dialog");

                let icon_path = PathBuf::from(icon_path.unwrap());
                if icon_path.exists() && icon_path.is_file() {
                    resize_img(&icon_path, &actual_input.join("AppIcon.png")).unwrap();
                    "AppIcon".to_string()
                } else {
                    dialog::Message::new("No icon was selected, one will be downloaded")
                        .show()
                        .expect("Couldn't show message");
                    download_file("https://icons.iconarchive.com/icons/kxmylo/simple/256/application-default-icon.png", actual_input.join("AppIcon.png").to_str().unwrap());
                    "AppIcon".to_string()
                }
            };

            let executable = if let Some(shell_file) = look_for_ext(&actual_input, "sh") {
                shell_file
            } else if let Some(linux_exe) = look_for_ext(&actual_input, "x86_64") {
                linux_exe
            } else {
                let exes = look_for_no_exts(&actual_input);
                if exes.is_empty() {
                    panic!("Couldn't find any suitable executable")
                } else if exes.len() == 1 {
                    exes.first().unwrap().clone()
                } else {
                    let parent_folder = actual_input.to_string_lossy().to_string();

                    fn display_pathbuf(prefix: &str, pb: &Path) -> String {
                        let full_path = pb.to_str().unwrap().to_owned();

                        if full_path.starts_with(prefix) {
                            full_path[prefix.len() + 1..].to_string()
                        } else {
                            full_path
                        }
                    }

                    let def_exe_path = exes.first().unwrap().clone();
                    let def_exe = display_pathbuf(&parent_folder, &def_exe_path);

                    let question = format!(
                        "Multiple exes where found: {}, which one do you want to use?",
                        exes.iter()
                            .map(|p| display_pathbuf(&parent_folder, p))
                            .join(", ")
                    );

                    let mut exe_pb = None;
                    while exe_pb.is_none() {
                        let name = dialog::Input::new(&question)
                            .title("Which executable?")
                            .default(&def_exe)
                            .show()
                            .expect("Failed to show message")
                            .unwrap();

                        exe_pb = exes
                            .iter()
                            .find(|p| display_pathbuf(&parent_folder, p) == name);

                        if exe_pb.is_none() {
                            dialog::Message::new("Please select a valid executable")
                                .show()
                                .expect("Failed to show message")
                        }
                    }
                    exe_pb.unwrap().clone()
                }
            };

            let entry = DesktopFile::new(
                executable
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
                Some(icon),
                args.categories,
                args.terminal,
            );

            let app_desktop = File::create(actual_input.join("app.desktop")).unwrap();
            desktop_entry::to_writer(app_desktop, &entry).unwrap();
            std::fs::copy(executable, actual_input.join("AppRun")).unwrap();

            // Make appstream
            // usr/share/metainfo/myapp.appdata.xml
            let appstream = AppStream {
                component: AppStreamComponent {
                    ctype: if args.terminal {
                        ComponentType::ConsoleApplication
                    } else {
                        ComponentType::DesktopApplication
                    },
                    id: "TODO!".to_string(),
                    metadata_license: "TODO!".to_string(),
                    project_license: "TODO!".to_string(),
                    name: "TODO!".to_string(),
                    summary: "TODO!".to_string(),
                    description: "TODO!".to_string(),
                    launchable: Launchable {
                        ctype: LaunchableType::DesktopId,
                        name: "app.desktop".to_string(),
                    },
                    url: None,
                    screenshots: Vec::new(),
                    provides: vec![Provides::Id("app.desktop".to_string())],
                },
            };

            appstream.write(&actual_input);

            let appimagetool_name = "gearlever_appimagetool_d3afa1.appimage";
            cmd::app(appimagetool_name)
                .unwrap()
                .arg(&actual_input)
                .run_outerr()
                .unwrap();
        }
    }

    // TODO: Doesn't work properly
    temp::clean_everything();
}
