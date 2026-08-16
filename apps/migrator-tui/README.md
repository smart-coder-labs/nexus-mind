# nexusmind-migrate

Interactive front-end for the knowledge migrations. Pick a source, see what it
contains before spending anything, watch the run, review the queue, commit.

```bash
cargo build --release                      # in apps/backend, for migrate-knowledge
cargo run --release                        # here
```

## What it is, and what it is not

It **drives** `migrate-knowledge`; it does not reimplement it. Every connector
rule — what gets excluded, how a unit is identified, what redaction does — lives
in the backend crate and nowhere else. The equivalent command line is on screen
at all times, so nothing here is a capability the CLI lacks, and anything you do
here you can script tomorrow.

The two processes talk over NDJSON on stdout (`migrate-knowledge --json`). That
mode is useful on its own: pipe it to `jq`, or to any supervisor that wants
progress instead of silence.

## Safety properties it preserves

These are not UI conventions. They are the reason the migration system is
allowed to touch client knowledge at all.

| Property | How the TUI holds it |
|---|---|
| Production is never the accident | The backend URL defaults to `localhost`, **not** to `NEXUSMIND_BASE_URL`. If that variable points elsewhere it is shown, labelled, and unused until typed in. |
| A DSN never reaches `argv` | Read from `NEXUSMIND_SOURCE_DSN`. It is never a field, never an argument, and never drawn — `ps`, shell history and command logs stay clean. |
| Row sampling needs four answers | An explicit table allowlist, a bounded row limit, local redaction, and a recorded attestation. All four, every time; turning sampling off clears all four. |
| Nothing is written until commit | The pipeline diagram in the header says so on every frame. Staging is not writing. |
| Some candidates cannot ride along | Harness candidates and client-attested ones are excluded from batch approval and marked as such. |

## Keys

| | |
|---|---|
| `Tab` / `⇧Tab` | move between screens |
| `↑` `↓` | move between fields or candidates |
| `Enter` | edit a field · open a run · toggle a switch |
| `Space` | toggle a switch · select a candidate |
| `d` | dry run — scan and report, post nothing |
| `r` | run — scan, classify, stage for review |
| `x` | stop a run (staged work survives) |
| `e` | cycle the activity panel: both / agents only / logs only |
| `↑` `↓` / `f` | inspect an exchange on the run screen / resume following the newest |
| `R` | load the review queue, or list runs |
| `a` / `j` | approve / reject the candidate under the cursor |
| `s` | send the candidate back to the queue, undecided |
| `A` | approve the selection — or the **whole queue** when nothing is selected |
| `X` | cancel a run's pending candidates (on the run list) |
| `C` | commit approved candidates |
| `p` | go back to the run list |
| `t` | test the backend connection |
| `Ctrl-U` | clear the field being edited |
| `?` / `q` | help / quit |

A pre-filled field is replaced by the first character you type, as in any form.
Backspace first if you meant to edit rather than replace.

There is no *delete* for a run, only cancel: `migration_provenance.run_id` is
`ON DELETE CASCADE`, so removing a run would erase the record of where its
already-committed knowledge came from — the audit trail this pipeline exists to
produce. Cancel drops what is still pending and leaves that record standing.

Backend calls run on their own thread. The terminal stays live while one is out,
and a call to an unreachable backend can be navigated away from or quit out of
rather than waiting out its timeout.

## The Agents panel

