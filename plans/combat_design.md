# Combat design: companion AI

Status: plan, not implemented.

Combat is turn-based only. Companions act on their own turns, steered by directives the player
assigns.

## Turn cycle

At the start of every turn cycle, the player chooses the **party stance** (`Offensive`,
`Defensive` or `Neutral`). Stance is party-wide only: companions have no stance of their own.
It applies to the whole party for that cycle. In multiplayer, the party leader chooses it (see
[Multiplayer](multiplayer.md)).

The stance does more than shift the AI's scoring weights. It grants the whole party a **small
buff for that turn**, matched to the stance. The buff rewards players who play in line with the
stance they chose.

The companion AI bases its decisions on the stance too. It plans its turns to make use of the
active buff, the same way a player would. Buff effects and sizes are still to be designed.

## 1. Core data structures

Companion directives are lightweight components, so each can be inspected and changed cleanly at
the start of every planning phase.

- **`TargetPriority`**: `Low`, `Mid`, `High`, `Kill`. Each maps to a score multiplier:

  | Priority | Multiplier |
  |---|---|
  | `Low` | 0.5 |
  | `Mid` | 1.0 |
  | `High` | 1.5 |
  | `Kill` | 3.0 |

- **`CombatRole`**: `Tank`, `Healer`, `Dps`.
- **`CombatStyle`**: `Kite`, `Engage`, `Hold`.

The party-wide **`PartyStance`** (`Offensive`, `Defensive`, `Neutral`) is a single resource set
each turn cycle, not a per-companion directive.

## 2. How the utility scorer evaluates a turn

At the start of a companion's turn, in the execution phase, the AI generates a list of candidate
actions and scores each one with weighted modifiers. Example candidates:
- move to tile X;
- cast Heal on ally Y;
- shoot enemy Z;
- bank AP for AC.

### Target selection

A target's score is its base threat score multiplied by the player's `TargetPriority` for it. An
enemy marked `Kill` gets a massive weight boost. That boost overrides the usual distance
penalties, unless the enemy is unreachable.

### Resource and stance thresholds

The party stance multiplies risk and resource spending for every companion:

- **`Offensive`** boosts the weights of damage actions. It also lowers the minimum Fatigue or
  Mana the companion keeps in reserve; for example, it will spend down to 10% Fatigue.
- **`Defensive`** boosts the weights of healing, buffing and AP banking. It also raises the
  reserve floor; for example, it keeps 40% Fatigue in reserve.

### Positioning and style constraints

Style filters or heavily weights the movement candidates before attacks are scored:

- **`Hold`** puts a near-infinite penalty on movement tiles, unless a hazard forces the companion
  to move.
- **`Kite`** rewards tiles that maximise distance from the nearest melee threat while staying
  within weapon or spell range.
- **`Engage`** rewards tiles next to the highest-priority target.
