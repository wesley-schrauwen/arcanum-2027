# World milestone: networked world from a save

Status: design approved 2026-11-13, not implemented.

## Intent

The game is multiplayer first. After the player picks or creates a character, the "The world goes
here" placeholder (`screens.rs`, `AppState::InGame`) is replaced by a real world. The PC stands on
the original starting map and walks where the player clicks.

The world is built on a host/client split from the start, with a hybrid trust model (below): the
client acts at once on its own PC and the host validates, corrects and owns everything shared.
Single player is the same game with the host and client in one process over a loopback transport.

Why this order: adding host authority to a single-player game later means rewriting every system.
Building the netcode before the world exists means guessing at messages that real state would
define. So the first world state goes through the split, and nothing is networked before there is
state to network.

## Goals

- **Milestone A:** the PC on the starting map, with its static scenery (walls, buildings, trees)
  drawn depth-sorted and blocking movement. The PC is animated and moves on click, entirely through
  `arcanum_sim` + `arcanum_net` over loopback.
- **Milestone A2:** a second client process over local UDP joins the host's world from a full
  snapshot, with its own character for that session. Two PCs walk around one world.
- The world always boots from a save. New Game writes an initial save; Load Game, Continue and
  the join snapshot use the same format and the same boot path.

## Non-goals (for these milestones)

- NPCs, containers, scenery interaction, dialogue, combat, scripts, quests.
- Client-side prediction, the tether, the inn, Steam transport, the host/join menus.
- Custom character creation (New Character stays dimmed).
- Drop-out and reconnect at any time. The owner wants this, but its design is open.
- The lobby UI, session broadcasting, and the inn/campfire restriction (disabled for
  development). Character reset is designed for but not built.

## Anti-goals

- Client code that changes shared state or combat results directly. The only state a client may
  change first is its own PC's out-of-combat position (and the combat-start report); everything
  else comes from the host.
- Message types designed ahead of the state they carry.
- A save format that differs from the join snapshot.

## Owner's decisions

- **Two separate ECS worlds.** The sim has its own Bevy `World`; the client has another. They
  share only net ids and talk only through the transport. A remote client has only the client
  world, so single player and remote play run the same client code.
- **Hybrid trust, for snappy responses:**
  - **Out of combat, the client owns its own PC's movement.** A click paths and walks at once on
    the client. The client sends its path/position to the host, which checks it (walkable tiles,
    plausible speed, later the tether), then accepts and relays it to other players or rejects it
    and sends a correction.
  - **Any client may start combat, and combat is party-wide.** A client enters combat when it
    detects aggro locally or when its player chooses to. It enters at once and reports positions,
    and the host puts every party member inside the engagement radius into combat mode. Members
    outside it stay free, so players are not forced to adventure together; one who walks into the
    radius joins at the bottom of the turn order on the next round (as in `multiplayer.md`). The host checks them within a tolerance and builds the encounter,
    correcting positions during the combat-start pause if needed.
  - **In combat, the host is fully authoritative.** The client computes only the AP cost of an
    action and whether its character can make it (enough AP, reachable, in range), so it can start
    the attack, spell or movement animation at once. Everything else is host-side: hit and miss,
    damage, mana and fatigue costs, effects and final positions. The host recomputes the AP and
    legality too, corrects the client and returns the new state.
  - **Shared things are always host-confirmed:** picking up items, containers, doors, NPCs,
    dialogue, quest and world flags. Two players grabbing one item is resolved by the host only.

- Multiplayer first; the build order below.
- **Inns and campfires** are more prominent than in the original. They are where players save,
  where others join, and where a host broadcasts a multiplayer session. For development, a
  setting turns this restriction off: saving and joining work anywhere.
- **Joining:** a player can only join while the host is checked into an inn and sleeping. A join
  happens at a quiet point (no combat, dialogue or movement), so the host sends one full snapshot
  and normal deltas follow. The joiner's PC appears in the inn (or campfire). In A2 the
  restriction is turned off and the join is a "join now" stub.
- **Characters per session:** a player chooses Join from the lobby. If they have no character in
  that session, they create one (in A2: pick a pre-made, since character creation does not exist
  yet). Characters belong to that session and are saved by the host in its `WorldSave`. A player
  can reset their character at any time, which discards it and returns them to character
  selection.
