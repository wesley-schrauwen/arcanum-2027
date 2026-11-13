# Project notes (handoff)

Working notes for whoever picks this up next, human or agent. Read this first, then `README.md`.
Last updated: 2026-11-13 (options screen).

## What this is

A reimplementation of *Arcanum: Of Steamworks and Magick Obscura* (Troika, 2001; originally C++)
in Rust on **Bevy 0.20**. The client reads the original game data from the player's install.
The finished game is planned to ship upscaled 4K art, resampled audio, updated animations and
increasingly original art. Players will still need a valid Steam copy. Code is MIT; game assets
are not.

## Current state

Working end to end: `cargo run -p arcanum_client` opens a window and shows the original main menu
at any supported resolution, with the original menu music. The five entries from `MainMenu.mes`
are buttons: hover brightens the text and plays the original hover sound; a click plays the click
sound and runs the action. Single Player, Options and Credits open placeholder screens (Esc
returns), Exit Game quits, MultiPlayer logs "not implemented". Options opens the original
options screen (see below).

| Area | State |
|---|---|
| `.dat` archives | Done. Reader, layered VFS, override folder. |
| `.mes` text | Done. Decoder + Bevy asset. |
| `.art` sprites | Frame decode done (raw + RLE, palettes, 8 rotations). No animation playback. |
| Bevy integration | Done. VFS is Bevy's default asset source; `.art` → `Image`, `.mes` → `MesAsset`. |
| Main menu | Interactive: buttons, hover, click, `AppState` transitions, placeholder screens. |
| Options screen | Original art and text; Game/Video/Sound tabs, arrows on every setting, commit on change. Resolution: arrows pick, Update applies, then a 15 s confirm/auto-revert. Settings are "dumb plugs": stored, not applied (except resolution/fullscreen), not saved. |
| Audio | Menu music (`sound/music/Arcanum.mp3`, loop, 50%) and hover/click sounds. |
| Fonts | Game bitmap fonts decoded and rendered (`BitmapText`). Menu uses `Morph30Font`/`morph15font`. Override TTF still wins if present; built-in font as last resort. |
| Resolutions | Done. 720p–8K, 4:3–2.4:1, `UiScale` from a 1080 baseline. |
| Tests | 153 tests, 127 need no game data (synthetic archives/art/fonts, headless Bevy). |
| Everything else | Not started: game screens, maps, critters, dialogue, scripts, saves. |

### Git state

`main` is current. On 2026-11-13 the stacked PRs #2–#7 were found to have merged only into their
base branches; PR #8 merged `test/review-and-headless-tests` into `main` to bring everything in.
Branch new work from `main`. When stacking PRs, merge bottom-up and retarget each PR to `main`
before merging it (or delete the merged base branch so GitHub retargets automatically).
The old `feat/*` branches are fully merged and can be deleted.

The GitHub repo was renamed to `wesley-schrauwen/open-arcanum`. The local `origin` still points at
`arcanum-2027` and works through GitHub's redirect.

## Layout and key files

