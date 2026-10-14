# Who each interface is for

Decided 14 October 2026. Companion to [`ARCHITECTURE.md`](ARCHITECTURE.md),
which chose the frontends, and [`ROADMAP.md`](ROADMAP.md), which sequences
them.

## The decision

1. **The CLI is a developer tool.** It is for working *on* wow-coach. It is not
   the product and is not presented as one. No workflow aimed at a player may
   require it.
2. **The web client is the player's interface**, exactly as decided on
   22 September. See [`WEB-CLIENT-V1.md`](WEB-CLIENT-V1.md).
3. **The desktop client follows**, for players who want discovery and
   background watching rather than dropping a file.

## Why this needed saying

The architecture decision names two frontends — a browser client and a Tauri
desktop client — and a phase list running re-baseline, collector, parser,
coaching engine, web, desktop. Its repo layout names `addon/`,
`crates/wow-coach-core/`, `apps/web/` and `apps/desktop/`.

No CLI appears anywhere in it.

The CLI arrived in `d0c5007`, during Phase 2, described in its own commit
message as *"a CLI that prints a roster from real saved data — the first output
in this project a person reads."* That was a reasonable thing to build: the
parser needed something to render it. It was never promoted to being the
interface. It became one by default, because through Phases 2 and 3 it was the
only thing that could display an answer.

Where that left the three frontends by October:

| | Rust |
|---|---|
| `apps/cli/` | 2,322 lines |
| `apps/desktop/` | 36 lines of scaffold |
| `apps/web/` | 0 — a README |

The project had already diagnosed this and carried on regardless. `ROADMAP.md`,
under Phase 4, says of role editing:

> Today this is `wow-coach set-role` only, which is fine for the author and a
> wall for everybody else, and a tool built for other players cannot put a core
> setting behind a shell prompt.

And `apps/web/README.md` still describes itself as *"a placeholder until Phase 2
produces a core worth calling"* — a condition met weeks earlier. The principle
was written down before the practice drifted from it. This document exists so
the drift is harder to repeat.

## The rule that follows

A capability that only exists behind a shell prompt is not shipped. If the
coaching engine learns something new, the question is which screen shows it,
not which subcommand prints it.
