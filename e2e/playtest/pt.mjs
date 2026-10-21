#!/usr/bin/env node
// The playtest driver: one browser kept open on the game and driven a command at a time, so an
// agent (or a person) can play it from a shell, look at what it shows, and note what needs work.
// `scripts/pt help` lists the commands; `.claude/skills/playtest/SKILL.md` is how to use them.
//
// The first `open` (or `showcase`) starts a small daemon holding the browser; every later command
// is a request to it, so the game keeps running between them. Each session (PT_SESSION, default
// `pt`) has its own browser, pilot and folder (e2e/artifacts/playtest/<session>/: screenshots,
// notes.md, the browser's console log); sessions share the world `scripts/pt up` started.
import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";
import { PNG } from "pngjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "../..");
const SESSION = (process.env.PT_SESSION ?? "pt").replace(/[^a-z0-9_-]/gi, "_");
const OUT = path.resolve(process.env.PT_OUT ?? path.join(ROOT, "e2e/artifacts/playtest"), SESSION);
const DAEMON = path.join(OUT, "daemon.json");
const BASE = process.env.BC_URL ?? `http://127.0.0.1:${process.env.BC_HTTP_PORT ?? 8080}`;
const [VW, VH] = (process.env.PT_VIEWPORT ?? "960x540").split("x").map(Number);
// Past this long without a command the daemon closes its browser (a forgotten one drawing in
// software takes a core).
const IDLE_S = Number(process.env.PT_IDLE ?? 2700);

const HELP = `scripts/pt — play Before Colony from a shell (PT_SESSION=<name> for a second browser)

The world
  up [survival|arcade]     start a server with drones, an agent, a miner and strollers (idempotent)
  down                     stop it (and every session's browser)
  world                    is it up? who's in it (the server's /status, summarised)

The browser (each prints a one-line status after)
  open [query]             load the game; default 'autoplay=1&name=<session>&quality=low'
                           e.g. 'autopilot=1&frame=altair' (the drone brain flies), 'autoplay=0' (title)
  showcase <scene> [cam] [t] [frames]   an offline scene (gfx.spec.ts lists them), screenshot
  close                    close this session's browser

Looking
  shot [label]             screenshot -> a PNG path (read it to see); waits for a fresh frame
  around [n]               turn 360/n degrees n times (default 4), one contact sheet of the views
  ui                       the page's visible text and controls (prompts, toasts, panels, buttons)
  status [key...]          window.__bc (the client's own status), all or some keys
  logs [all]               the browser console since the last call (errors first)

Doing
  key <Key[+Key]> [ms]     tap, or hold for ms (KeyW, Space, ShiftLeft+KeyW, Escape, KeyE, KeyM, F1, Tab)
  type <text>              type into whatever has focus
  click <selector> | click <x> <y>   a page control (CSS, text=..., role=button[name="..."]) or the canvas
  mouse <button> [ms]      hold a mouse button on the middle of the view (right: the secondary
                           weapon, which fires without the pointer locked; left needs the lock)
  turn <yaw> [pitch]       turn the eyes, degrees (right, up): flying or on foot
  walk <spot>              walk there and wait to arrive: bay: airlock cockpit suit fabricator stores
                           exchange; city: proving_ground exchange_floor charter_board bar hub_gate_1..3
                           tram seat pool gantry sight_0..9
  use                      use what's in view (E)
  cmd <json>               a dev command onto window.bcInbox, e.g. '{"cmd":"fly_to","spot":"bar","up":20,"ahead":30}'
                           '{"cmd":"aim_hostile","on":true}' '{"cmd":"skip"}' '{"cmd":"set","key":"hints","value":"false"}'
  wait <expr> [secs]       until a JS expression over s (= window.__bc) holds, e.g. 's.place=="city"' 300
  sleep <secs>             let the game run
  eval <js>                run JS in the page, print the result

Notes
  note <text>              add a finding to notes.md, with the last screenshot's path
  notes                    print notes.md

Env: PT_SESSION (default pt), BC_HTTP_PORT (8080), PT_HEADED=1 (a headed browser under xvfb-run),
     PT_VIEWPORT (960x540), PT_IDLE (seconds idle before the browser closes; 2700), PT_TIMEOUT
     (seconds a command may take; 600)`;