```
Cargo.toml                       workspace (edition 2024); root src/main.rs is an unused stub
crates/arcanum_vfs/              game data access, no Bevy dependency
  src/archive.rs                 ArcanumArchive: one .dat file (format notes at top of file)
  src/vfs.rs                     Vfs: override dir + archives, case-insensitive, path-escape safe
  src/test_util.rs               DatBuilder: writes synthetic .dat files (feature "test-util")
crates/arcanum_formats/          file format decoders, no Bevy dependency
  src/mes.rs                     MesFile: {id}{text} message files
  src/cp1252.rs                  Windows-1252 decode/encode (shared by .mes and fonts)
  src/font.rs                    BitmapFont: glyph lookup, measure, render to a coverage mask
  src/art.rs                     ArtFile: sprites (format notes at top of file)
  src/test_util.rs               ArtBuilder, TestFrame, rle_encode, font_art (feature "test-util")
  tests/real_files.rs            parses every .mes/.art in the real archives (skips without data)
crates/arcanum_client/           the Bevy game
  src/main.rs                    app setup; DEBUG_ARCANUM_SCREENSHOT handling
  src/resource_pack.rs           ResourcePack (game dir, override, module) + VFS mount order + `paths::*`
  src/vfs_asset.rs               VfsAssetReader / VfsAssetPlugin: VFS as Bevy's asset source
  src/art_loader.rs              ArtAssetLoader: .art → Image (frame 0); ArtFramesLoader: all frames
  src/mes_loader.rs              MesAsset / MesAssetLoader
  src/settings.rs                DisplayResolution, GameSettings, DisplayPlugin, UiScale
  src/main_menu.rs               AppState, MenuAction, MainMenuPlugin: camera, background, buttons,
                                 hover highlight, Activate observer, menu_ready
  src/options.rs                 OptionsPlugin: Options screen, OptionId/OptionsTab model, resolution confirm
  src/widgets.rs                 WidgetsPlugin: HoverText (+Selected), FrameButton (art-frame buttons)
  src/screens.rs                 ScreensPlugin: placeholder screens (Single Player, Credits), Esc back
  src/audio.rs                   MenuAudioPlugin: menu music, MenuSounds (hover/click)
  src/fonts.rs                   FontsPlugin: ArcanumFonts, UiFont, insert_text (picks TTF/bitmap/default)
  src/bitmap_text.rs             BitmapTextPlugin: font asset + loader, BitmapText rasterise/tint/fallback
  tests/headless.rs              58 Bevy tests, no window/GPU/game data
  tests/vfs_assets.rs            real-data tests, incl. "every resource_pack path exists"
assets/                          symlink to the local game install; gitignored, never commit
plans/                           design plans for future work (combat_design.md: companion combat AI; companions.md: follower management)
```

## Running

```sh
cargo run -p arcanum_client                         # needs game data
cargo test --workspace                              # real-data tests skip without game data
```

| Env var | Purpose |
|---|---|
| `ARCANUM_DIR` | Game folder with `arcanum1.dat` etc. Default `assets/Arcanum`. Tests honour it too. |
| `ARCANUM_OVERRIDE` | Loose-file folder that shadows everything else. |
| `ARCANUM_MODULE` | Module to mount from `modules/`. Default `Arcanum`. |
| `ARCANUM_RES` | `WxH`, e.g. `3440x1440`. Invalid values fall back to 1920x1080. |
| `ARCANUM_FULLSCREEN` | `1`/`true` for borderless fullscreen. |
| `DEBUG_ARCANUM_SCREENSHOT` | Path; saves the window once the menu (or Options) is ready, then exits. Used to verify rendering. |
| `DEBUG_ARCANUM_SCREEN` | `options` opens straight onto the Options screen (for screenshots). |

## Format knowledge

Recovered by inspecting data and by reverse engineering `Arcanum.exe` with the REA MCP tools
(Ghidra). Full layouts are in the doc comments of `archive.rs` and `art.rs`.

- **VFS mount order** (`ResourcePack::open_vfs`, highest priority first): override folder,
  `modules/<module>.PATCH*` (highest number first), `modules/<module>/` (loose files: music,
  maps, saves), `modules/<module>.dat`, then every `.dat` in the game folder. Music is only in
  the module folder. `modules/Vormantown.dat` is a second module.
- **`.dat`**: index at the end of the file. The last 28 bytes are a footer: GUID, `1TAD`, unused u32,
  index+footer size. Entry flags: `0x1` raw, `0x2` zlib, `0x400` directory. Paths use `\`.
  Archives in the game folder: `arcanum1-3.dat`, `Arcanum4.dat` (patch), `tig.dat`. Higher
  numbers shadow lower ones; the VFS mounts them in descending name order.
- **`.mes`**: `{id}{text}` pairs, Windows-1252, `//` comments outside braces, multi-line text.
  Braces do not nest. Pairs are read strictly in order.
- **`.art`**: loader is `FUN_0051b710` (called from `FUN_0051b170`, near the "ART LOAD FAILURE"
  string at `0x5bec2c`). 0x84-byte header, up to 4 BGR palettes, per-rotation frame tables
  (28 bytes: w, h, size, center x/y, delta x/y), then pixels. Raw if size == w*h, else RLE
  (bit 7 set = literal run, clear = repeat). Flag bit 0 = static (1 rotation), otherwise 8.
- **Unverified assumptions**: palette index 0 = transparent (decoder sets alpha 0); frame 0 uses
  the first palette present (the game picks a palette per art id). Not yet checked against the
  engine's blit code.
