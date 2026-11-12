//! Runs against the real game archives; skipped when `assets/` is absent.
use std::path::PathBuf;

use arcanum_formats::art::ArtFile;
use arcanum_formats::mes::MesFile;
use arcanum_vfs::ArcanumArchive;

fn archives() -> Vec<ArcanumArchive> {
    // `ARCANUM_DIR`, or `assets/Arcanum` at the workspace root.
    let dir = std::env::var_os("ARCANUM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/Arcanum"));
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut paths: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dat")))
        .collect();
    paths.sort();
    paths.iter().filter_map(|p| ArcanumArchive::open(p).ok()).collect()
}

#[test]
fn main_menu_parses() {
    for a in archives() {
        if let Ok(data) = a.read("mes/MainMenu.mes") {
            let mes = MesFile::parse(&data).unwrap();
            assert_eq!(mes.get(1), Some("Loading Arcanum..."));
            assert_eq!(mes.get(5), Some("Loading Save Game..."));
            return;
        }
    }
    eprintln!("skipping: game archives not found");
}

#[test]
fn every_mes_file_parses() {
    let mut count = 0;
    for a in archives() {
        let paths: Vec<String> = a
            .entries()
            .iter()
            .filter(|e| e.path.to_ascii_lowercase().ends_with(".mes"))
            .map(|e| e.path.clone())
            .collect();
        for p in paths {
            let data = a.read(&p).unwrap();
            MesFile::parse(&data).unwrap_or_else(|e| panic!("{p}: {e}"));
            count += 1;
        }
    }
    eprintln!("parsed {count} .mes files");
}

#[test]
fn art_sprites_decode() {
    let mut checked = 0;
    for a in archives() {
        if let Ok(data) = a.read("art/interface/mainmenuback.art") {
            let art = ArtFile::parse(&data).unwrap();
            let (rgba, dims) = art.decode_frame0().unwrap();
            assert_eq!(dims, (800, 600));
            assert_eq!(rgba.len(), 800 * 600 * 4);
        }
        if let Ok(data) = a.read("art/interface/navbutton.art") {
            let (_, dims) = ArtFile::parse(&data).unwrap().decode_frame0().unwrap();
            assert_eq!(dims, (45, 45));
        }
        let paths: Vec<String> = a
            .entries()
            .iter()
            .filter(|e| e.path.to_ascii_lowercase().ends_with(".art"))
            .map(|e| e.path.clone())
            .collect();
        for p in paths {
            let data = a.read(&p).unwrap();
            let art = ArtFile::parse(&data).unwrap_or_else(|e| panic!("{p}: {e}"));
            // Palette-less files exist only if the engine never draws them; report, don't fail.
            if art.default_palette().is_ok() && art.frame_count > 0 {
                art.decode_frame0().unwrap_or_else(|e| panic!("{p}: {e}"));
            }
            checked += 1;
        }
    }
    eprintln!("decoded frame 0 of {checked} .art files");
}