// ---------------------------------------------------------------- the client side (each command)

async function request(body) {
  const info = JSON.parse(fs.readFileSync(DAEMON, "utf8"));
  const res = await fetch(`http://127.0.0.1:${info.port}/`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout((Number(process.env.PT_TIMEOUT ?? 600) + 60) * 1000),
  });
  return { ok: res.ok, text: await res.text() };
}

async function alive() {
  if (!fs.existsSync(DAEMON)) return false;
  try {
    return (await request({ verb: "ping", args: [] })).ok;
  } catch {
    return false;
  }
}

async function startDaemon() {
  fs.mkdirSync(OUT, { recursive: true });
  fs.rmSync(DAEMON, { force: true });
  const log = fs.openSync(path.join(OUT, "daemon.log"), "a");
  const self = fileURLToPath(import.meta.url);
  const [cmd, args] =
    process.env.PT_HEADED === "1"
      ? ["xvfb-run", ["-a", "-s", `-screen 0 ${VW + 40}x${VH + 120}x24`, process.execPath, self, "serve"]]
      : [process.execPath, [self, "serve"]];
  spawn(cmd, args, { detached: true, stdio: ["ignore", log, log], env: process.env }).unref();
  for (let i = 0; i < 150; i++) {
    if (await alive()) return;
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`the browser daemon didn't start: see ${path.join(OUT, "daemon.log")}`);
}

async function client(verb, args) {
  // A reader that has seen enough (`| head`) isn't an error.
  process.stdout.on("error", () => process.exit(0));
  if (verb === "help" || !verb) return console.log(HELP);
  if (verb === "notes") {
    const f = path.join(OUT, "notes.md");
    return console.log(fs.existsSync(f) ? fs.readFileSync(f, "utf8") : "(no notes yet)");
  }
  if (!(await alive())) {
    if (verb === "close") return console.log("no browser open");
    if (verb !== "open" && verb !== "showcase") {
      console.error(`no browser open for session ${SESSION}: scripts/pt open first`);
      process.exit(2);
    }
    await startDaemon();
  }
  const { ok, text } = await request({ verb, args });
  process.stdout.write(text.endsWith("\n") ? text : `${text}\n`);
  if (!ok) process.exit(1);
}

// ---------------------------------------------------------------- the daemon (holds the browser)

// Every toast the page shows (a later one can replace it before a slow page is looked at again).
const KEEP_TOASTS = () =>
  addEventListener("DOMContentLoaded", () => {
    const t = document.getElementById("toast");
    const seen = (window.ptToasts = []);
    if (t) {
      new MutationObserver(() => {
        const text = (t.textContent ?? "").trim();
        if (text && seen[seen.length - 1] !== text) seen.push(text);
      }).observe(t, { childList: true, characterData: true, subtree: true });
    }
  });

// The game's wasm memory, kept where the status can read it (a leak, or a trap that's an
// allocation failing, shows there first).
const KEEP_MEMORY = () => {
  const keep = (r) => {
    const exports = (r.instance ?? r).exports;
    if (exports?.memory instanceof WebAssembly.Memory) window.ptMemory = exports.memory;
    return r;
  };
  for (const f of ["instantiate", "instantiateStreaming"]) {
    const was = WebAssembly[f];
    WebAssembly[f] = (...a) => was.apply(WebAssembly, a).then(keep);
  }
};

// The pointer stays free (PT_LOCK=1 lets the page lock it). Headless Chromium grants the lock, and
// with it held on foot the game's frames slow tenfold and its memory climbs by gigabytes until the
// wasm aborts (2026-10-21: `scripts/pt open`, then `cmd '{"cmd":"resume"}'`, and watch wasm_mb).
// Refused, the game asks for a click, as a page that won't lock does.
const NO_LOCK = () => {
  Element.prototype.requestPointerLock = function () {
    setTimeout(() => document.dispatchEvent(new Event("pointerlockerror")), 0);
    const refused = Promise.reject(new DOMException("pointer lock is off in playtests", "NotSupportedError"));
    refused.catch(() => {});
    return refused;
  };
};

