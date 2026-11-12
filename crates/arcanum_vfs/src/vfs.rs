use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::archive::{ArcanumArchive, normalize};

enum Layer {
    Dir(PathBuf),
    Archive(ArcanumArchive),
}

/// Layered read-only file system. Layers are searched in the order they were
/// mounted; the first layer that has a path wins.
///
/// Paths are case-insensitive and accept `/` or `\`. Paths containing `..` or
/// anything that could leave a mounted directory are treated as not found.
#[derive(Default)]
pub struct Vfs {
    layers: Vec<Layer>,
}

/// Split a path into safe components, or `None` if it could escape its root.
fn components(path: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for part in path.replace('\\', "/").split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            p if p.contains([':', '\0']) => return None,
            p => out.push(p.to_string()),
        }
    }
    Some(out)
}

/// Resolve `parts` under `root`, matching each component case-insensitively.
fn resolve(root: &Path, parts: &[String]) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in parts {
        let exact = cur.join(part);
        if exact.exists() {
            cur = exact;
            continue;
        }
        let found = fs::read_dir(&cur)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        cur = found.path();
    }
    Some(cur)
}

fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, path.to_string())
}

impl Vfs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mount a loose-file directory (e.g. an override or mod folder).
    pub fn mount_dir(&mut self, path: impl Into<PathBuf>) {
        self.layers.push(Layer::Dir(path.into()));
    }

    /// Mount a `.dat` archive.
    pub fn mount_archive(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        self.layers.push(Layer::Archive(ArcanumArchive::open(path)?));
        Ok(())
    }

    /// Mount the usual stack: `override_dir` first (if given), then every `.dat`
    /// in `game_dir`, highest file name first. Later patch archives
    /// (`Arcanum4.dat`) therefore shadow earlier ones (`arcanum1.dat`).
    pub fn from_game_dir(game_dir: impl AsRef<Path>, override_dir: Option<&Path>) -> io::Result<Self> {
        let mut vfs = Self::new();
        if let Some(dir) = override_dir {
            vfs.mount_dir(dir);
        }
        let mut dats: Vec<PathBuf> = fs::read_dir(game_dir)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dat")))
            .collect();
        dats.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
        dats.reverse();
        for dat in dats {
            vfs.mount_archive(dat)?;
        }
        Ok(vfs)
    }

    /// Read a whole file from the first layer that has it.
    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let parts = components(path).ok_or_else(|| not_found(path))?;
        for layer in &self.layers {
            match layer {
                Layer::Dir(root) => {
                    if let Some(p) = resolve(root, &parts).filter(|p| p.is_file()) {
                        return fs::read(p);
                    }
                }
                Layer::Archive(a) => {
                    if let Some(e) = a.find(&parts.join("/")).filter(|e| !e.is_dir()) {
                        return a.read_entry(e);
                    }
                }
            }
        }
        Err(not_found(path))
    }

    pub fn exists(&self, path: &str) -> bool {
        let Some(parts) = components(path) else { return false };
        self.layers.iter().any(|l| match l {
            Layer::Dir(root) => resolve(root, &parts).is_some_and(|p| p.is_file()),
            Layer::Archive(a) => a.find(&parts.join("/")).is_some_and(|e| !e.is_dir()),
        })
    }

    pub fn is_dir(&self, path: &str) -> bool {
        let Some(parts) = components(path) else { return false };
        self.layers.iter().any(|l| match l {
            Layer::Dir(root) => resolve(root, &parts).is_some_and(|p| p.is_dir()),
            Layer::Archive(a) => a.is_dir(&parts.join("/")),
        })
    }

    /// Immediate children of a directory across all layers, as `/`-separated
    /// paths relative to the VFS root. Sorted and de-duplicated; the first layer's
    /// spelling wins. `None` if no layer has the directory.
    pub fn list_dir(&self, path: &str) -> Option<Vec<String>> {
        let parts = components(path)?;
        let dir = parts.join("/");
        let mut merged: BTreeMap<String, String> = BTreeMap::new();
        let mut found = false;
        for layer in &self.layers {
            match layer {
                Layer::Dir(root) => {
                    let Some(p) = resolve(root, &parts).filter(|p| p.is_dir()) else { continue };
                    found = true;
                    for e in fs::read_dir(p).into_iter().flatten().flatten() {
                        let name = e.file_name().to_string_lossy().into_owned();
                        let child = if dir.is_empty() { name } else { format!("{dir}/{name}") };
                        merged.entry(normalize(&child)).or_insert(child);
                    }
                }
                Layer::Archive(a) => {
                    let Some(children) = a.list_dir(&dir) else { continue };
                    found = true;
                    for child in children {
                        merged.entry(normalize(child)).or_insert_with(|| child.to_string());
                    }
                }
            }
        }
        found.then(|| merged.into_values().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/Arcanum")
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("arcanum_vfs_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn dir_layer_is_case_insensitive_and_confined() {
        let root = scratch("dir");
        fs::create_dir_all(root.join("Art/Interface")).unwrap();
        fs::write(root.join("Art/Interface/Foo.art"), b"hi").unwrap();
        fs::write(root.parent().unwrap().join("arcanum_vfs_secret.txt"), b"no").unwrap();

        let mut vfs = Vfs::new();
        vfs.mount_dir(&root);
        assert_eq!(vfs.read("art\\interface\\FOO.ART").unwrap(), b"hi");
        assert!(vfs.is_dir("ART"));
        let listed = vfs.list_dir("art/interface").unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].eq_ignore_ascii_case("art/interface/foo.art"));
        assert!(vfs.read("../arcanum_vfs_secret.txt").is_err());
        assert!(vfs.read("art/../../arcanum_vfs_secret.txt").is_err());
        assert!(!vfs.exists("/etc/passwd"));
        assert!(vfs.read("missing.art").is_err());
        let _ = fs::remove_file(root.parent().unwrap().join("arcanum_vfs_secret.txt"));
    }

    #[test]
    fn override_dir_shadows_archives() {
        let game = game_dir();
        if !game.join("arcanum1.dat").exists() {
            return;
        }
        let over = scratch("override");
        fs::create_dir_all(over.join("mes")).unwrap();
        fs::write(over.join("mes/MainMenu.mes"), b"{1}{Overridden}").unwrap();

        let vfs = Vfs::from_game_dir(&game, Some(&over)).unwrap();
        assert_eq!(vfs.read("mes/mainmenu.mes").unwrap(), b"{1}{Overridden}");

        let plain = Vfs::from_game_dir(&game, None).unwrap();
        assert!(plain.read("mes/mainmenu.mes").unwrap().len() > 1000);
    }

    #[test]
    fn archives_layer_with_patch_priority() {
        let game = game_dir();
        if !game.join("arcanum1.dat").exists() {
            return;
        }
        let vfs = Vfs::from_game_dir(&game, None).unwrap();
        // Present in both arcanum3.dat (13561 bytes) and the Arcanum4.dat patch (13560 bytes).
        assert_eq!(vfs.read("mes/item_effect.mes").unwrap().len(), 13560);
        // Only in arcanum1.dat.
        assert!(vfs.exists("art/interface/mainmenuback.art"));
        assert!(vfs.is_dir("art/interface"));
        assert!(vfs.list_dir("art/interface").unwrap().iter().any(|p| p.eq_ignore_ascii_case("art/interface/mainmenuback.art")));
        assert!(vfs.list_dir("no/such/dir").is_none());
    }
}
