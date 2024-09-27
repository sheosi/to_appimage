use std::{fs, path::Path};

use serde::Serialize;

#[derive(Serialize)]
pub struct AppStream {
    pub component: AppStreamComponent,
}

#[derive(Serialize)]
pub struct AppStreamComponent {
    #[serde(rename = "@type")]
    pub ctype: ComponentType,

    pub id: String,
    pub metadata_license: String,
    pub project_license: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    pub launchable: Launchable,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<Url>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<Screenshot>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<Provides>,
}

#[derive(Serialize)]
pub enum ComponentType {
    #[serde(rename = "desktop-applicatiom")]
    DesktopApplication,

    #[serde(rename = "console-application")]
    ConsoleApplication,
}

#[derive(Serialize)]
pub struct Launchable {
    #[serde(rename = "@type")]
    pub ctype: LaunchableType,

    pub name: String,
}

#[derive(Serialize)]
pub enum LaunchableType {
    #[serde(rename = "desktop-id")]
    DesktopId,
}

#[derive(Serialize)]
pub struct Url {
    #[serde(rename = "@type")]
    pub ctype: UrlType,

    pub data: String,
}

#[derive(Serialize)]
pub enum UrlType {
    #[serde(rename = "homepage")]
    Homepage,
}

#[derive(Serialize)]
pub struct Screenshot {
    #[serde(rename = "@type")]
    pub ctype: ScreenshotType,

    pub image: Image,
}

#[derive(Serialize)]
pub enum ScreenshotType {
    #[serde(rename = "default")]
    Default,
}

#[derive(Serialize)]
pub struct Image {
    pub data: String,
}

#[derive(Serialize)]
pub enum Provides {
    #[serde(rename = "id")]
    Id(String),
}

impl AppStream {
    pub fn write(&self, base_path: &Path) {
        let appstream_path = base_path.join("usr").join("share").join("metainfo");
        if !appstream_path.exists() {
            fs::create_dir_all(&appstream_path).unwrap();
        }

        fs::write(
            appstream_path.join(format!("{}.appdata.xml", self.component.id)),
            quick_xml::se::to_string(self).unwrap().as_bytes(),
        )
        .unwrap();
    }
}