// What the one-line status shows, in order, when the client has it.
const BRIEF = [
  "screen", "panel", "mode", "scene", "place", "seq", "district", "strip", "city_room", "focus",
  "terminal", "driving", "riding", "seated", "interior", "in_hall", "map_open", "objective",
  "waypoint", "frame", "alive", "speed", "pos", "fps", "wasm_mb", "link", "last_error",
];
// The page's overlays whose words say what the player is being told.
const WORDS = ["prompt-main", "use", "hint", "banner-text", "link-message", "news", "boot-status", "boot-why"];

async function serve() {
  fs.mkdirSync(OUT, { recursive: true });
  const browser = await chromium.launch({
    headless: process.env.PT_HEADED !== "1",
    args: [
      "--no-proxy-server",
      "--autoplay-policy=no-user-gesture-required",
      "--use-angle=swiftshader",
      "--enable-unsafe-swiftshader",
      "--ignore-gpu-blocklist",
    ],
  });
  const context = await browser.newContext({ viewport: { width: VW, height: VH }, baseURL: BASE });
  const page = await context.newPage();
  await page.addInitScript(KEEP_TOASTS);
  await page.addInitScript(KEEP_MEMORY);
  if (process.env.PT_LOCK !== "1") await page.addInitScript(NO_LOCK);
  const consoleLog = fs.createWriteStream(path.join(OUT, "console.log"), { flags: "a" });
  const lines = [];
  let read = 0;
  // When the game died (a wasm trap or a panic): everything it shows after is frozen.
  let crashed = "";
  const keep = (line) => {
    if (!crashed && /^\[pageerror\] unreachable|^\[crash\]|panicked at/.test(line)) {
      crashed = `${new Date().toISOString().slice(11, 19)}: ${line.split("\n")[0].slice(0, 160)}`;
    }
    lines.push(line);
    consoleLog.write(`${new Date().toISOString()} ${line}\n`);
  };
  page.on("console", (m) => keep(`[${m.type()}] ${m.text()}`));
  page.on("pageerror", (e) => keep(`[pageerror] ${e.message}`));
  page.on("crash", () => keep("[crash] the page crashed"));

  let shots = fs.readdirSync(OUT).filter((f) => /^\d{3}-.*\.png$/.test(f)).length;
  let lastShot = "";
  let toastsRead = 0;
  const bc = async () =>
    await page
      .evaluate(() => {
        const s = { ...(window.__bc ?? {}) };
        if (window.ptMemory) s.wasm_mb = Math.round(window.ptMemory.buffer.byteLength / 2 ** 20);
        return s;
      })
      .catch(() => ({}));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const timeout = Number(process.env.PT_TIMEOUT ?? 600) * 1000;

  async function until(what, pred, ms = timeout) {
    const t0 = Date.now();
    for (;;) {
      const s = await bc();
      if (pred(s)) return s;
      if (Date.now() - t0 > ms) throw new Error(`timed out after ${Math.round(ms / 1000)} s waiting for ${what}\n${brief(s)}`);
      await sleep(300);
    }
  }
  // A frame drawn after now (a page drawing in software can take seconds over one).
  async function freshFrame(ms = 30_000) {
    const f = (await bc()).frames ?? 0;
    await until("a fresh frame", (s) => (s.frames ?? 0) > f || s.showcase_frames !== undefined, ms).catch(() => {});
  }
  function brief(s) {
    // Left out: what isn't so (false, a -1 for none), but for a suit out and not alive.
    const told = (k) => s[k] !== undefined && s[k] !== "" && s[k] !== null && s[k] !== -1 && s[k] !== false;
    return BRIEF.filter((k) => told(k) || (k === "alive" && s.place === "space" && s.alive === false))
      .map((k) => `${k}=${typeof s[k] === "number" && !Number.isInteger(s[k]) ? s[k].toFixed(1) : s[k]}`)
      .join(" ");
  }
  // The status, the words on screen, and any new toasts.
  async function summary() {
    const s = await bc();
    const page_ = await page
      .evaluate(
        ([ids, from]) => {
          const shown = (el) => el?.checkVisibility({ opacityProperty: true, visibilityProperty: true });
          const words = ids
            .map((id) => [id, document.getElementById(id)])
            .filter(([, el]) => shown(el) && el.innerText.trim())
            .map(([id, el]) => `${id}: ${el.innerText.trim().replace(/\s+/g, " ").slice(0, 200)}`);
          const toasts = (window.ptToasts ?? []).slice(from);
          return { words, toasts, seen: (window.ptToasts ?? []).length };
        },
        [WORDS, toastsRead],
      )
      .catch(() => ({ words: [], toasts: [], seen: toastsRead }));
    toastsRead = page_.seen;
    const out = [brief(s) || "(no status yet: the game is loading)"];
    if (crashed) out.unshift(`!! THE GAME CRASHED at ${crashed}\n!! what it shows is frozen from then: scripts/pt logs, then scripts/pt open to reload`);
    for (const w of page_.words) out.push(`  ${w}`);
    for (const t of page_.toasts) out.push(`  toast: ${t}`);
    const errors = lines.slice(read).filter(isError).length;
    if (errors) out.push(`  ! ${errors} new console error(s): scripts/pt logs`);
    return out.join("\n");
  }
  const isError = (l) => /\[error\]|\[pageerror\]|\[crash\]|%cERROR|panicked|wgpu error|validation error/i.test(l);
  async function shoot(label) {
    shots += 1;
    const name = `${String(shots).padStart(3, "0")}-${(label || "shot").replace(/[^a-z0-9_-]+/gi, "_").slice(0, 40)}.png`;
    const file = path.join(OUT, name);
    await page.screenshot({ path: file });
    lastShot = file;
    return file;
  }
  async function push(cmd) {
    await page.evaluate((c) => (window.bcInbox ||= []).push(c), cmd);
  }
  async function onPage(selector) {
    const loc = page.locator(selector).first();
    await loc.waitFor({ state: "visible", timeout: 60_000 });
    // In the page itself: a page drawing in software is rarely still for Playwright's own click.
    await loc.evaluate((el) => el.click());
  }
  function query(q) {
    const p = new URLSearchParams(q ?? "");
    if (!p.has("showcase")) {
      if (!p.has("autoplay") && !p.has("autopilot")) p.set("autoplay", "1");
      if (!p.has("name")) p.set("name", SESSION === "pt" ? "Playtester" : SESSION);
    }
    if (!p.has("quality")) p.set("quality", "low");
    if (!p.has("gfx")) p.set("gfx", "webgl2");
    return p.toString();
  }

  const verbs = {
    ping: async () => "pong",
    open: async (q) => {
      const url = `/?${query(q)}`;
      crashed = "";
      await page.goto(url);
      await until("the game to start drawing", (s) => (s.frames ?? 0) >= 2, 240_000);
      if (/autoplay=1|autopilot=/.test(url)) {
        await until("the world (welcomed)", (s) => s.welcomed || s.screen === "failed", 240_000).catch(() => {});
      }
      return `opened ${BASE}${url}\n${await summary()}`;
    },
    showcase: async (scene, cam = "1", t = "6", frames = "12") => {
      crashed = "";
      const q = `showcase=${scene}&cam=${cam}&t=${t}&hold=${frames}&quality=${process.env.PT_QUALITY ?? "high"}`;
      await page.goto(`/?${query(q)}`);
      await until(`${frames} showcase frames`, (s) => (s.showcase_frames ?? 0) >= Number(frames), 420_000);
      const file = await shoot(`showcase-${scene}-${cam}-t${t}`);
      return `${file}\n${await summary()}`;
    },
    shot: async (label) => {
      await freshFrame();
      return `${await shoot(label)}\n${await summary()}`;
    },
    around: async (n = "4") => {
      const k = Math.max(2, Math.min(8, Number(n) || 4));
      const files = [];
      for (let i = 0; i < k; i++) {
        if (i > 0) {
          await push({ cmd: "look", yaw: 360 / k, pitch: 0 });
          await freshFrame();
        }
        await freshFrame();
        files.push(await shoot(`around-${i * Math.round(360 / k)}deg`));
      }
      const sheet = path.join(OUT, `${String(++shots).padStart(3, "0")}-around-sheet.png`);
      contactSheet(files, sheet);
      lastShot = sheet;
      return `${sheet}  (${k} views, turning right ${Math.round(360 / k)} degrees each; the full ones: ${files.map((f) => path.basename(f)).join(" ")})\n${await summary()}`;
    },
    ui: async () => {
      const snap = await page.locator("body").ariaSnapshot().catch((e) => `(no snapshot: ${e.message})`);
      // A table's rows say what their cells do: the cells' own lines are left out.
      const rows = snap.split("\n").filter((l) => !/^\s*- (cell|rowgroup)\b/.test(l));
      return `${rows.join("\n")}\n---\n${await summary()}`;
    },
    status: async (...keys) => {
      const s = await bc();
      const picked = keys.length ? Object.fromEntries(keys.map((k) => [k, s[k]])) : s;
      return JSON.stringify(picked, null, keys.length ? 0 : 1);
    },
    logs: async (all) => {
      const from = all === "all" ? 0 : read;
      const fresh = lines.slice(from);
      read = lines.length;
      const errs = fresh.filter(isError);
      const rest = fresh.filter((l) => !isError(l));
      const out = [];
      if (errs.length) out.push(`${errs.length} error(s):`, ...errs.map((l) => l.slice(0, 1200)));
      out.push(`${rest.length} other line(s)${rest.length > 60 ? " (the last 60)" : ""}:`, ...rest.slice(-60).map((l) => l.slice(0, 400)));
      return out.join("\n");
    },
    key: async (keys, ms) => {
      if (!keys) throw new Error("key <Key[+Key]> [ms]");
      const ks = keys.split("+");
      // Keys go to the game unless a field of the page has them (the radio's line, the callsign).
      await page.evaluate(() => {
        const a = document.activeElement;
        if (!a || a === document.body) document.getElementById("bc")?.focus();
      });
      if (ms === undefined) {
        if (ks.length === 1) await page.keyboard.press(ks[0], { delay: 60 });
        else {
          for (const k of ks) await page.keyboard.down(k);
          await sleep(60);
          for (const k of ks.reverse()) await page.keyboard.up(k);
        }
      } else {
        for (const k of ks) await page.keyboard.down(k);
        await sleep(Number(ms));
        for (const k of ks.reverse()) await page.keyboard.up(k);
      }
      await freshFrame();
      return summary();
    },
    type: async (...words) => {
      await page.keyboard.type(words.join(" "), { delay: 40 });
      return summary();
    },
    click: async (a, b) => {
      if (a !== undefined && b !== undefined && !Number.isNaN(Number(a)) && !Number.isNaN(Number(b))) {
        await page.mouse.click(Number(a), Number(b));
      } else if (a) {
        await onPage(a);
      } else throw new Error("click <selector> | click <x> <y>");
      await freshFrame();
      return summary();
    },
    mouse: async (button = "right", ms = "1000") => {
      if (!["left", "right", "middle"].includes(button)) throw new Error("mouse <left|right|middle> [ms]");
      await page.mouse.move(VW / 2, VH / 2);
      await page.mouse.down({ button });
      await sleep(Number(ms));
      await page.mouse.up({ button });
      await freshFrame();
      return summary();
    },
    turn: async (yaw = "0", pitch = "0") => {
      await push({ cmd: "look", yaw: Number(yaw), pitch: Number(pitch) });
      await freshFrame();
      await freshFrame();
      return summary();
    },
    walk: async (spot) => {
      if (!spot) throw new Error("walk <spot>");
      await push({ cmd: "walk_to", spot });
      // (A short walk can be over before it's seen: there already counts.)
      const going = await until(
        "the walk to start",
        (s) => s.walking_to || s.city_walking_to || s.focus === spot,
        30_000,
      ).catch(() => null);
      if (!going) return `didn't start walking to ${spot} (not on foot here, or no such spot on this strip)\n${await summary()}`;
      await until(`arriving at ${spot}`, (s) => !s.walking_to && !s.city_walking_to);
      await freshFrame();
      return summary();
    },
    use: async () => {
      await push({ cmd: "use" });
      await freshFrame();
      await freshFrame();
      return summary();
    },
    cmd: async (...json) => {
      const c = JSON.parse(json.join(" "));
      await push(c);
      await freshFrame();
      return summary();
    },
    wait: async (expr, secs) => {
      if (!expr) throw new Error("wait <expr> [secs]");
      const pred = new Function("s", `try { return (${expr}); } catch { return false; }`);
      await until(expr, pred, secs ? Number(secs) * 1000 : timeout);
      return summary();
    },
    sleep: async (secs = "1") => {
      await sleep(Number(secs) * 1000);
      return summary();
    },
    eval: async (...js) => {
      const r = await page.evaluate(js.join(" "));
      return JSON.stringify(r, null, 1) ?? "undefined";
    },
    note: async (...words) => {
      const text = words.join(" ").trim();
      if (!text) throw new Error("note <text>");
      const s = await bc();
      const where = ["place", "district", "seq", "scene"].map((k) => s[k]).filter(Boolean).join("/");
      const line = `- ${text}${where ? `  _(at ${where})_` : ""}${lastShot ? `  [${path.basename(lastShot)}](${path.basename(lastShot)})` : ""}\n`;
      const file = path.join(OUT, "notes.md");
      if (!fs.existsSync(file)) fs.writeFileSync(file, `# Playtest notes: ${SESSION}\n\n`);
      fs.appendFileSync(file, line);
      return `noted in ${file}`;
    },
  };

  let busy = Promise.resolve();
  let lastUsed = Date.now();
  const server = http.createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      busy = busy.then(async () => {
        lastUsed = Date.now();
        let verb = "?";
        try {
          const { verb: v, args } = JSON.parse(body);
          verb = v;
          if (verb === "close") {
            res.end("closed");
            await shutdown();
            return;
          }
          const fn = verbs[verb];
          if (!fn) throw new Error(`unknown command '${verb}': scripts/pt help`);
          res.end(String(await fn(...(args ?? []))));
        } catch (e) {
          res.statusCode = 500;
          res.end(`${verb}: ${e.message ?? e}`);
        }
        lastUsed = Date.now();
      });
    });
  });
  async function shutdown() {
    fs.rmSync(DAEMON, { force: true });
    await browser.close().catch(() => {});
    server.close();
    process.exit(0);
  }
  process.on("SIGTERM", shutdown);
  process.on("SIGINT", shutdown);
  setInterval(() => {
    if (Date.now() - lastUsed > IDLE_S * 1000) {
      console.log(`idle for ${IDLE_S} s: closing`);
      shutdown();
    }
  }, 30_000).unref();
  server.listen(0, "127.0.0.1", () => {
    fs.writeFileSync(DAEMON, JSON.stringify({ port: server.address().port, pid: process.pid, session: SESSION }));
    console.log(`playtest daemon for ${SESSION} on port ${server.address().port}, out ${OUT}`);
  });
}

