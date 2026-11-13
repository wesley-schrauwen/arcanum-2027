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
///
/// Names come from the directory listing, never from `parts`, so the result has the on-disk
/// spelling even on case-insensitive filesystems. An exact match beats a case-insensitive one.
fn resolve(root: &Path, parts: &[String]) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in parts {
        let mut fallback = None;
        let mut exact = None;
        for entry in fs::read_dir(&cur).ok()?.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == part.as_str() {
                exact = Some(entry.path());
                break;
            }
            if fallback.is_none() && name.eq_ignore_ascii_case(part) {
                fallback = Some(entry.path());
            }
        }
        cur = exact.or(fallback)?;
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
                    // Use the on-disk spelling of the directory, not the caller's.
                    let on_disk = p
                        .strip_prefix(root)
                        .map(|r| r.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
                        .unwrap_or_else(|_| dir.clone());
                    for e in fs::read_dir(&p).into_iter().flatten().flatten() {
                        let name = e.file_name().to_string_lossy().into_owned();
                        let child = if on_disk.is_empty() { name } else { format!("{on_disk}/{name}") };
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
    use crate::test_util::DatBuilder;

    fn write(root: &Path, rel: &str, data: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    }

    #[test]
    fn dir_layer_is_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "Art/Interface/Foo.art", b"hi");
        let mut vfs = Vfs::new();
        vfs.mount_dir(tmp.path());

        assert_eq!(vfs.read("art\\interface\\FOO.ART").unwrap(), b"hi");
        assert!(vfs.exists("ART/INTERFACE/foo.art"));
        assert!(vfs.is_dir("art"));
        assert!(vfs.is_dir(""));
        assert!(!vfs.exists("art")); // directories are not files
        assert_eq!(vfs.list_dir("art/interface").unwrap(), vec!["Art/Interface/Foo.art"]);
    }

    #[test]
    fn paths_cannot_escape_a_mounted_dir() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("root");
        write(&root, "inside.txt", b"ok");
        write(outer.path(), "secret.txt", b"no");
        let mut vfs = Vfs::new();
        vfs.mount_dir(&root);

        assert_eq!(vfs.read("inside.txt").unwrap(), b"ok");
        for bad in ["../secret.txt", "..\\secret.txt", "a/../../secret.txt", "./../secret.txt", "C:/secret.txt", "x\0y"] {
            assert_eq!(vfs.read(bad).unwrap_err().kind(), io::ErrorKind::NotFound, "{bad:?}");
            assert!(!vfs.exists(bad));
            assert!(!vfs.is_dir(bad));
            assert!(vfs.list_dir(bad).is_none());
        }
        // Leading separators are relative to the mount, not the filesystem root.
        assert_eq!(vfs.read("/inside.txt").unwrap(), b"ok");
        assert!(!vfs.exists("/etc/passwd"));
    }

    #[test]
    fn first_mounted_layer_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("first.dat");
        let second = tmp.path().join("second.dat");
        DatBuilder::new().raw("a.txt", b"first").write(&first);
        DatBuilder::new().raw("A.TXT", b"second").raw("b.txt", b"only second").write(&second);
        let over = tmp.path().join("override");
        write(&over, "b.txt", b"override");

        let mut vfs = Vfs::new();
        vfs.mount_dir(&over);
        vfs.mount_archive(&first).unwrap();
        vfs.mount_archive(&second).unwrap();
        assert_eq!(vfs.read("a.txt").unwrap(), b"first");
        assert_eq!(vfs.read("b.txt").unwrap(), b"override");
        assert!(vfs.read("c.txt").is_err());
    }

    #[test]
    fn archive_directory_entries_are_not_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dat = tmp.path().join("a.dat");
        DatBuilder::new().dir("art").raw("art/x.art", b"x").write(&dat);
        let mut vfs = Vfs::new();
        vfs.mount_archive(&dat).unwrap();
        assert_eq!(vfs.read("art").unwrap_err().kind(), io::ErrorKind::NotFound);
        assert!(!vfs.exists("art"));
        assert!(vfs.is_dir("art"));
    }

    #[test]
    fn list_dir_merges_layers() {
        let tmp = tempfile::tempdir().unwrap();
        let dat = tmp.path().join("a.dat");
        DatBuilder::new().raw("mes/A.mes", b"").raw("mes/b.mes", b"").raw("other/c", b"").write(&dat);
        let over = tmp.path().join("override");
        write(&over, "mes/a.MES", b"");
        write(&over, "mes/new.mes", b"");

        let mut vfs = Vfs::new();
        vfs.mount_dir(&over);
        vfs.mount_archive(&dat).unwrap();

        // Sorted by normalised path, de-duplicated, stored spelling kept (first layer wins).
        assert_eq!(vfs.list_dir("MES").unwrap(), vec!["mes/a.MES", "mes/b.mes", "mes/new.mes"]);
        assert_eq!(vfs.list_dir("").unwrap(), vec!["mes", "other"]);
        assert_eq!(vfs.list_dir("other").unwrap(), vec!["other/c"]);
        assert!(vfs.list_dir("missing").is_none());
    }

    #[test]
    fn from_game_dir_orders_patches_first() {
        let tmp = tempfile::tempdir().unwrap();
        DatBuilder::new().raw("x.mes", b"base").raw("base_only", b"1").write(tmp.path().join("arcanum1.dat"));
        DatBuilder::new().raw("x.mes", b"mid").write(tmp.path().join("arcanum3.dat"));
        DatBuilder::new().raw("X.MES", b"patch").write(tmp.path().join("Arcanum4.dat"));
        fs::write(tmp.path().join("readme.txt"), b"not an archive").unwrap();

        let vfs = Vfs::from_game_dir(tmp.path(), None).unwrap();
        assert_eq!(vfs.read("x.mes").unwrap(), b"patch");
        assert_eq!(vfs.read("base_only").unwrap(), b"1");
        assert!(!vfs.exists("readme.txt"));

        let over = tmp.path().join("override");
        write(&over, "x.mes", b"override");
        let vfs = Vfs::from_game_dir(tmp.path(), Some(&over)).unwrap();
        assert_eq!(vfs.read("x.mes").unwrap(), b"override");
    }

    #[test]
    fn from_game_dir_reports_bad_archives() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("broken.dat"), [0u8; 64]).unwrap();
        assert!(Vfs::from_game_dir(tmp.path(), None).is_err());
        assert!(Vfs::from_game_dir(tmp.path().join("missing"), None).is_err());
    }

    /// `ARCANUM_DIR`, or `assets/Arcanum` at the workspace root.
    fn game_dir() -> PathBuf {
        std::env::var_os("ARCANUM_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/Arcanum"))
    }

    #[test]
    fn real_game_dir_patch_priority() {
        let game = game_dir();
        if !game.join("arcanum1.dat").exists() {
            return;
        }
        let vfs = Vfs::from_game_dir(&game, None).unwrap();
        // In both arcanum3.dat (13561 bytes) and the Arcanum4.dat patch (13560 bytes).
        assert_eq!(vfs.read("mes/item_effect.mes").unwrap().len(), 13560);
        assert!(vfs.exists("art/interface/mainmenuback.art"));
    }
}
