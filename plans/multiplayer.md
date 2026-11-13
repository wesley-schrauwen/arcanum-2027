# Multiplayer: co-op architecture

Status: plan, not implemented.

Co-op players are persistent party members in one shared world. There is one narrative timeline
and one loaded region, so there are no divergent story states and no split-map loading. One
player hosts.

**Party leader:** the host is the party leader by default. Leadership can be handed to another
player in game; it is a simple toggle. The leader makes the party-wide choices, such as the
party stance each turn cycle.

Client presentation is optimistic: animations, movement and effects start at once. The host is
authoritative over all state and every consequence.

## 1. Tether and spatial co-location

Players must stay within a **2000-tile radius** of one another. The radius bounds what has to be
streamed and simulated.

- **Single active sector:** the party stays inside one shared envelope. The host therefore
  simulates AI, pathfinding and physics for one active region only, never for several distant
  cities at once.
- **Soft, then hard tether:**
  - At about 85% of the limit, the client gives a visual and audio warning.
  - At 100%, the host clamps movement commands.
  - On map transitions and world-map fast travel, party members who are lagging behind snap to
    the rest of the party.

## 2. Per-character NPC reactions, shared quest state

NPCs react to each player's character differently, through Beauty, Charisma, race, alignment,
tech/magic aptitude and reputation. So a half-ogre technologist and an elven mage in the same
party are treated differently by the same NPC.

- **Disposition is per character; quest state is shared.**
  - The host stores each NPC's reaction to each player character separately:
    `(NpcId, PlayerCharacterId) -> ReactionScore`.
  - Quest and world flags live in one global party ledger.
- **Dialogue locking and spectating:**
  - When player A talks to an NPC, the host locks that NPC's dialogue to A's character. A's
    stats, such as Persuasion or Beauty, govern the checks.
  - Nearby party members see the lines and choices read-only, so nobody is surprised when an NPC
    turns hostile.

## 3. Shared combat bubble

When combat starts, every party member inside the engagement radius joins it immediately. No one
is left free to act outside the fight while others are locked in it.

- **`CombatEncounter`:** active combat is a host-side entity or resource. It holds:
  - the participants: party members in the radius and aggroed hostiles;
  - the turn order queue.

  Combat is turn-based only (see [Combat design](combat_design.md)), so the encounter needs no
  real-time/turn-based mode switch.
- **Joining mid-combat:** a party member who was outside the radius and walks in during the
  fight is added to the bottom of the turn order on the next round. The encounter does not
  reset.
- **Party stance:** the party leader chooses the party stance at the start of each turn cycle
  (see [Combat design](combat_design.md)).

## 4. Optimistic presentation, host ledger reconciliation

**The client predicts immediately:**
- **Movement:** it starts walking along the A* path the moment the player clicks. It corrects
  gently if the host reports an obstacle or a tether clamp.
- **Visual effects:** attack windups, spellcasting animations, muzzle flashes and projectile
  travel.

**These wait for the host:**
- Hit and miss rolls and critical failures.
- HP and Fatigue changes.
- Status effects, loot generation and death.

For example, when an NPC swings at player B, B's client plays the swing as soon as the intent
event arrives. The floating damage number, the blood and the HP bar drop wait for the host's
authoritative `StateDelta`.

**Ledger sequence numbers:**
- The host tags every authoritative state change with an ID that only ever increases.
- Each client tracks `last_confirmed_ledger_id`.
- After out-of-order packets or a latency spike, a client fast-forwards cosmetic state without
  applying damage or quest rewards twice.

## Crate boundaries

Three boundaries keep the transport swappable, for example to Steamworks later:

1. **`arcanum_sim`: pure, deterministic rules.**
   - Contents: `WorldLedger`, `PlayerIntent` commands, combat maths, reaction scores and quest
     flags.
   - No dependency on Bevy rendering, audio or Steamworks.
2. **`arcanum_net`: transport and replication.**
   - Clients send `PlayerIntent` to the host.
   - The host replicates `LedgerDelta` and cosmetic `CombatCue` events back to the clients.
   - It works the same over an in-memory channel for single player and over Steam Networking
     Sockets for co-op.
3. **`arcanum_client`: presentation and prediction.**
   - Runs local movement prediction.
   - Plays `.art` animations and sounds as soon as an intent or cue arrives.
   - Updates health, inventory and quest logs only from the replicated `WorldLedger`.

## Related

- [Combat design](combat_design.md): turn-based companion AI and the per-cycle party stance.
- [Companions](companions.md): follower management.