The run screen shows two panels side by side: **Agents** (every exchange with
the classifier — what was asked, what came back, tokens, duration) and **Logs**
(the runner's own output). `e` expands either to the full width.

It is worth knowing about because a classifier can fail while looking like it
works. A run reporting `classified 0 / fallback 249` says only that the answers
were unusable; the Agents panel shows the answer itself. That is how the
classifier's real problem was found: the model was replying about a memory
protocol from the operator's `CLAUDE.md` instead of returning a candidate, and
`--output-format json` reports the *last* message, so a closing remark replaced
the JSON. The runner now gives the classifier its own system prompt
(`--system-prompt`, `--exclude-dynamic-system-prompt-sections`) and runs it from
a neutral directory.

## The mascot

A robot mascot is drawn on the run screen, bound to what the run is doing:
Three moods, not one per pipeline stage:

- **Working** — anything running. Cycles every action set in the sheet in turn
  (`walk`, `scan`, `analyze`, `pick up`, `carry`, `transfer`, `type`, `run`),
  each one a complete animation, then starts again.
- **Celebrating** — a run that staged and finished cleanly, or a commit. Plays
  once and holds on the last frame, so a finished run does not quietly appear
  to start again.
- **Resting** — nothing happening. The eight sitting poses from
  `reposo-bot.png`, held long enough to read as poses; they are separate poses
  rather than a cycle, so they are paced slowly on purpose.

Miming the current stage was tried first and abandoned: tying each stage to one
short loop meant most of the sheet was never seen, and the mascot carries no
information anyway, so a set that does not match the stage costs nothing.

### It is decoration. It is never the only thing telling you something.

This is the rule the rest of the section exists to protect. Every state the
mascot depicts is *already* on screen as text, a counter, a gauge or the
pipeline diagram. Nothing is conveyed by the mascot alone, and nothing depends
on it having rendered. A terminal that cannot draw it loses a picture and no
information whatsoever.

Concretely, when the mascot cannot be drawn:

- The screen is laid out exactly as if the feature did not exist — its space is
  given back to the panels beside it. No blank rectangle, no placeholder, no
  "image unavailable" box.
- Nothing is logged, warned about, or shown to the operator. A terminal without
  truecolor is not a fault, and a tool that nags about decoration is worse than
  one without decoration.
- Every key, every panel and every number behaves identically.

A failure while decoding or drawing a frame disables the mascot for the rest of
the session and is otherwise silent. It must never take a frame of the UI with
it.

### What it needs

| Requirement | Why | Without it |
|---|---|---|
| A truecolor terminal (`COLORTERM=truecolor` or `24bit`) | Frames are drawn as Unicode half-blocks (`▀`), one cell carrying two pixels as foreground and background colours. 256 colours turn a white-and-cyan robot muddy. | Mascot off |
| A locale that does not *deny* UTF-8 | An unset `LANG` is not evidence of anything, and VS Code's terminal on macOS sets none — requiring one cost the feature on the most ordinary setup there is. Only a locale naming a non-UTF-8 charset (`en_US.ISO8859-1`) backs it off. | Mascot off only on explicit evidence |
| A free corner — 24x12 cells of untouched screen, 32x16 for the larger frame | Below that the robot is illegible and the space is better spent on the counters. | Mascot off, space returned to the panels |
| The `mascot` cargo feature (default on) | The frames are embedded in the binary with `include_bytes!`. Building without the feature drops them, and the code. | Mascot absent from the build |

It is drawn at 32x16 cells where there is room and 24x12 where there is less;
48 was tried and dropped, because at that size it reads as a second panel rather
than an ornament. It reserves nothing — it is overlaid on cells no widget has
written to, and if none are free it does not appear.

Each cell carries a 2x2 group of pixels, drawn with the quadrant glyphs
(`▘▝▖▗▚▞▌▐▛▜▙▟`, U+2596–U+259F). A cell can only show two colours, so each
group is approximated by its lightest and darkest members — but the extra
spatial detail is worth far more than the lost shades at this size: the face
reads as a face. Half-blocks (`▀` alone, 1x2 pixels per cell) shipped first and
were visibly softer.

All of these are Block Elements, as widely supported as `▀` itself — unlike the
sextants (U+1FB00), which would buy 2x3 pixels and demand a font most people do
not have.

### The real image, where the terminal can draw one

If the terminal answers that it speaks kitty, iTerm2 or sixel, it is handed the
actual PNG and draws actual pixels. Quadrants are the fallback, and what most
terminals get. `?` inside the app says which one is in use.

The question is asked in a **child process**, which is not fussiness:
`ratatui-image`'s `from_query_stdio` spawns a thread to read the terminal's
reply and, when no reply comes, returns on a timeout *without stopping it*.
That thread stays blocked on stdin for the life of the process and swallows
every keystroke — the UI draws and the keyboard does nothing. Running the query
in a child and killing it on a deadline is what keeps a decoration from taking
the application down with it.

`--no-mascot-graphics` or `NEXUSMIND_MIGRATE_GRAPHICS=0` keeps the mascot and
skips the query entirely.

No terminal graphics protocol is required. Sixel, kitty and iTerm2 inline images
are deliberately not used: they work in some terminals and corrupt others, and
half-blocks work everywhere that has colour.

### Turning it off

| | |
|---|---|
| `NEXUSMIND_MIGRATE_MASCOT=0` | off for the session |
| `--no-mascot` | off for one run |
| `m` | toggle while running |
| `cargo build --no-default-features` | not compiled in at all |

Detection is a capability check, never a terminal-name allowlist: `TERM` strings
lie in both directions, and the honest question is "did colour and Unicode come
out right", not "which emulator is this".

## Environment

| Variable | Effect |
|---|---|
| `NEXUSMIND_API_KEY` | pre-fills the key field |
| `NEXUSMIND_SOURCE_DSN` | the Postgres DSN for `db-schema`; required, and the only way to supply it |
| `NEXUSMIND_MIGRATE_BIN` | explicit path to `migrate-knowledge` |
| `NEXUSMIND_BASE_URL` | **not** used as a default — shown as a warning if it differs |

Without an override the runner is the **most recently built** of
`../backend/target/{release,debug}/migrate-knowledge`. Newest rather than
release-first: a stale release build that predates a flag makes the runner exit
before emitting a single event, which reads as a hang.

## Tests

```bash
cargo test
```

`runner::tests::stream_from_the_real_runner_parses` spawns the real binary over
this repository and asserts the stream parses — it is the contract test between
this crate's wire types and the backend's. It skips, with a message, when the
runner is not built.

`ui::tests` render every screen into a `TestBackend` at four sizes down to
20×6, and assert that neither the API key nor the DSN is ever drawn. Set
`DUMP_FRAMES=1` to print frames for visual review.
