use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
};

pub const INTERVALS: [u64; 4] = [5, 15, 30, 60];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub refresh_seconds: u64,
    pub cell_order: [usize; 3],
    pub yellow_percent: u8,
    pub red_percent: u8,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            refresh_seconds: 30,
            cell_order: [0, 1, 2],
            yellow_percent: 20,
            red_percent: 10,
        }
    }
}
fn path() -> io::Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is unavailable")
        })?,
    )
    .join("CellTray")
    .join("settings.json"))
}
fn load_at(path: &Path) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
        .map(|mut s| {
            if !valid_order(s.cell_order) {
                s.cell_order = [0, 1, 2];
            }
            if !valid_thresholds(s.yellow_percent, s.red_percent) {
                s.yellow_percent = 20;
                s.red_percent = 10;
            }
            s
        })
        .filter(|s| INTERVALS.contains(&s.refresh_seconds))
        .unwrap_or_default()
}
pub fn load() -> Settings {
    path().map(|p| load_at(&p)).unwrap_or_default()
}
fn save_at(path: &Path, settings: &Settings) -> io::Result<()> {
    if !INTERVALS.contains(&settings.refresh_seconds)
        || !valid_order(settings.cell_order)
        || !valid_thresholds(settings.yellow_percent, settings.red_percent)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid settings: red threshold must be lower than yellow",
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(settings).map_err(io::Error::other)?,
    )
}
pub fn save(settings: &Settings) -> io::Result<()> {
    save_at(&path()?, settings)
}
pub fn valid_thresholds(yellow: u8, red: u8) -> bool {
    red > 0 && red < yellow && yellow <= 100
}
fn valid_order(mut order: [usize; 3]) -> bool {
    order.sort();
    order == [0, 1, 2]
}
pub fn reorder(mut order: [usize; 3], from: usize, to: usize) -> [usize; 3] {
    if from >= 3 || to >= 3 {
        return order;
    }
    let item = order[from];
    if from < to {
        order[from..=to].rotate_left(1);
    } else {
        order[to..=from].rotate_right(1);
    }
    order[to] = item;
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistence_and_invalid_file_fallback() {
        let dir = std::env::temp_dir().join(format!(
            "celltray-settings-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = dir.join("settings.json");
        assert_eq!(load_at(&path), Settings::default());
        let settings = Settings {
            refresh_seconds: 15,
            cell_order: [2, 0, 1],
            yellow_percent: 30,
            red_percent: 15,
        };
        save_at(&path, &settings).unwrap();
        assert_eq!(load_at(&path), settings);
        std::fs::write(&path, b"{\"refresh_seconds\":0}").unwrap();
        assert_eq!(load_at(&path), Settings::default());
        std::fs::write(&path, b"invalid").unwrap();
        assert_eq!(load_at(&path), Settings::default());
        assert!(
            save_at(
                &path,
                &Settings {
                    refresh_seconds: 1,
                    ..Settings::default()
                }
            )
            .is_err()
        );
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn drag_insertion_preserves_all_cells() {
        assert_eq!(reorder([0, 1, 2], 0, 2), [1, 2, 0]);
        assert_eq!(reorder([0, 1, 2], 2, 0), [2, 0, 1]);
        assert_eq!(reorder([2, 0, 1], 1, 1), [2, 0, 1]);
        assert!(!valid_order([0, 0, 2]));
        assert!(!valid_order([0, 1, 3]));
        let legacy: Settings = serde_json::from_str(r#"{"refresh_seconds":5}"#).unwrap();
        assert_eq!(legacy.cell_order, [0, 1, 2]);
    }
}