- **Sound**: `sound/snd_*.mes` map sound ids to files; `sound/schemeindex.mes` and
  `schemelist.mes` define music schemes. "M.Main Menu" (#2600) = `music\Arcanum.mp3 /loop /vol:50`.
  Menu text sounds are ids 3026/3027 (`MorphText_HOVER.wav`/`MorphText_CLICK.wav`). WAVs are
  16-bit PCM; music is MP3 (the `bevy` dependency enables the `mp3` and `wav` features).
- **Fonts**: no TTFs in the game. 34 bitmap fonts: `art/interface/*Font.art` plus
  `art/morph15font.art` in `tig.dat`. Each is static `.art` with 225 frames: frame 0 = text
  cursor, frame n = Windows-1252 byte n+31 (frame 1 = space, last = 0xFF). All glyphs share the
  line height; pixel palette indices are a grey ramp = anti-aliasing coverage (alpha mask).
  Glyph width = advance (no kerning; e.g. `R` includes its tail). Fonts are listed in
  `art/interface/interface.mes` (Morph30Font = id 327, next to MainMenuBack = 329).
  The font data has no shading or outline. The original menus draw it in red with a dark
  drop shadow (from screenshots of the original Single Player menu: solid strokes about
  `rgb(96,28,20)`, hovered about `rgb(184,48,33)`). `BitmapText` adds a black shadow one font
  pixel down/right at 80% opacity. The original also looks slightly embossed (lighter stroke
  centres); that effect is not reproduced and its source in the engine is unknown.
  **Assumption**: the main menu uses `Morph30Font`. The menu code (`FUN_00546340`) draws with font
  handles at `0x64c210`/`0x64c218`/`0x64c0cc`/`0x64c228`, chosen by flags; where those are created
  was not traced.
- **Options screen** (original): full-screen `OptionsMenuBack.ART` (800x600): title plate,
  tab box (Game/Video/Sound/Done = `MainMenu.mes` 4000-4003), 2x4 grid of 213x56 slots
  (`OptionSlider.ART` tube, `OptionMultiChoice.ART` value window), arrow buttons
  `SldrButt_L/R_Arrow.ART` (frames: 0 normal, 1 pressed, 2 hover). Setting names in
  `mes/options_ui.mes`; values in `OptionsOffOn/OptionsWalk/OptionsFloats.mes`
  (`OptionsDifficulty`/`OptionsCombatMode` exist but are unused by design). The original had
  no Cancel: changes apply live. Layout constants are in `options.rs` (original pixels).
- Not yet investigated: the engine's 5-rotation mirrored-sprite optimisation, art id → file name
  mapping (`interface.mes`, `item_inven.mes` etc. look like the indexes), fonts
  (`art/morph15font.art` and similar), sound formats, `.dlg`, `scr/` scripts, maps (`terrain/`,
  `modules/`), save games.

## Conventions and decisions

- Asset paths live in `resource_pack::paths`, grouped by kind (`backgrounds`, `interface`,
  `portraits`, `textures`, `items`, `critters`, `scenery`, `text`, `sound`). Add new ones there,
  plus to `ALL_FILES`/`ALL_DIRS` so the real-data test checks they exist.
- **Owner's design decisions (Options):** turn-based combat only (no combat mode setting); a
  single difficulty (no difficulty setting); no Module, Violence Filter, Follower Skills,
  Auto Switch Weapons, Always Run or Control Followers settings; every change commits immediately,
  no Cancel. Resolution is the exception: its arrows only pick a pending value; Done (or Esc)
  applies it and asks "Confirm Resolution" before returning to the menu. Keep keeps it; Revert, Esc
  or 15 s without an answer restores the previous one; either way the menu follows. The rest
  of the screen is locked while the question is up. Settings are dumb plugs for now;
  English only (a future text manager may hot-swap strings). Tabs: Game = Auto Attack, Combat
  Taunts; Video = Brightness, Text Duration, Floats, Float Speed, Fullscreen, Resolution; Sound = Effects, Voice, Music.
- UI and camera are authored against a **1080-unit-tall baseline**. The camera is
  `FixedVertical { viewport_height: 1080 }`, and `UiScale` = window logical height / 1080.
  Original 800x600 art is scaled ×1.8 and pillarboxed.
- `WindowResolution::new` takes physical pixels in Bevy 0.20, so `UiScale` uses the logical
  height (correct on Retina).
- Bevy 0.20 APIs differ from older docs and examples (`Camera2d`, `Sprite`, `Text`/`Node`, Parley
  text, `MessageWriter`). Check `~/.cargo/registry/src/*/bevy_*-0.20.1` when unsure.
- Tests: prefer synthetic fixtures (`DatBuilder`, `ArtBuilder`, `tempfile`) and headless Bevy apps.
  Real-data tests must skip cleanly when the game data is absent.
- The VFS reads synchronously inside the async `AssetReader`. Acceptable for now.
- Bevy 0.20 has no `Interaction` component. Buttons are the headless `bevy::ui_widgets::Button`;
  `ActivateOnPress` makes a press send `Activate` (handled by an observer), and
  `bevy::picking::hover::Hovered` tracks hover. Tests insert `Hovered(true)` and trigger
  `Activate` directly.
- Screens are `AppState` states; their UI roots carry `DespawnOnExit(state)`. The camera,
  background and music persist across menu states.
- Headless tests need `StatesPlugin`, `init_asset::<Font>()` and `init_asset::<AudioSource>()`
  (normally provided by `DefaultPlugins`); see `menu_app` in `tests/headless.rs`.

## Known issues / backlog (not started)

- Sprites use linear filtering, so scaled 800x600 art looks soft. Consider nearest or a scaler.
- Windows narrower than 4:3 crop the menu art (they are rejected by resolution validation anyway).
- Label positions in `main_menu.rs` are hand-tuned constants for the original art.
- `BitmapText` is one line only: no wrapping, no alignment options, no `\n`. Characters
  outside Windows-1252 render as `?`.
- Bitmap text is upscaled with linear filtering (×1.8 at 1080p), so it is slightly soft.
- Options sliders only step with the arrows (no click or drag on the tube). The bottom help box
  and the round lens (`OptionsPCLens.art`) on the options art are unused.
- `.art` loaders register `art`, `ART` and `Art`: Bevy's extension match is case-sensitive.
- Hover and click are tested headlessly by inserting `Hovered` and triggering `Activate`. The
  real pointer path is only checked manually: a screenshot run with the mouse over the window
  showed the entry under the pointer highlighted.
- No keyboard navigation of the menu, and no hotkeys from `MainMenu.mes`.
- `GameSettings` is not persisted (by design for now; env vars only).
- Steam install folder is not auto-detected; players must set `ARCANUM_DIR`.

## Suggested next steps

1. Make settings take effect: music/effects volume on the audio, brightness on the camera.
   Then persist `GameSettings` (owner deferred this).
2. Single Player menu: the original's second level (`MainMenu.mes` 50 New Game, 51 Load Game, ...)
   in place of the placeholder screen, then menu keyboard hotkeys.
3. Multi-line `BitmapText` (wrapping, alignment) for longer UI text such as credits.
4. Art id → path mapping via the `.mes` index files, so assets can be loaded by game id.
5. `.art` animation playback (frame rate, rotations, center/delta offsets) as a Bevy component.
6. Replacement-art layer: a PNG next to an asset path (e.g. `mainmenuback.png`) overrides the
   `.art`, so new art and upscales can drop in. Plus a coverage report of replaced assets.
7. Steam install auto-detection (libraryfolders.vdf) as the default `ARCANUM_DIR`.
8. Later: maps/tiles, critters, dialogue, scripts, saves.

## Art pipeline (outside the repo)

The owner is producing new and upscaled art in ComfyUI (`~/ComfyUI`, M3 Max, 48 GB).
`4x-UltraSharp` is linked into `~/ComfyUI/models/upscale_models`. Planned workflow: SDXL with the
original menu background as a low-strength depth ControlNet reference, then Ultimate SD Upscale
and a 4x-UltraSharp pass to 3840x2160. The extracted original background is at
`assets/extracted/mainmenuback.png` (outside git).

## Working with the owner

- Short, factual responses. No filler.
- Commit and open PRs only when asked. New work goes on a new branch; PRs have been stacked.
- Commit messages end with the `Co-Authored-By` line the harness specifies; PR bodies end with
  the Claude Code footer.
- PR review comments are left on GitHub; check all open PRs when asked to address them.
