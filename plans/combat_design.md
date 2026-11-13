# Combat design: companion AI

Status: plan, not implemented.

Combat is turn-based only. Companions act on their own turns, steered by directives the player
assigns.

## Turn cycle

At the start of every turn cycle, the player chooses the **party stance** (`Offensive`,
`Defensive` or `Neutral`). It applies to the whole party for that cycle, and the companions'
turns are then planned with it.

Open question: does the party stance override each companion's own `CombatStance`, or act as a
default that individual directives can change?

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
- **`CombatStance`**: `Offensive`, `Defensive`, `Neutral`.
- **`CombatStyle`**: `Kite`, `Engage`, `Hold`.

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

Stance multiplies risk and resource spending:

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
