# Project notes (handoff)

Working notes for whoever picks this up next, human or agent. Read this first, then `README.md`.
Last updated: 2026-11-13.

## What this is

A reimplementation of *Arcanum: Of Steamworks and Magick Obscura* (Troika, 2001; originally C++)
in Rust on **Bevy 0.20**. The client reads the original game data from the player's install.
The finished game is planned to ship upscaled 4K art, resampled audio, updated animations and
increasingly original art. Players will still need a valid Steam copy. Code is MIT; game assets
are not.

## Current state

Working end to end: `cargo run -p arcanum_client` opens a window and shows the original main menu
(background art + 5 labels from `MainMenu.mes`) at any supported resolution. Nothing is
interactive yet: no buttons, hover, input or game screens.

| Area | State |
|---|---|
| `.dat` archives | Done. Reader, layered VFS, override folder. |
| `.mes` text | Done. Decoder + Bevy asset. |
| `.art` sprites | Frame decode done (raw + RLE, palettes, 8 rotations). No animation playback. |
| Bevy integration | Done. VFS is Bevy's default asset source; `.art` → `Image`, `.mes` → `MesAsset`. |
| Main menu | Static render only. |
| Resolutions | Done. 720p–8K, 4:3–2.4:1, `UiScale` from a 1080 baseline. |
| Tests | 94 tests, 70 need no game data (synthetic archives/art, headless Bevy). |
| Everything else | Not started: fonts, input, UI widgets, maps, critters, sound, scripts, saves. |

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
  src/art.rs                     ArtFile: sprites (format notes at top of file)
  src/test_util.rs               ArtBuilder, TestFrame, rle_encode (feature "test-util")
  tests/real_files.rs            parses every .mes/.art in the real archives (skips without data)
crates/arcanum_client/           the Bevy game
  src/main.rs                    app setup; DEBUG_ARCANUM_SCREENSHOT handling
  src/resource_pack.rs           ResourcePack (game dir, override dir) + `paths::*` asset paths
  src/vfs_asset.rs               VfsAssetReader / VfsAssetPlugin: VFS as Bevy's asset source
  src/art_loader.rs              ArtAssetLoader: .art → Image (frame 0)
  src/mes_loader.rs              MesAsset / MesAssetLoader
  src/settings.rs                DisplayResolution, GameSettings, DisplayPlugin, UiScale
  src/main_menu.rs               MainMenuPlugin: camera, background, label column, menu_ready
  tests/headless.rs              22 Bevy tests, no window/GPU/game data
  tests/vfs_assets.rs            real-data tests, incl. "every resource_pack path exists"
assets/                          symlink to the local game install; gitignored, never commit
```

## Running

```sh
cargo run -p arcanum_client                         # needs game data
cargo test --workspace                              # real-data tests skip without game data
```

| Env var | Purpose |
|---|---|
| `ARCANUM_DIR` | Game folder with `arcanum1.dat` etc. Default `assets/Arcanum`. Tests honour it too. |
| `ARCANUM_OVERRIDE` | Loose-file folder that shadows the archives. |
| `ARCANUM_RES` | `WxH`, e.g. `3440x1440`. Invalid values fall back to 1920x1080. |
| `ARCANUM_FULLSCREEN` | `1`/`true` for borderless fullscreen. |
| `DEBUG_ARCANUM_SCREENSHOT` | Path; saves the window once the menu is ready, then exits. Used to verify rendering. |

## Format knowledge

Recovered by inspecting data and by reverse engineering `Arcanum.exe` with the REA MCP tools
(Ghidra). Full layouts are in the doc comments of `archive.rs` and `art.rs`.

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
- Not yet investigated: the engine's 5-rotation mirrored-sprite optimisation, art id → file name
  mapping (`interface.mes`, `item_inven.mes` etc. look like the indexes), fonts
  (`art/morph15font.art` and similar), sound formats, `.dlg`, `scr/` scripts, maps (`terrain/`,
  `modules/`), save games.

## Conventions and decisions

- Asset paths live in `resource_pack::paths`, grouped by kind (`backgrounds`, `interface`,
  `portraits`, `textures`, `items`, `critters`, `scenery`, `text`, `sound`). Add new ones there,
  plus to `ALL_FILES`/`ALL_DIRS` so the real-data test checks they exist.
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

## Known issues / backlog (not started)

- Sprites use linear filtering, so scaled 800x600 art looks soft. Consider nearest or a scaler.
- Windows narrower than 4:3 crop the menu art (they are rejected by resolution validation anyway).
- Label positions in `main_menu.rs` are hand-tuned constants for the original art.
- Labels use Bevy's built-in font, not Arcanum's bitmap fonts.
- `GameSettings` is not persisted (by design for now; env vars only).
- Steam install folder is not auto-detected; players must set `ARCANUM_DIR`.

## Suggested next steps

1. Main menu interactivity: buttons with hover/press states (likely `art/interface/*button*.art`
   frames), click handling, Exit Game working, keyboard hotkeys (`MainMenu.mes` has hotkey
   entries after each menu).
2. Bitmap fonts: decode Arcanum's font `.art` files and render labels with them.
3. Art id → path mapping via the `.mes` index files, so assets can be loaded by game id.
4. `.art` animation playback (frame rate, rotations, center/delta offsets) as a Bevy component.
5. Replacement-art layer: a PNG next to an asset path (e.g. `mainmenuback.png`) overrides the
   `.art`, so new art and upscales can drop in. Plus a coverage report of replaced assets.
6. Steam install auto-detection (libraryfolders.vdf) as the default `ARCANUM_DIR`.
7. Later: maps/tiles, critters, sound, dialogue, scripts.

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