- **New Game creates a save:** after character creation (or the pre-made picker's Next), the game
  writes an initial save holding the character and the starting map and position, then boots the
  world from it.
- Combat is turn-based only. This milestone has no combat, but the message design must not assume
  a real-time mode.
- From earlier discussion: the original streams 64×64-tile sectors and records per-sector
  changes. Streaming follows that model, not per-entity hydration radii.

## Approaches considered

1. **Single-player world first, add the network split later.** Fastest to see a world. Rejected:
   every system gets rewritten when authority moves to the host.
2. **Netcode first, with an empty world.** Proves the transport early. Rejected: with no state to
   carry, the message set is guesswork and gets redone.
3. **The world built on the split from its first line (chosen).** It is slightly slower to the
   first picture, but every message carries real state, and A2 tests the architecture before any
   content is built on it.

## Build order

Each step is its own PR and ends in something testable.

1. **Map formats** (`arcanum_formats`). Readers for sector files (tiles, blocking, the object
   list), the object types needed for static scenery (walls, portals, scenery) in addition to
   PCs, map properties, the tile-name tables, and art id → path via the
   `.mes` index files. Find the map and position the original starts a new game on (Arcanum.exe
   and the data). Done when every sector of the starting map parses in a real-data test.
2. **`arcanum_sim` and `arcanum_net` crates.** `PlayerIntent`, `LedgerDelta` stamped with a
   tick number that only increases, a `Transport` trait, and `LoopbackTransport`. Done when a
   headless test sends an intent and gets the expected delta back.
3. **Save format and boot.** A `WorldSave` holding the session's state (players' characters, map,
   positions, and later the per-sector changes). Pre-made Next builds the initial save from the
   chosen prototype and the starting map, writes it, and boots the host from it. A development
   quick-save key serializes the live host state (the inn/campfire rule is off in development).
   Done when tests round-trip a save, including one taken mid-session, and boot an identical sim
   from it.
4. **Host sim loads the world.** The starting map's sectors around the PC, the PC as an entity,
   and movement validation at a fixed tick. Tile pathfinding and movement rules are shared code
   in `arcanum_sim` (plain functions, usable from both worlds) over the static blocking data plus
   host-replicated dynamic blockers. Done when headless tests give the right path, accept a valid
   client move, and correct an invalid one.
5. **Client draws the world.** Isometric terrain, tiles and static scenery (depth-sorted) for the
   loaded sectors, and the camera, replacing the placeholder. Unchanged map content comes from
   the client's own copy of the original data; game state and changes to map content come only
   from host deltas. Done with a screenshot check on real data.
6. **`.art` animation playback.** Frame timing, 8 directions, frame offsets. The PC stands and
   walks.
7. **Click to move, end to end.** Click → tile → the client paths and walks its PC at once →
   sends the move to the host → the host validates and accepts or corrects → the client smooths
   any correction. **Milestone A.**
8. **Milestone A2.** `UdpTransport`, a second client process, and a join: the joiner sends a
   player id; if the host's save has no character for it, the joiner picks a pre-made and sends
   it in a `SubmitCharacter` intent; the host adds it to its `WorldSave` and sends the snapshot,
   then deltas. Two PCs on the map, each controlled from its own window.

## Builder's calls (proposed by Claude, not owner decisions)

- `arcanum_sim` uses `bevy_ecs` (and `bevy_app` for scheduling) with no rendering, audio or
  windowing. Host systems are ordinary Bevy systems on a fixed tick. In single player, the host
  runs in the same process as the client.
- `arcanum_sim` and `arcanum_net` do not depend on `arcanum_client`. `arcanum_net` depends on
  `arcanum_sim` for message payload types.
- Saves are this project's own format (serde), not the original's. They store references into the
  original data (prototype numbers, map names) plus deviations, never copies of original assets.
- The join snapshot is a `WorldSave`. One serializer serves saving, loading and joining.
- The client keeps `last_confirmed_tick` and drops stale or duplicate deltas, as in
  `multiplayer.md`.
- Entities are identified across the wire by host-issued ids, never by Bevy `Entity`.
- Until Steam ids exist, a player is identified by a random id stored in that player's config.
  The host keys session characters by it.
- Static scenery blocks movement on the host from the same data the client draws.

## Left to the builder

- Exact message variants, beyond the move report, the correction, and position, spawn and
  despawn deltas.
- Validation tolerances and how corrections are smoothed.
- The save file's location and name, and how `SaveGames` lists it.
- Pathfinding algorithm details, movement speed per tick, how many sectors are loaded around the
  PC.
- The UDP framing and the wire encoding.

## Related

- [Multiplayer](multiplayer.md): the co-op architecture this milestone starts.
- [Combat design](combat_design.md), [Companions](companions.md): later milestones.
