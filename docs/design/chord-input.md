# Chord input (proposal)

Status: **proposal, not implemented.** Today a rule `key` array is a chord
recognized when all keys are pressed within `timing.chordMs`
([config-format.md](../config-format.md)); this document describes a stricter
"chord" trigger mode for letter, digit and command chord keymaps
(`D+F → 1`, `D+F+G → Я`, chord → macro or layer).

## Problem

Recognition lives in `mapper/engine/mod.rs` (`handle_chord()`), triggers are
parsed by `config::key_catalog::trigger_keys()`, rules are built in
`engine/builder.rs`. The current logic does not buffer a possible chord start:

| Config and input | Current result |
| --- | --- |
| `D+F → text:1`; press D, then F | `KEY_D down` is emitted before the chord; a stray `d` may appear |
| `D+F → 2`, `D+F+G → 3`; press D, F, G | `D+F` wins; G cannot extend it |
| `D+F`, then `F+G` while F is held | F stays consumed; the second chord is not recognized |

## Configuration

| Field | Where | Meaning |
| --- | --- | --- |
| `triggerMode: "normal" \| "chord"` | rule | missing = `normal` (current behaviour) |
| `defaultChordWindowMs` | settings | max collection time from the first participant; start at 50 ms, range 10–250 |
| `conditionLayerId` | rule, later stage | scope: missing = global, value = only while that layer is active |

```json
{ "key": ["KeyD", "KeyF"], "triggerMode": "chord", "tap": "text:1" }
```

These are optional fields, allowed within keymap format version 1. Existing
keymaps are not migrated; without chord rules the old path and its tests stay
unchanged. No per-rule windows in the first version.

## Recognition

- Only new presses that can start an available chord are buffered; other keys
  keep the normal path, preserving order. Rules whose conditions do not match
  add no delay. Candidates and active layers are snapshotted at session start.
- The window ends at first participant + `defaultChordWindowMs`; later presses do
  not extend it. Participants must be held together at the decision point; press
  order is irrelevant. A key emitted before the session is never absorbed later.
- Decide when: a full candidate has no possible extension; the window expires
  (longest full candidate); a participant is released (longest candidate full
  just before the release); an unrelated event arrives (resolve first). Clicks
  and unrelated presses are boundaries.
- Two different longest candidates are an ambiguity: fall back and report it;
  never choose by map or file order. Static validation catches obvious cases.
- Without a full candidate, buffered events replay through the normal single-key
  path (remap, swallow, tap/hold/double tap) with original timestamps, bypassing
  the recognizer. Auto-repeat does not add participants; repeats are not flushed
  in a burst.

## Chosen chord lifecycle

- Identity is a chord instance with its participant set, not the first key.
- Hold timing starts at the last participant's press; the window is not added to
  the hold threshold.
- Tap runs once on the first participant release unless hold took over; a long
  press without hold still runs tap, never the raw physical chord.
- A layer is active while all participants are held; long hold runs once;
  `holdModifiers` and held output keys (e.g. Alt for window switching) are kept.
- Absorbed physical events are never replayed; their releases only clear state.
- First version: a used participant becomes available again after its own
  release and a new press. Independent chords do not block each other.

## Interactions

- A single-key rule on D may coexist with `D+F`; it runs on fallback, with no
  side effects before the decision.
- A normal multi-key rule and a chord rule sharing participants must not be
  active together in the first version: the editor reports the conflict.
  Disjointness by conditions is allowed only when the validator can prove it.
- A context change cancels an unfinished session; the replay uses the original
  snapshot. A config change resolves pending input with the old config first.
- Stop, device loss and overflow clear bounded buffers and release keys; buffered
  text is never typed late.
- Chords inside layers (later stage): priority top active layer → lower layers →
  global; longest candidate after scope selection.

## Architecture

All recognition stays in `tecloid-core`; the shell only edits fields and shows
diagnostics. Extract a session recognizer from `handle_chord()` that outputs
"wait / chosen chord / replay"; actions keep running in the shared tap/hold/layer
machine. Deadlines go through `next_deadline()` and `tick()`; a press exactly at
the deadline is inside the window, and `tick` vs. event order must not change the
result.

UI: per-rule "Normal | Chord" choice (chord needs ≥ 2 distinct keys), window in
behaviour settings, no double tap for multi-key rules, diagnostics for mixed
modes, duplicate sets, ambiguity and missing layers. Key order does not change a
set's identity; new strings follow [i18n.md](../i18n.md).

## Stages

1. Contract and regressions; round-trip of new fields without recognition.
2. Buffering, fallback, fixed window, longest candidate, full release for reuse.
3. Layers, tap/hold, long hold, held output keys, conditions, config changes.
4. Chords inside layers.
5. Continuous overlapping input (`D+F → F+G`) — separate design, opt-in.

## Acceptance

Deterministic engine tests with injected `Instant`s covering: no chord rules →
unchanged output; `D+F` in both orders → exactly `1`; single D keeps its
mapping; D released before F → two singles; extension inside/after the window;
boundary press with both `tick`/event orders; ambiguity fallback; independent
chords; layer, long hold and held Alt with repeated Tab; every release order
without stray letters or stuck keys; context and config changes; repeat,
hotplug, stop, overflow; save and reopen in the editor. Add generative
key-down/up sequences, confirm letter output in a real Linux editor, and measure
latency on keyboards with different rollover.