// Tiles screenshots, each at half size, two to a row.
function contactSheet(files, out) {
  const imgs = files.map((f) => PNG.sync.read(fs.readFileSync(f)));
  const w = Math.floor(imgs[0].width / 2);
  const h = Math.floor(imgs[0].height / 2);
  const cols = 2;
  const rows = Math.ceil(imgs.length / cols);
  const sheet = new PNG({ width: w * cols + (cols - 1) * 4, height: h * rows + (rows - 1) * 4 });
  sheet.data.fill(255);
  imgs.forEach((img, i) => {
    const ox = (i % cols) * (w + 4);
    const oy = Math.floor(i / cols) * (h + 4);
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const d = ((oy + y) * sheet.width + ox + x) * 4;
        for (let c = 0; c < 4; c++) {
          // The mean of the 2x2 block under the pixel.
          let sum = 0;
          for (const [dx, dy] of [[0, 0], [1, 0], [0, 1], [1, 1]]) {
            sum += img.data[((2 * y + dy) * img.width + 2 * x + dx) * 4 + c];
          }
          sheet.data[d + c] = sum >> 2;
        }
      }
    }
  });
  fs.writeFileSync(out, PNG.sync.write(sheet));
}

const [verb, ...args] = process.argv.slice(2);
if (verb === "serve") {
  serve().catch((e) => {
    console.error(e);
    process.exit(1);
  });
} else {
  client(verb, args).catch((e) => {
    console.error(e.message ?? e);
    process.exit(1);
  });
}
