//! Locate a KakaoTalk installation without depending on its localized app name.

use std::path::{Path, PathBuf};

const BUNDLE_ID: &str = "com.kakao.KakaoTalkMac";
const APP_NAMES: [&str; 2] = ["KakaoTalk.app", "카카오톡.app"];

pub struct InstalledApp {
    pub path: PathBuf,
    pub version: String,
    pub bundle_id: String,
}

fn find_in(roots: &[&Path]) -> Option<InstalledApp> {
    for root in roots {
        for name in APP_NAMES {
            let path = root.join(name);
            let plist = path.join("Contents/Info.plist");
            let Ok(dict) = plist::from_file::<_, plist::Dictionary>(&plist) else {
                continue;
            };
            if dict.get("CFBundleIdentifier").and_then(|v| v.as_string()) != Some(BUNDLE_ID) {
                continue;
            }
            return Some(InstalledApp {
                path,
                version: dict
                    .get("CFBundleShortVersionString")
                    .and_then(|v| v.as_string())
                    .unwrap_or("unknown")
                    .to_string(),
                bundle_id: BUNDLE_ID.to_string(),
            });
        }
    }
    None
}

pub fn installed_app() -> Option<InstalledApp> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    let paths: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    find_in(&paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_localized_bundle_and_rejects_other_app_identity() {
        let dir = tempfile::tempdir().unwrap();
        for (name, id) in [
            ("KakaoTalk.app", "example.impostor"),
            ("카카오톡.app", BUNDLE_ID),
        ] {
            let contents = dir.path().join(name).join("Contents");
            std::fs::create_dir_all(&contents).unwrap();
            let mut dict = plist::Dictionary::new();
            dict.insert("CFBundleIdentifier".into(), id.into());
            dict.insert("CFBundleShortVersionString".into(), "26.8.0".into());
            plist::to_file_xml(contents.join("Info.plist"), &dict).unwrap();
        }
        let app = find_in(&[dir.path()]).unwrap();
        assert_eq!(app.path, dir.path().join("카카오톡.app"));
        assert_eq!(app.version, "26.8.0");
    }
}
