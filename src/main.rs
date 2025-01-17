use std::{
    fs::{self, File}, io::Write, path::{Path, PathBuf}, process::Command, str::FromStr
};

use appstream::{
    AppStream, AppStreamComponent, ComponentType, ContentRating, Description, Launchable, LaunchableType, Provides, Screenshot, ScreenshotType, Screenshots, Url
};
use clap::Parser;
use cmd::RunExt;
use image::imageops::resize;
use itertools::Itertools;
use licensing::License;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml::to_writer;
use thiserror::Error;


mod appstream;
mod desktop_entry;
mod licensing;

const DEFAULT_ICON: &[u8; 530] = include_bytes!("../default-icon.svg");

#[derive(Parser, Debug)]
struct AppImageArgs {
    #[arg(short, long, default_value_t = false)]
    terminal: bool,

    #[arg(short, long, default_value = "Utility")]
    categories: Vec<String>,

    #[arg(short, long)]
    icon: Option<String>,

    target: String,
}

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
            && !["legal_details", "license", "readme", "apprun", ".diricon"].contains(&file_name_lower.as_str())
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
    use image::ImageReader;

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
        if common.exists() && common.read_dir().unwrap().next().is_none() {
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

            fn valid_icon(path: &Option<String>) -> Option<PathBuf> {
                if let Some(icon) = path {
                    let path = Path::new(icon).to_path_buf();
                    if path.exists() {
                        Some(path)
                    }
                    else { None}
                }
                else { None}
            }

            // Due to how the pkg2appimagetool works we NEED an icon, that's why it isn't an
            // option
            let icon = 
            if let Some(icon) = valid_icon(&args.icon) {
                fs::copy(icon, actual_input.join("AppIcon.png")).expect("Couldn't write AppIcon");
                "AppIcon".to_string()
            }
            else if actual_input.join("AppIcon.png").exists() || actual_input.join("AppIcon.svg").exists() {
                "AppIcon".to_string()
            } else if let Some(exe_name) = look_for_ext(&actual_input, "exe") {
                extract_icon_from_exe(&conf, &actual_input, exe_name.to_str().unwrap());
                "AppIcon".to_string()
            } else {
                    dialog::Message::new("No icon found, writing one")
                        .show()
                        .expect("Couldn't show message");
                    File::create(actual_input.join("AppIcon.svg")).expect("This should be possible").write(DEFAULT_ICON).expect("Failed to write icon");
                    "AppIcon".to_string()
            };

            let executable = if let Some(shell_file) = look_for_ext(&actual_input, "sh") {
                shell_file
            } else if let Some(linux_exe) = look_for_ext(&actual_input, "x86_64") {
                linux_exe
            } else {
                let mut exes = look_for_no_exts(&actual_input);
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

                    fn filename_len(path: &PathBuf) -> usize {
                        path.file_name().expect("Must have filename").to_string_lossy().len()
                    }

                    //Sort exes by length, usually the one we want is the one with the shortest name
                    exes.sort_by(|a,b |filename_len(a).cmp(&filename_len(b)));

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
                        println!("{}",name);
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

            let f_name = executable.file_name().expect("Executable must have a file name").to_string_lossy().to_string();
            let id = format!("{}.to_appimage.com", f_name);
            let desktop = format!("{}.desktop", id);
            let app_desktop = File::create(actual_input.join(&desktop)).unwrap();
            let whole_name = actual_input.file_name().expect("Input must have a file name");

            desktop_entry::to_writer(app_desktop, &entry).unwrap();
            std::fs::copy(&executable, actual_input.join("AppRun")).unwrap();

   
            // Make appstream
            // usr/share/metainfo/myapp.appdata.xml
            let summary = "TODO!TODO!".to_string();
            let description = "TODO!TODO!".to_string();
            const NAME_LIMIT: usize = 15;

            let appstream = AppStream {
                component: AppStreamComponent {
                    ctype: if args.terminal {
                        ComponentType::ConsoleApplication
                    } else {
                        ComponentType::DesktopApplication
                    },
                    id,
                    metadata_license: License::CC0,
                    project_license: License::locate(&actual_input).expect("Couldn't get the license"),
                    name: whole_name.to_string_lossy()[0..std::cmp::min(whole_name.len(), NAME_LIMIT)].to_string(),
                    summary,
                    description: Description{p: description},
                    launchable: Launchable {
                        ctype: LaunchableType::DesktopId,
                        name: desktop.clone()
                    },
                    url: Some(Url{ctype: appstream::UrlType::Homepage, data: "https://github.com/sheosi/to_appimage".to_string()}),
                    screenshots: Screenshots{screenshot: vec![Screenshot{ctype: ScreenshotType::Default, image: "https://placehold.co/700x400.png".to_string()}]},
                    provides: Provides{id: desktop.clone()},
                    content_rating: ContentRating {t: "oars-1.0".to_string()}, // This is for a program that is not +18
                },
            };

            appstream.write(&actual_input);

            let appimagetool_name = "gearlever_appimagetool_d3afa1.appimage";
            cmd::app(appimagetool_name)
                .unwrap()
                .arg(&actual_input)
                .arg("-n") // For the time being, ignore checking the appstram file, it appears the desktop file path is not correct, but don't know how to fix it
                .run_outerr()
                .unwrap();
        }
    }

    // TODO: Doesn't work properly
    temp::clean_everything();
}
