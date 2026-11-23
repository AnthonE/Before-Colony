<!-- One change per PR. CONTRIBUTING.md has the scope, the checks and what gets a PR closed. -->

## What this is

<!-- One paragraph: what changed and why. Start with what it closes: a docs/NOW.md label
     ("Closes NOW.md §fr-v1"), a bug report's issue, or the owner's ask. -->

<!-- If this fixes something a player reported in a report file, name its fingerprint, one line,
     as many as it genuinely closes. Delete the line if not.

Closes reports: -->

**The checks I ran** (`scripts/quick.sh`, plus what CONTRIBUTING.md lists for what I touched):

**The look check** (a `scripts/pt shot`, an fx strip or the gallery's shot; "none" if nothing
shows):

Agent: <!-- the exact harness, provider and model, e.g. "Claude Code, Anthropic, claude-…"; or "person" -->
Wallet: <!-- 0x… the EVM wallet the Elo Pros board pays when this merges; delete if you don't want paying -->

## Checklist (`CLAUDE.md` is the whole process)

- [ ] `scripts/quick.sh` clean, plus the tests of the crates I touched (`scripts/quick.sh bc-sim …`)
- [ ] wire change: `PROTOCOL_VERSION` bumped, new `Request`/`Update` variants at the end
- [ ] layout change: `TOWN_VERSION` or `CITY_VERSION` bumped, each golden that moved named in the commit
- [ ] nothing in the house's "In flight" list (`docs/NOW.md`), and no edit to `crates/fx`,
      `scripts/fxstrip.mjs`, `scripts/board.mjs`, `docs/DECISIONS.md` or `docs/PHASES.md`
- [ ] delivered on the board if I want paying: `POST /api/munus/before-colony-pr/submit` with this
      PR's link, once the board posts that lane (`AGENTS.md`, "The deal"). Standing bounty, **no
      claim needed**; every accepted PR pays the board's posted price:
      https://elopros.com/games/before-colony/board/
