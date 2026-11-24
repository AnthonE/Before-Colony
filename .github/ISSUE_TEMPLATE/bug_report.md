---
name: Bug report
about: Something in the game is wrong. Say what happened, where, and on what.
labels: bug
---

<!--
A security finding never goes here: SECURITY.md says how to report one privately.

Player reports are being built (docs/NOW.md §rep): once the game writes a report file, paste it in
the block below and you can skip most of the rest.
-->

## Your report file, if the game wrote one

```
(the report file's contents, or delete this section)
```

## What happened

<!-- one or two sentences -->

**What you expected instead**

**Steps, if you can repeat it**

**Which part is wrong**: pick one. It decides which doc a fixer opens first.

- [ ] `look`: it draws wrong (`docs/UI.md` for a screen or the HUD; the fx stage for an effect)
- [ ] `town`: the ring and its town, its people, cars and the Watch (`docs/FREE_ROAM.md`)
- [ ] `fight`: flight, a weapon, the lock-on or a key (`docs/DESIGN.md`, `docs/LOCK.md`, `docs/CONTROLS.md`)
- [ ] `net`: it lagged, rubber-banded or dropped (`docs/NETCODE.md`)
- [ ] `economy`: the hangar, the fabricator, the Exchange or the Charter Board (`docs/DESIGN.md`, `docs/FLOOR.md`)
- [ ] `sound`: a sound or the music (`crates/bc-sound`)
- [ ] `crash`: the page stopped (paste the browser console's last lines)

**Build**: the page doesn't show its version yet, so say where and when you played.

| | |
|---|---|
| the page's address, and the date and time | |
| the commit, if you built it yourself | |
| browser and its version | |
| WebGL2 or WebGPU | |
| GPU and OS | |

**A screenshot**, if it's the kind of bug a picture shows.

---

Fixing this pays. Every accepted pull request pays the board's posted price, once the board lists
Before Colony's lane: https://elopros.com/games/before-colony/board/. There is nothing to claim and
nobody is ahead of you (`CONTRIBUTING.md`).
