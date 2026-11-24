// The boot watchdog: the loading screen names the step it's on, and a client that never starts
// says so (Mecha Royale's lesson, docs/PEERS.md: a load that dies quietly reads as "loading"
// forever). A plain script ahead of everything, so it works when nothing else does: it hears
// the loader's steps (window.bcBoot.step), errors anywhere on the page, and a panic's message
// (console.error), and it gives up on a step only once that step has stopped moving: a slow
// download still arriving is never called broken. A file of its own (and no inline handlers on
// the page), so the page runs under a CSP of `script-src 'self'` (the store's).
(function () {
  // In the game (not the gallery, a showcase, the echo spike or an autoplay) the front screen
  // (ui.js) is up from the start and shows the boot as LAUNCH filling: this screen stays out of
  // sight unless it fails before the page has taken over (`front-up`).
  var q = new URLSearchParams(location.search);
  var auto = q.get("autoplay") === "1" || (!!q.get("autopilot") && q.get("autoplay") !== "0");
  if (!q.has("gallery") && !q.get("showcase") && q.get("mode") !== "echo" && !auto) {
    document.documentElement.classList.add("front-early");
  }
  var STALL_MS = { loader: 20000, graphics: 15000, client: 30000, start: 60000 };
  var state = { step: "loader", detail: "", failed: false, done: false, errors: [], at: Date.now() };
  var timer = 0;
  function el(id) { return document.getElementById(id); }
  function arm() {
    clearTimeout(timer);
    var ms = STALL_MS[state.step];
    if (ms && !state.done && !state.failed) {
      timer = setTimeout(function () {
        fail(state.step === "client" ? "the download stalled" : "nothing happened for " + ms / 1000 + " s",
          "stalled at: " + state.step);
      }, ms);
    }
  }
  // A crash files itself, once a page (oneshot's boot.js): a panic (the wasm build aborts on one:
  // the trap that follows its message), a graphics context lost, or a boot that never started. Its
  // words are the console's last lines, which hold the panic's message; its place in the code, a
  // panic's `file:line:col`, is what tells one crash from another (the fingerprint). Sent as a
  // beacon (plain text: a page on another site asks nothing first), so a tab on its way out still
  // gets it there. The intake's share per address stops a crash on every reload filling it.
  var crashed = false;
  function crash(what, line, message) {
    if (crashed || !window.bcReports) return;
    crashed = true;
    try {
      var panic = state.errors.join("\n").match(/panicked at ([^\s:]+:\d+:\d+)/);
      var where = what === "panic" && panic ? panic[1] : what + (what === "boot" ? ":" + state.step : "");
      var b = window.__bc || {};
      var doc = window.bcReports.doc("crash", line, message, where, "browser", b);
      doc.crash = { what: what, where: where, step: state.step, after_ms: Date.now() - state.at, started: state.done };
      doc.errors = state.errors.slice();
      doc.machine = window.bcReports.machine();
      window.bcReports.send(doc, null, true);
    } catch (_) {}
  }
  // A trap: the wasm module stopped (after a panic, `unreachable`).
  function trapped(err, msg) {
    return (typeof WebAssembly !== "undefined" && err instanceof WebAssembly.RuntimeError) ||
      /^RuntimeError\b|unreachable executed/.test(msg || "");
  }
  function fail(message, detail) {
    if (state.failed || state.done) return;
    crash("boot", "the game didn't start: " + message, [message, detail || ""].filter(Boolean).join(" · "));
    state.failed = true;
    clearTimeout(timer);
    var boot = el("boot");
    if (boot) boot.classList.add("failed");
    var s = el("boot-status");
    if (s) s.textContent = "failed: " + message;
    var why = el("boot-why");
    if (why) why.textContent = [detail || ""].concat(state.errors.slice(-3)).filter(Boolean).join("\n");
    var box = el("boot-error");
    if (box) box.classList.remove("hidden");
    var gl = el("boot-webgl2");
    if (gl && window.BC_CONFIG && window.BC_CONFIG.gfx === "webgl2") gl.classList.add("hidden");
    tell({ failed: true, line: "failed: " + message });
  }
  // The front screen (ui.js) draws the boot as LAUNCH filling, from these.
  function tell(detail) {
    try { window.dispatchEvent(new CustomEvent("bc-boot", { detail: detail })); } catch (_) {}
  }
  window.bcBoot = {
    // A step begins (its line shows) or moves on (`fraction` of it done: the meter, re-arming the
    // stall timer).
    step: function (name, line, fraction) {
      if (state.failed || state.done) return;
      state.step = name;
      var s = el("boot-status");
      if (s && line) s.textContent = line;
      var fill = el("boot-fill");
      if (fill) {
        fill.classList.toggle("unknown", fraction == null);
        fill.style.width = fraction == null ? "" : Math.round(Math.min(1, fraction) * 100) + "%";
      }
      tell({ step: name, line: line || "", fraction: fraction == null ? null : Math.min(1, fraction) });
      arm();
    },
    fail: fail,
    // The first frame is drawn: the watch is over.
    done: function () {
      if (state.done) return;
      state.done = true;
      clearTimeout(timer);
      tell({ done: true });
    },
    failed: function () { return state.failed; },
    // The console's last error lines (a report carries them).
    errors: function () { return state.errors.slice(); },
    report: function () {
      var c = window.BC_CONFIG || {};
      return ["Before Colony boot report", "step: " + state.step, "after: " + (Date.now() - state.at) + " ms",
        "graphics: " + (c.gfx || "?") + " (" + (c.quality || "?") + ")", "renderer: " + (c.renderer || "?"),
        "page: " + location.href, "browser: " + navigator.userAgent, ""].concat(state.errors).join("\n");
    },
  };
  // A panic's message reaches the console before the trap that ends it: keep the last few.
  var error = console.error;
  console.error = function () {
    try {
      state.errors.push(Array.prototype.map.call(arguments, String).join(" ").slice(0, 600));
      if (state.errors.length > 8) state.errors.shift();
    } catch (_) {}
    return error.apply(console, arguments);
  };
  // Errors anywhere while the loading screen is up (resource errors don't bubble: the capture
  // phase hears them). Winit's control-flow exception is how Bevy hands the browser its loop.
  // Only the game's own files count: a script someone else puts on the page (a CDN's analytics
  // beacon, which the store's CSP blocks) failing, or throwing, is no reason to stop.
  function own(url) {
    if (!url) return true;
    try { return new URL(url, location.href).origin === location.origin; } catch (_) { return false; }
  }
  window.addEventListener("error", function (e) {
    var t = e.target;
    // A script of ours that didn't load (the loader's, or the client's): any other element's is no
    // reason to stop.
    var msg = t && t.tagName
      ? (t.tagName === "SCRIPT" && own(t.src) ? "could not load " + t.src : "")
      : (own(e.filename) ? e.message : "");
    if (!msg || /exceptions for control flow/.test(msg)) return;
    if (trapped(e.error, msg)) crash("panic", "the game stopped", msg);
    fail(msg);
  }, true);
  window.addEventListener("unhandledrejection", function (e) {
    var msg = String(e.reason && e.reason.message || e.reason);
    if (/exceptions for control flow/.test(msg)) return;
    if (trapped(e.reason, msg)) crash("panic", "the game stopped", msg);
    fail(msg);
  });
  // The graphics context lost (a driver reset, the GPU process gone): the game can't draw again.
  document.addEventListener("DOMContentLoaded", function () {
    var c = el("bc");
    if (c) c.addEventListener("webglcontextlost", function () {
      crash("context", "the graphics context was lost", "webglcontextlost");
    });
  });
  arm();
})();

