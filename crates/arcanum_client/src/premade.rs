//! The original's pre-made characters, read from their prototypes (`proto/016087 - PC.pro` to
//! `016098`) plus the text and portrait tables they refer to.
//!
//! Prototype 016086 is the empty "create your own" template (named "None") and is skipped; the
//! New Game choice menu offers character creation separately.

use arcanum_formats::mes::MesFile;
use arcanum_formats::object::Object;
use arcanum_vfs::Vfs;
use bevy::prelude::*;

/// Folder of the loose prototypes (mounted from `<game>/data`).
pub const PROTO_DIR: &str = "proto";
/// Prototype numbers of the pre-made characters, in the original's order.
pub const FIRST_PREMADE: u32 = 16087;
pub const LAST_PREMADE: u32 = 16098;
/// Background stories (`pc_background_text` ids).
pub const STORIES: &str = "mes/gameback.mes";
/// Race names at 720 + race and gender names at 740 + gender, as the original picker uses
/// (Arcanum.exe 0x544aad, 0x544b82).
pub const MENU_TEXT: &str = "mes/MainMenu.mes";
pub const RACE_TEXT_BASE: i32 = 720;
pub const GENDER_TEXT_BASE: i32 = 740;
/// Portrait number -> file name stem.
pub const PORTRAITS: &str = "portrait/gameport.mes";

const STAT_LEVEL: usize = 17;
const STAT_AGE: usize = 25;
const STAT_GENDER: usize = 26;
const STAT_RACE: usize = 27;

/// One pre-made character, ready to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Premade {
    pub prototype: u32,
    pub name: String,
    /// Race and gender names from `mes/MainMenu.mes`.
    pub race: String,
    pub gender: String,
    pub age: i32,
    pub level: i32,
    /// `pc_background` (row in `Rules/backgrnd.mes` / 10).
    pub background: i32,
    /// The story from `mes/gameback.mes`: its first line is the character's name, then
    /// paragraphs.
    pub story: String,
    /// Large (128x128) portrait, e.g. `portrait/DWM2_b.bmp`.
    pub portrait: Option<String>,
}

/// All pre-made characters, in prototype order.
#[derive(Resource, Debug, Clone, Default)]
pub struct PremadeCharacters(pub Vec<Premade>);

/// Why the pre-made characters could not be read.
#[derive(Debug)]
pub struct LoadError(pub String);

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn mes(vfs: &Vfs, path: &str) -> Result<MesFile, LoadError> {
    let data = vfs.read(path).map_err(|e| LoadError(format!("{path}: {e}")))?;
    MesFile::parse(&data).map_err(|e| LoadError(format!("{path}: {e}")))
}

/// Reads every pre-made character. Characters whose prototype is missing or broken are skipped
/// with a warning; missing text tables are an error.
pub fn load(vfs: &Vfs) -> Result<Vec<Premade>, LoadError> {
    let stories = mes(vfs, STORIES)?;
    let names = mes(vfs, MENU_TEXT)?;
    let portraits = mes(vfs, PORTRAITS)?;
    let files = vfs.list_dir(PROTO_DIR).unwrap_or_default();
    let mut out = Vec::new();
    for number in FIRST_PREMADE..=LAST_PREMADE {
        let prefix = format!("{PROTO_DIR}/{number:06} - ");
        let Some(path) = files.iter().find(|f| f.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase())) else {
            warn!("pre-made character prototype {number} not found");
            continue;
        };
        let object = match vfs.read(path).map_err(|e| e.to_string()).and_then(|d| Object::parse_prototype(&d).map_err(|e| e.to_string())) {
            Ok(o) => o,
            Err(e) => {
                warn!("{path}: {e}");
                continue;
            }
        };
        let base = object.ints("critter_stat_base_idx").unwrap_or_default();
        let stat = |i: usize| base.get(i).copied().unwrap_or(0);
        let text = |file: &MesFile, id: i32| u32::try_from(id).ok().and_then(|id| file.get(id)).unwrap_or("").to_string();
        out.push(Premade {
            prototype: number,
            name: object.string("pc_player_name").unwrap_or("").to_string(),
            race: text(&names, RACE_TEXT_BASE + stat(STAT_RACE)),
            gender: text(&names, GENDER_TEXT_BASE + stat(STAT_GENDER)),
            age: stat(STAT_AGE),
            level: stat(STAT_LEVEL),
            background: object.int("pc_background").unwrap_or(0),
            story: text(&stories, object.int("pc_background_text").unwrap_or(-1)),
            portrait: object
                .int("critter_portrait")
                .and_then(|id| u32::try_from(id).ok())
                .and_then(|id| portraits.get(id))
                .map(|stem| format!("portrait/{stem}_b.bmp")),
        });
    }
    Ok(out)
}

/// Splits a background text into its title (the first line, shown in the picker's fourth bar)
/// and its body (shown in the large panel).
pub fn split_story(story: &str) -> (&str, &str) {
    let story = story.trim_start_matches(['\r', '\n']);
    match story.split_once('\n') {
        Some((head, body)) => (head.trim_end_matches('\r'), body.trim_start_matches(['\r', '\n'])),
        None => (story, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stories_split_into_heading_and_body() {
        assert_eq!(split_story("Lugard Bloodstone\r\n\r\n\tLugard is...\n\tMore."), ("Lugard Bloodstone", "\tLugard is...\n\tMore."));
        assert_eq!(split_story("Just a name"), ("Just a name", ""));
        assert_eq!(split_story(""), ("", ""));
    }
}
