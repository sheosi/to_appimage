use std::{fs, path::Path};

use serde::Serialize;

use crate::licensing::License;

pub struct AppStream {
    pub component: AppStreamComponent,
}

#[derive(Serialize)]
#[serde(rename = "component")]
pub struct AppStreamComponent {
    #[serde(rename = "@type")]
    pub ctype: ComponentType,

    pub id: String,
    pub metadata_license: License,
    pub project_license:  License,
    pub name: String,
    pub summary: String,
    pub description: Description,
    pub launchable: Launchable,
    pub content_rating: ContentRating,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<Url>,

    #[serde(skip_serializing_if = "Screenshots::is_empty")]
    pub screenshots: Screenshots,

    pub provides: Provides,
}

#[derive(Serialize)]
pub struct Screenshots {
    pub screenshot: Vec<Screenshot>
}

impl Screenshots {
    pub fn is_empty(&self) -> bool {
        self.screenshot.is_empty()
    }
}

#[derive(Serialize)]
pub struct Description {
    pub p: String
}


#[derive(Serialize)]
pub enum ComponentType {
    #[serde(rename = "desktop-application")]
    DesktopApplication,

    #[serde(rename = "console-application")]
    ConsoleApplication,
}

#[derive(Serialize)]
pub struct Launchable {
    #[serde(rename = "@type")]
    pub ctype: LaunchableType,

    #[serde(rename = "$text")]
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

    #[serde(rename = "$text")]
    pub data: String,
}

#[derive(Serialize)]
pub enum UrlType {
    #[serde(rename = "homepage")]
    Homepage,
}

#[derive(Serialize)]
#[serde(rename="screenshot")]
pub struct Screenshot {
    #[serde(rename = "@type")]
    pub ctype: ScreenshotType,

    pub image: String,
}

#[derive(Serialize)]
pub enum ScreenshotType {
    #[serde(rename = "default")]
    Default,
}

#[derive(Serialize)]
pub struct Provides {
    pub id: String,
}

#[derive(Serialize)]
pub struct ContentRating {
    #[serde(rename="@type")]
    pub t: String
}

impl AppStream {
    pub fn write(&self, base_path: &Path) {
        let appstream_path = base_path.join("usr").join("share").join("metainfo");
        if !appstream_path.exists() {
            fs::create_dir_all(&appstream_path).unwrap();
        }

        fs::write(
            appstream_path.join(format!("{}.appdata.xml", self.component.id)),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_string() + &quick_xml::se::to_string(&self.component).unwrap()
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a() {
        assert_eq!("a", "a")
    }
}