// The loading screen's buttons (wired here rather than inline, for the CSP).
document.addEventListener("DOMContentLoaded", function () {
  function on(id, fn) {
    var b = document.getElementById(id);
    if (b) b.addEventListener("click", fn);
  }
  on("boot-reload", function () { location.reload(); });
  on("boot-webgl2", function () {
    var u = new URL(location.href);
    u.searchParams.set("gfx", "webgl2");
    location.href = u.href;
  });
  on("boot-copy", function () {
    var b = this;
    if (navigator.clipboard) {
      navigator.clipboard.writeText(window.bcBoot.report()).then(function () { b.textContent = "COPIED"; }, function () {});
    }
  });
});

// Where the game server is (window.bcServer), for the loader's dial and the page's `status`:
// config.js's BC_SERVER, overridden by `?server=host:port` / `?http=url` / `?pin=1`. With no `wt`
// the page dials the server that served it, asking its `cert-hash` for the port (and the hash to
// pin: null when its certificate is CA-issued).
(function () {
  var params = new URLSearchParams(location.search);
  var baked = window.BC_SERVER || {};
  var wt = params.get("server") || baked.wt || "";
  var http = params.get("http") || baked.http || "";
  if (http && !/\/$/.test(http)) http += "/";
  window.bcServer = {
    wt: wt,
    http: http,
    pin: params.has("pin") ? params.get("pin") !== "0" : !!baked.pin,
    // A path on the game server's HTTP side (relative to the page when it's the page's own).
    url: function (path) { return http + path; },
    // Where a report goes (F7's, and a crash's): the game server's `bugs`, unless config.js names
    // another intake that takes the same body (the Elo Pros one, `https://elopros.com/api/bugs`).
    reports: baked.reports || http + "bugs",
  };
  // What the page was built from (config.js's BC_BUILD), in the shape a report's `build` takes.
  var b = window.BC_BUILD || {};
  var ver = /^[0-9A-Za-z.+_-]{1,40}$/;
  window.bcBuild = {
    version: ver.test(b.version || "") ? b.version : "dev",
    commit: ver.test(b.commit || "") ? b.commit : "unknown",
  };

  // Reports: a player's (F7, ui.js) and a crash's (above). The document is the one the game
  // server's `bugs` (crates/bc-server/src/bugs.rs) and the Elo Pros intake both take: `format` 1,
  // a `kind`, a one-line `issue_title`, the doc to `read_first`, when it was `reported`, the
  // `build`, and a `fingerprint` that groups one bug's reports (the server works out its own).
  var KIND_DOCS = { crash: "CLAUDE.md", net: "docs/NETCODE.md", verb: "docs/CONTROLS.md" };
  var PLACE_DOCS = { town: "docs/FREE_ROAM.md", city: "docs/COLONY.md" };
  var TITLES = { look: "something looks wrong", world: "something in the world is wrong",
    verb: "a control didn't do what it should", net: "lag", numbers: "a number is wrong",
    crash: "the game stopped", other: "something's wrong" };
  // The doc a fixer opens first: by the kind, then the page's screen or panel, then the place.
  function readFirst(kind, at) {
    if (KIND_DOCS[kind]) return KIND_DOCS[kind];
    if ((at.screen && at.screen !== "playing") || (at.panel && at.panel !== "none")) return "docs/UI.md";
    return PLACE_DOCS[at.place] || "docs/DESIGN.md";
  }
  function utf8(s) {
    if (typeof TextEncoder !== "undefined") return new TextEncoder().encode(s);
    var bin = unescape(encodeURIComponent(s)), out = [];
    for (var i = 0; i < bin.length; i++) out.push(bin.charCodeAt(i));
    return out;
  }
  // FNV-1a, 64-bit, as 16 hex digits, in two 32-bit halves (no BigInt on an old browser's page):
  // the same as the server's `bugs::fnv1a`.
  function fnv1a(s) {
    var hi = 0xcbf29ce4, lo = 0x84222325, bytes = utf8(s);
    for (var i = 0; i < bytes.length; i++) {
      lo = (lo ^ bytes[i]) >>> 0;
      var a = lo * 0x1b3;
      hi = (hi * 0x1b3 + Math.floor(a / 4294967296) + ((lo << 8) >>> 0)) % 4294967296;
      lo = a % 4294967296;
    }
    return ("00000000" + hi.toString(16)).slice(-8) + ("00000000" + lo.toString(16)).slice(-8);
  }
  // One line, no control or invisible characters, at most `max`.
  function oneLine(s, max) {
    var flat = String(s || "").replace(/[\u0000-\u001f\u007f\u200b-\u200f\u202a-\u202e\u2060-\u2069\ufeff]+/g, " ");
    flat = flat.replace(/\s+/g, " ").trim();
    return flat.length > max ? flat.slice(0, max - 1) + "…" : flat;
  }
  window.bcReports = {
    readFirst: readFirst,
    fingerprint: function (kind, commit, doc, where) {
      return fnv1a(kind + "|" + commit + "|" + doc + "|" + (where || ""));
    },
    // A report of `kind`: its title from the first line of `title` (else the kind's own words),
    // `words` as typed, `where` a crash's place in the code, `runtime` `browser` or `agent`, and
    // `at` the page's screen, panel and place (for the doc to read first).
    doc: function (kind, title, words, where, runtime, at) {
      var doc = readFirst(kind, at || {});
      var line = oneLine(title, 120) || TITLES[kind] || kind;
      return {
        format: 1,
        kind: kind,
        fingerprint: window.bcReports.fingerprint(kind, window.bcBuild.commit, doc, where),
        issue_title: "[" + kind + "] " + line,
        read_first: doc,
        reported: new Date().toISOString().replace(/\.\d+Z$/, "Z"),
        build: { version: window.bcBuild.version, commit: window.bcBuild.commit, platform: "web", runtime: runtime },
        words: String(words || "").slice(0, 4000),
      };
    },
    // What it ran on: the browser, the GPU (the loader's), the screen. Nothing about who.
    machine: function () {
      var c = window.BC_CONFIG || {};
      var r = window.devicePixelRatio || 1;
      return {
        user_agent: navigator.userAgent,
        gpu: c.renderer || "?",
        graphics: (c.gfx || "?") + " (" + (c.quality || "?") + ")",
        screen: screen.width + "x" + screen.height + "@" + r,
        view: innerWidth + "x" + innerHeight,
      };
    },
    // Files `report` (and `shot`, a base64 PNG, if any). As plain text, so a page on another site
    // than the intake asks nothing first. `beacon`: fire and forget (a crash, a tab going), and
    // true if the browser took it; else a promise of the intake's answer, `{id, fingerprint, seen,
    // shot}`, or a rejection saying why not (an Error with the answer's `status`).
    send: function (report, shot, beacon) {
      var body = { game: "before-colony", report: report };
      if (shot) body.shot_png_b64 = shot;
      var text = JSON.stringify(body);
      var to = window.bcServer.reports;
      if (beacon && navigator.sendBeacon) return navigator.sendBeacon(to, text);
      return fetch(to, { method: "POST", body: text, credentials: "omit",
        headers: { "Content-Type": "text/plain;charset=UTF-8" } }).then(function (res) {
        return res.json().catch(function () { return {}; }).then(function (said) {
          if (res.ok) return said;
          var e = new Error(said.error || (res.status === 404 ? "this server takes no reports" : "the server said " + res.status));
          e.status = res.status;
          throw e;
        });
      });
    },
  };
})();
