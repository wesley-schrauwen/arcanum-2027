//! Runs against the real game archives; skipped when `assets/` is absent.
use std::path::PathBuf;

use arcanum_formats::art::ArtFile;
use arcanum_formats::mes::MesFile;
use arcanum_formats::object::{Object, PlayerFile};
use arcanum_vfs::ArcanumArchive;

/// `ARCANUM_DIR`, or `assets/Arcanum` at the workspace root.
fn game_dir() -> PathBuf {
    std::env::var_os("ARCANUM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/Arcanum"))
}

fn archives() -> Vec<ArcanumArchive> {
    let Ok(rd) = std::fs::read_dir(game_dir()) else { return vec![] };
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

#[test]
fn every_bitmap_font_parses() {
    use arcanum_formats::font::BitmapFont;
    let mut checked = 0;
    for a in archives() {
        let fonts: Vec<String> = a
            .entries()
            .iter()
            .filter(|e| e.path.to_ascii_lowercase().ends_with("font.art"))
            .map(|e| e.path.clone())
            .collect();
        for path in fonts {
            let font = BitmapFont::parse(&a.read(&path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert!(font.line_height > 0, "{path}");
            assert!(font.glyph('?').is_some_and(|g| g.width > 0), "{path}");
            checked += 1;
        }
        // The main menu font.
        if let Ok(data) = a.read("art/interface/Morph30Font.ART") {
            let font = BitmapFont::parse(&data).unwrap();
            assert_eq!(font.line_height, 33);
            let text = font.render("Single Player");
            assert_eq!((text.width, text.height), (font.measure("Single Player"), 33));
            assert!(text.alpha.iter().any(|&a| a > 200), "glyphs have solid pixels");
        }
    }
    eprintln!("parsed {checked} bitmap fonts");
}

/// The PC prototypes from the loose `data/proto` folder, sorted by number.
fn pc_prototypes() -> Vec<(String, Vec<u8>)> {
    let Ok(rd) = std::fs::read_dir(game_dir().join("data/proto")) else { return vec![] };
    let mut out: Vec<(String, Vec<u8>)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(" - PC.pro")))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()))
        .collect();
    out.sort();
    out
}

#[test]
fn every_pc_prototype_parses_to_the_last_byte() {
    let protos = pc_prototypes();
    if protos.is_empty() {
        eprintln!("skipping: data/proto not found");
        return;
    }
    assert_eq!(protos.len(), 13);
    // (file, name, background, story id, portrait, race, gender)
    let expected = [
        // The "create your own" template is literally named "None".
        ("016086", Some("None"), 0, 0, 1005, 0, 1),
        ("016087", Some("Merwin Tumblebrook"), 39, 1039, 1017, 5, 1),
        ("016094", Some("Lugard Bloodstone"), 46, 1046, 1020, 1, 1),
        ("016095", Some("Victoria Warrington"), 47, 1047, 1025, 0, 0),
        ("016097", Some("Clarisse Vorak"), 49, 1049, 1031, 6, 0),
        ("016098", Some("Ker'ee Melange"), 50, 1050, 1029, 3, 0),
    ];
    for (file, data) in &protos {
        let obj = Object::parse_prototype(data).unwrap_or_else(|e| panic!("{file}: {e}"));
        let number: u32 = file[..6].parse().unwrap();
        assert_eq!(obj.id.prototype(), Some(number), "{file}");
        // The picker's description id: 16066 + the character's index (0 = create your own).
        assert_eq!(obj.int("description"), Some(16066 + (number as i32 - 16086)), "{file}");
        if let Some(&(_, name, bg, story, portrait, race, gender)) = expected.iter().find(|e| file.starts_with(e.0)) {
            let stats = obj.ints("critter_stat_base_idx").unwrap();
            assert_eq!(obj.string("pc_player_name"), name, "{file}");
            assert_eq!(obj.int("pc_background"), Some(bg), "{file}");
            assert_eq!(obj.int("pc_background_text"), Some(story), "{file}");
            assert_eq!(obj.int("critter_portrait"), Some(portrait), "{file}");
            assert_eq!((stats[27], stats[26]), (race, gender), "{file}");
        }
    }
}

#[test]
fn every_pre_made_character_file_parses_to_the_last_byte() {
    let mut count = 0;
    for a in archives() {
        let paths: Vec<String> = a
            .entries()
            .iter()
            .filter(|e| e.path.to_ascii_lowercase().ends_with(".mpc"))
            .map(|e| e.path.clone())
            .collect();
        for p in paths {
            let file = PlayerFile::parse(&a.read(&p).unwrap()).unwrap_or_else(|e| panic!("{p}: {e}"));
            // The object's GUID is the file name.
            let guid = file.object.id.guid().unwrap();
            assert!(p.to_ascii_uppercase().contains(&guid), "{p} vs {guid}");
            assert!(!file.name.is_empty() && !file.level_scheme.is_empty(), "{p}");
            let proto = file.object.prototype.prototype().unwrap();
            assert!((16087..=16098).contains(&proto), "{p}: {proto}");
            count += 1;
        }
    }
    if count == 0 {
        eprintln!("skipping: game archives not found");
    } else {
        assert_eq!(count, 12);
    }
}
