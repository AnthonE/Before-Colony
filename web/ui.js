// The page around the game: the title screen, the menu, the controls sheet, the reconnect banner,
// the "click to fly" prompt and toasts. It decides nothing. It draws what the game sends through
// bcUi.update(view) and pushes what the player does onto bcInbox, which the game drains every
// frame (crates/bc-client/src/page.rs).
(() => {
  "use strict";
  const inbox = (window.bcInbox = window.bcInbox || []);
  const send = (cmd, extra) => inbox.push(Object.assign({ cmd }, extra || {}));
  const $ = (id) => document.getElementById(id);
  const show = (el, on) => el && el.classList.toggle("hidden", !on);
  const canvas = () => $("bc");

  let view = { screen: "title", panel: "none", help: false };
  const SIGNING = "APPROVE THE SIGN-IN IN YOUR WALLET…\nIt proves the address is yours. It authorizes nothing and moves no funds.";
  let frames = [];
  let chosen = "wingzero";
  let autoplay = false;
  let toastSeq = 0;
  let toastTimer = null;
  let newsSeq = 0;
  let newsTimer = null;
  let debriefSeq = 0;
  let debriefTimer = null;
  // Survival rules (the server's /status says): the title has no frame to choose.
  let survival = false;

  // --- Title screen. ---
  function renderFrames() {
    const box = $("frames");
    box.textContent = "";
    for (const f of frames) {
      const card = document.createElement("button");
      card.type = "button";
      card.className = "frame" + (f.slug === chosen ? " chosen" : "");
      card.setAttribute("role", "radio");
      card.setAttribute("aria-checked", String(f.slug === chosen));
      card.dataset.slug = f.slug;
      const name = document.createElement("div");
      name.className = "name";
      name.textContent = f.name;
      const des = document.createElement("div");
      des.className = "designation";
      des.textContent = f.designation + (f.zero ? " · ZERO SYSTEM" : "");
      const kit = document.createElement("div");
      kit.className = "kit";
      kit.textContent = f.weapons.join(" · ");
      card.append(name, des, kit);
      if (f.special) {
        const sp = document.createElement("div");
        sp.className = "special";
        sp.textContent = "H  " + f.special;
        card.append(sp);
      }
      card.addEventListener("click", () => {
        chosen = f.slug;
        renderFrames();
      });
      box.append(card);
    }
  }

  function renderControls(groups) {
    const box = $("controls-list");
    box.textContent = "";
    for (const g of groups) {
      const section = document.createElement("section");
      const h = document.createElement("h3");
      h.textContent = g.title;
      const table = document.createElement("table");
      for (const [keys, action] of g.rows) {
        const tr = document.createElement("tr");
        const k = document.createElement("td");
        k.className = "keys";
        k.textContent = keys;
        const a = document.createElement("td");
        a.textContent = action;
        tr.append(k, a);
        table.append(tr);
      }
      section.append(h, table);
      box.append(section);
    }
  }

  // --- The wallet: the same three calls as Gates' page (has, connect, sign). The page never writes
  // the sign-in text: the game does (bc-auth), because the server rebuilds the same bytes to
  // verify them. This only hands a finished string to the wallet, which shows it to the person
  // approving it. ---
  const short = (a) => String(a).slice(0, 6) + "…" + String(a).slice(-4);
  const WALLET_KEY = "bc.wallet";
  const wallet = {
    has: () => !!window.ethereum,
    async accounts() {
      if (!wallet.has()) return [];
      try {
        return (await window.ethereum.request({ method: "eth_accounts" })) || [];
      } catch {
        return [];
      }
    },
    async connect() {
      if (!wallet.has()) throw new Error("this browser has no wallet extension");
      const a = await window.ethereum.request({ method: "eth_requestAccounts" });
      if (!a || !a[0]) throw new Error("the wallet gave no account");
      return a[0];
    },
    // A wallet switched to another account since Connect would otherwise fail with an error
    // naming nothing anyone can act on.
    async sign(message, address) {
      const want = String(address || "").toLowerCase();
      const have = (await wallet.accounts()).map((a) => String(a).toLowerCase());
      if (want && have.length && !have.includes(want)) {
        throw new Error(`your wallet is on ${short(have[0])}, and this asks ${short(want)} to sign. ` +
          "Switch the wallet to that account and try again.");
      }
      if (want && !have.length) throw new Error("the wallet is locked or not connected to this page. Open it and try again.");
      const hex = "0x" + Array.from(new TextEncoder().encode(message))
        .map((b) => b.toString(16).padStart(2, "0")).join("");
      return window.ethereum.request({ method: "personal_sign", params: [hex, address] });
    },
  };
  window.bcWallet = wallet;

  let address = null; // the connected wallet, or null: a guest
  const remember = (on) => {
    try {
      if (on) localStorage.setItem(WALLET_KEY, "on");
      else localStorage.removeItem(WALLET_KEY);
    } catch {}
  };

  function showWallet(note) {
    const account = $("wallet-account");
    const button = $("wallet-button");
    if (!wallet.has()) {
      button.disabled = true;
      account.textContent = note || "No wallet extension here: you can fly as a guest (your suit is lost when you leave).";
    } else if (address) {
      account.textContent = note || `${short(address)} · signed in, your suit stays in the sector when you leave`;
      button.textContent = "CHANGE WALLET";
    } else {
      account.textContent = note || "Flying as a guest: your suit is lost when you leave.";
      button.textContent = "CONNECT WALLET";
    }
    account.classList.toggle("error", !!note);
    show($("guest-button"), !!address);
  }

  async function connectWallet() {
    const button = $("wallet-button");
    button.disabled = true;
    try {
      address = await wallet.connect();
      remember(true);
      showWallet();
    } catch (e) {
      // The extension talking about its own prompt: its words are the useful thing to show.
      showWallet(e && e.message ? e.message : String(e));
    }
    button.disabled = !wallet.has();
  }

  function launch(e) {
    if (e) e.preventDefault();
    send("sfx", { cue: "confirm" });
    send("play", { name: $("callsign").value.trim(), frame: chosen, address: address || "" });
    canvas()?.focus();
  }

  // --- The colony's radio (crates/bc-client/src/chat.rs): the latest lines, for a while after one
  // comes in, and while the line is open, focused to type on. ---
  let radioSeq = 0;
  let radioTimer = null;
  function renderRadio(v) {
    const box = $("chat");
    const input = $("chat-input");
    if (v.radioSeq !== radioSeq) {
      radioSeq = v.radioSeq;
      $("chat-log").replaceChildren(...(v.radio || []).map(([from, text]) => {
        const line = document.createElement("div");
        const who = document.createElement("span");
        who.className = "from";
        who.textContent = from;
        line.append(who, " ", text);
        return line;
      }));
      if (radioSeq > 0) {
        box.classList.add("recent");
        clearTimeout(radioTimer);
        radioTimer = setTimeout(() => box.classList.remove("recent"), 15000);
      }
    }
    const open = v.screen === "playing" && !!v.chat;
    box.classList.toggle("playing", v.screen === "playing");
    box.classList.toggle("open", open);
    if (open && document.activeElement !== input) {
      input.focus();
    } else if (!open && document.activeElement === input) {
      input.value = "";
      input.blur();
      canvas()?.focus();
    }
  }

  // --- The game's view. ---
  function update(v) {
    view = v;
    renderRadio(v);
    const s = v.screen;
    const onTitle = s === "title" || s === "connecting" || s === "failed";
    show($("title"), onTitle);
    // With ?autoplay the title is only the link's state: no form until something fails.
    show($("launch"), (s === "title" && !autoplay) || s === "failed");
    const linkBox = s === "connecting" || s === "failed" || (s === "title" && autoplay);
    show($("link"), linkBox);
    const msg = $("link-message");
    msg.classList.toggle("error", s === "failed");
    if (s === "connecting" && v.signing) msg.textContent = SIGNING;
    else if (s === "connecting") msg.textContent = "CONNECTING TO THE SECTOR…";
    else if (s === "failed") msg.textContent = v.message || "The link is down.";
    else msg.textContent = "";
    show($("reload"), s === "failed" && v.reload);
    show($("cancel"), s === "connecting");
    const launchButton = $("launch-button");
    launchButton.disabled = s === "connecting";
    launchButton.textContent = s === "failed" && v.retryable ? "RETRY" : "LAUNCH";
    show(launchButton, !(s === "failed" && v.reload));

    show($("banner"), s === "reconnecting");
    if (s === "reconnecting") {
      const when = v.retryIn > 0 ? `reconnecting in ${v.retryIn} s` : "reconnecting…";
      $("banner-text").textContent = `LINK LOST · ${when} (attempt ${Math.max(1, v.attempt)})` +
        (v.signing ? `\n${SIGNING}` : v.message ? `\n${v.message}` : "");
    }

    show($("pause"), s === "playing" && v.panel === "pause");
    // Signed in, leaving puts the pilot to sleep in the cockpit: in a hide spot the suit is left
    // hidden (and under survival rules it outlasts a restart), at rest on a body it's parked, in
    // the air in a body's grip it comes down and parks; a guest's suit goes with them.
    const hidden = v.signedIn && v.hideSpot && v.survival;
    $("disconnect-button").textContent = v.place === "hangar" ? "LEAVE THE BAY"
      : !v.signedIn ? "DISCONNECT"
      : hidden ? "LEAVE SUIT HIDDEN"
      : v.parkable ? "PARK & DISCONNECT"
      : "SLEEP & DISCONNECT";
    const who = address ? `Signed in as ${short(address)}. ` : "Signed in. ";
    if (v.place === "hangar") {
      $("pause-who").textContent = v.signedIn
        ? who + "Your hangar is kept while you're away: its stores, its suit, its jobs, your orders on the exchange."
        : "Playing as a guest: your hangar, and everything in it, is gone when you leave. Connect a wallet to keep it.";
    } else {
      $("pause-who").textContent = !v.signedIn
        ? "Flying as a guest: your suit is lost when you leave."
        : who + (v.hideSpot
          ? `Hidden in ${v.hideSpot}: after 8 s (60 s after a fight) it's seen only within 150 m.` +
            (v.survival ? " It survives a server restart." : "")
          : v.parkable
          ? "Your suit stays parked here while you sleep: after 8 s (60 s after a fight) sensors lose it, and eyes see it only within 400 m."
          : v.aloft
          ? "Your suit stays here while you sleep: in the grip, it settles onto the body below and parks where it lands."
          : "Your suit stays out here while you sleep, drifting on as it was. Land on a body, or rest against one, to park it.");
    }
    show($("settings"), v.panel === "settings");
    const hint = $("hint");
    hint.textContent = v.hint || "";
    show(hint, s === "playing" && !!v.hint && v.panel === "none");
    show($("help"), v.help);
    show($("map"), s === "playing" && !!v.map && v.panel === "none");
    if (s === "playing" && v.map && v.panel === "none") drawMap(v.map, v.mapGoal, v.mapFound || 0);
    show($("prompt"), s === "playing" && v.clickToFly && !v.help && !v.sequence);
    const verb = v.place === "hangar" && v.onFoot ? "WALK" : "FLY";
    $("prompt-main").textContent = v.refused ? `CLICK AGAIN TO ${verb}` : `CLICK TO ${verb}`;

    // Survival rules: on foot in the hangar bay, its line, a dot to aim by, and what can be used.
    const playing = s === "playing";
    show($("onfoot"), playing && !!v.bayLine && v.panel === "none");
    $("bay-line").textContent = v.bayLine || "";
    show($("dot"), playing && v.onFoot && !v.clickToFly);
    const use = $("use");
    use.textContent = v.prompt || "";
    show(use, playing && !!v.prompt && v.panel === "none" && !v.help);
    // A terminal: opened on the tab of the one used; the book stops coming when it closes.
    const termOpen = playing && v.panel === "terminal";
    show($("terminal"), termOpen);
    if (termOpen && !terminalShown) {
      terminalShown = true;
      openTab(v.terminal || tab);
    } else if (!termOpen && terminalShown) {
      terminalShown = false;
      watch("");
      watchBoard(false);
    }
    // A sortie's news.
    if (v.newsSeq !== newsSeq) {
      newsSeq = v.newsSeq;
      if (v.news) {
        const n = $("news");
        n.textContent = v.news;
        n.classList.toggle("bad", !!v.newsBad);
        show(n, true);
        clearTimeout(newsTimer);
        newsTimer = setTimeout(() => show(n, false), 5000);
      }
    }

    // A sortie's payout sheet: what it earned and cost, for a while under its news.
    if (v.debriefSeq !== debriefSeq) {
      debriefSeq = v.debriefSeq;
      const box = $("debrief");
      if (v.debrief && v.debrief.length) {
        const cr = (n) => `${n > 0 ? "+" : n < 0 ? "−" : ""}${Math.abs(n).toLocaleString("en-US")} CR`;
        const lines = $("debrief-lines");
        lines.replaceChildren(
          ...v.debrief.map(([what, n]) => {
            const row = document.createElement("div");
            const a = document.createElement("span");
            const b = document.createElement("span");
            a.textContent = what;
            b.textContent = cr(n);
            b.className = n >= 0 ? "earned" : "spent";
            row.append(a, b);
            return row;
          }),
        );
        const net = $("debrief-net");
        net.replaceChildren();
        const a = document.createElement("span");
        const b = document.createElement("span");
        a.textContent = "NET";
        b.textContent = cr(v.debriefNet);
        net.append(a, b);
        net.classList.toggle("bad", v.debriefNet < 0);
        show(box, true);
        clearTimeout(debriefTimer);
        debriefTimer = setTimeout(() => show(box, false), 12000);
      } else {
        show(box, false);
      }
    }
    // A panel opened over it puts it away.
    if (v.panel !== "none") show($("debrief"), false);

    if (v.toastSeq !== toastSeq) {
      toastSeq = v.toastSeq;
      // Over a terminal its log says it (the toast would cover its tabs).
      if (v.toast && v.panel !== "terminal") {
        const t = $("toast");
        t.textContent = v.toast;
        show(t, true);
        clearTimeout(toastTimer);
        toastTimer = setTimeout(() => show(t, false), 2500);
      }
    }
  }

  // --- The colony's map (M): the strip the pilot is on, drawn from the city's tables (`init`). ---
  let city = null;
  // A canvas the page's width, `data-h` CSS pixels tall, at the screen's resolution.
  function sizeCanvas(c) {
    const dpr = window.devicePixelRatio || 1;
    const cssH = Number(c.dataset.h);
    c.style.height = `${cssH}px`;
    const w = Math.round(c.clientWidth * dpr), h = Math.round(cssH * dpr);
    if (w > 0 && (c.width !== w || c.height !== h)) {
      c.width = w;
      c.height = h;
    }
    return c.getContext("2d");
  }
  function drawMap(at, goal, found) {
    if (!city) return;
    const strip = city.strips[at.strip] || city.strips[0];
    const district = strip.districts.find((d) => at.x >= d.x && at.x < d.x1);
    $("map-title").textContent = `THE FIRST COLONY · ${strip.name}` + (district ? ` · ${district.name}` : "");
    const css = getComputedStyle(document.documentElement);
    const cyan = css.getPropertyValue("--cyan").trim();
    const amber = css.getPropertyValue("--amber").trim();
    const label = css.getPropertyValue("--label").trim();
    const font = (px) => `${px}px ${css.getPropertyValue("--label-font")}`;
    const dpr = window.devicePixelRatio || 1;

    // The whole strip, end to end: its districts, and where the pilot is.
    const top = sizeCanvas($("map-strip"));
    const tw = top.canvas.width, th = top.canvas.height;
    const tx = (x) => ((x - city.x0) / (city.x1 - city.x0)) * tw;
    top.clearRect(0, 0, tw, th);
    strip.districts.forEach((d, i) => {
      top.fillStyle = d.x === strip.districts[strip.districts.length - 1].x ? "rgba(255,181,71,0.18)"
        : i % 2 ? "rgba(159,198,230,0.16)" : "rgba(159,198,230,0.28)";
      top.fillRect(tx(d.x), 0, tx(d.x1) - tx(d.x), th);
    });
    top.fillStyle = cyan;
    top.fillRect(tx(at.x) - 1.5 * dpr, 0, 3 * dpr, th);

    // Round the pilot: the strip's whole width across (+s up, as seen from the axis), kilometres along.
    const g = sizeCanvas($("map-near"));
    const W = g.canvas.width, H = g.canvas.height;
    const k = H / city.width;
    const span = W / k;
    const x0 = Math.min(Math.max(at.x - span / 2, city.x0), city.x1 - span);
    const X = (x) => (x - x0) * k;
    const Y = (s) => H - s * k;
    g.clearRect(0, 0, W, H);
    // The blocks between the streets, the window banks green.
    g.fillStyle = "rgba(159,198,230,0.07)";
    g.fillRect(0, 0, W, H);
    g.fillStyle = "rgba(141,255,168,0.12)";
    const bank = (city.width - city.avenue) / 2 - city.rows * city.block;
    g.fillRect(0, Y(bank), W, bank * k);
    g.fillRect(0, 0, W, bank * k);
    g.strokeStyle = "rgba(4,10,18,0.9)";
    g.lineWidth = Math.max(1, 0.8 * dpr);
    for (let bx = Math.floor((x0 - city.gridX0) / city.block); ; bx++) {
      const x = city.gridX0 + bx * city.block;
      if (X(x) > W) break;
      if (x < city.x0 || x > city.x1) continue;
      g.beginPath(); g.moveTo(X(x), Y(bank)); g.lineTo(X(x), Y(city.width - bank)); g.stroke();
    }
    for (let r = 0; r <= city.rows; r++) {
      for (const sign of [-1, 1]) {
        const s = city.width / 2 + sign * (city.avenue / 2 + r * city.block);
        g.beginPath(); g.moveTo(0, Y(s)); g.lineTo(W, Y(s)); g.stroke();
      }
    }
    // Hub Gate's square, the avenue and the canal.
    const [q0, q1, qx0, qx1] = city.square;
    g.fillStyle = "rgba(238,244,251,0.18)";
    g.fillRect(X(qx0), Y(q1), (qx1 - qx0) * k, (q1 - q0) * k);
    g.fillStyle = "rgba(238,244,251,0.35)";
    g.fillRect(0, Y(city.width / 2 + city.avenue / 2), W, Math.max(2 * dpr, city.avenue * k));
    g.fillStyle = "rgba(90,170,230,0.55)";
    g.fillRect(0, Y(city.canal[1]), W, Math.max(2 * dpr, (city.canal[1] - city.canal[0]) * k));
    // District lines and names.
    g.font = font(11 * dpr);
    g.textBaseline = "top";
    for (const d of strip.districts) {
      if (X(d.x1) < 0 || X(d.x) > W) continue;
      g.strokeStyle = "rgba(159,198,230,0.5)";
      g.beginPath(); g.moveTo(X(d.x), 0); g.lineTo(X(d.x), H); g.stroke();
      g.fillStyle = label;
      g.fillText(d.name, Math.max(X(d.x), 0) + 6 * dpr, 6 * dpr);
    }
    // Sights and places: a sight found is ticked off, one still to find is a ring.
    const mark = (p, colour, size, name = p.name, ring = false) => {
      if (X(p.x) < -40 || X(p.x) > W + 40) return;
      g.fillStyle = colour;
      g.strokeStyle = colour;
      g.lineWidth = 1.5 * dpr;
      g.beginPath(); g.arc(X(p.x), Y(p.s), size * dpr, 0, Math.PI * 2);
      if (ring) g.stroke(); else g.fill();
      g.fillText(name, X(p.x) + (size + 4) * dpr, Y(p.s) - 6 * dpr);
    };
    const got = (p) => ((found >>> p.i) & 1) === 1;
    for (const p of strip.sights) {
      if (got(p)) mark(p, cyan, 3.5, `\u2713 ${p.name}`);
      else mark(p, label, 3.5, p.name, true);
    }
    for (const p of strip.places) mark(p, amber, 4.5);
    // The objective's door: a ◆ over it (and pointing the way when it's off the map's edge).
    const target = strip.places.find((p) => p.slug === goal);
    if (target) {
      const pink = css.getPropertyValue("--pink").trim();
      const gx = Math.min(Math.max(X(target.x), 14 * dpr), W - 14 * dpr), gy = Y(target.s);
      const d = 8 * dpr;
      g.strokeStyle = pink;
      g.lineWidth = 2 * dpr;
      g.beginPath();
      g.moveTo(gx, gy - d); g.lineTo(gx + d, gy); g.lineTo(gx, gy + d); g.lineTo(gx - d, gy);
      g.closePath();
      g.stroke();
    }
    // The found-list: every strip's sights, those found by name.
    const all = city.strips.flatMap((k) => k.sights);
    const have = all.filter(got);
    $("map-sights").textContent = `SIGHTS FOUND ${have.length}/${all.length}` +
      (have.length ? ` · ${have.map((p) => p.name).join(" · ")}` : " · WALK THE CITY TO FIND THEM");
    // The pilot, and the way they face.
    const px = X(at.x), py = Y(at.s), r = 9 * dpr;
    const c = Math.cos(at.heading), sn = Math.sin(at.heading);
    g.fillStyle = cyan;
    g.beginPath();
    g.moveTo(px + c * r, py + sn * r);
    g.lineTo(px - c * r * 0.6 - sn * r * 0.55, py - sn * r * 0.6 + c * r * 0.55);
    g.lineTo(px - c * r * 0.6 + sn * r * 0.55, py - sn * r * 0.6 - c * r * 0.55);
    g.closePath();
    g.fill();
  }

  function init(data) {
    city = data.city || null;
    frames = data.frames || [];
    autoplay = !!data.autoplay;
    if (data.frame && frames.some((f) => f.slug === data.frame)) chosen = data.frame;
    if (data.name) $("callsign").value = data.name;
    renderFrames();
    renderControls(data.controls || []);
    update(view);
    if (!autoplay) $("callsign").focus();
  }

  // --- Settings: built once, then only the values change (so a dragged slider isn't rebuilt
  // under the pointer). ---
  const settingRows = new Map();
  function renderSettings(data) {
    const box = $("settings-list");
    if (!settingRows.size) {
      let group = null;
      let table = null;
      for (const r of data.rows) {
        if (r.group !== group) {
          group = r.group;
          const section = document.createElement("section");
          const h = document.createElement("h3");
          h.textContent = group;
          table = document.createElement("div");
          table.className = "setting-rows";
          section.append(h, table);
          box.append(section);
        }
        const row = document.createElement("label");
        row.className = "setting";
        const name = document.createElement("span");
        name.textContent = r.label;
        const value = document.createElement("span");
        value.className = "value";
        let input;
        if (r.kind === "range") {
          input = document.createElement("input");
          input.type = "range";
          input.min = r.min;
          input.max = r.max;
          input.step = r.step;
          input.addEventListener("input", () => send("set", { key: r.key, value: input.value }));
        } else if (r.kind === "toggle") {
          input = document.createElement("input");
          input.type = "checkbox";
          input.addEventListener("change", () => send("set", { key: r.key, value: String(input.checked) }));
        } else {
          input = document.createElement("select");
          for (const c of r.choices) {
            const o = document.createElement("option");
            o.value = c;
            o.textContent = c.toUpperCase();
            input.append(o);
          }
          input.addEventListener("change", () => send("set", { key: r.key, value: input.value }));
        }
        input.dataset.key = r.key;
        row.append(name, input, value);
        table.append(row);
        settingRows.set(r.key, { input, value, kind: r.kind });
      }
    }
    // The game's field of view is vertical; other games mostly quote the horizontal one, so give
    // both (horizontal at this window's shape).
    const fovText = (v) => {
      const aspect = window.innerWidth / Math.max(1, window.innerHeight);
      const h = (2 * Math.atan(Math.tan((v * Math.PI) / 360) * aspect) * 180) / Math.PI;
      return `${v}° V\n${Math.round(h)}° H`;
    };
    for (const r of data.rows) {
      const row = settingRows.get(r.key);
      if (!row) continue;
      if (row.kind === "range") {
        if (document.activeElement !== row.input) row.input.value = r.value;
        const n = Number(r.value);
        row.value.textContent = r.key === "fov" ? fovText(n) : r.max <= 1 ? `${Math.round(n * 100)}%` : `${n.toFixed(2)}×`;
      } else if (row.kind === "toggle") {
        row.input.checked = r.value === "true";
        row.value.textContent = r.value === "true" ? "ON" : "OFF";
      } else {
        row.input.value = r.value;
        row.value.textContent = "";
      }
    }
  }

  // --- The hangar's terminals (survival rules). The game sends the catalogue once and the pilot's
  // hangar whenever the server's word on it changes (crates/bc-client/src/terminal.rs); what the
  // pilot asks for goes back as {cmd: "hangar", req}, a request the server checks and answers. ---
  let catalogue = { items: [], recipes: [], lines: [], parts: [], systems: [], module_mounts: [], fee_bp: 0 };
  const items = new Map();
  const lines = new Map();
  let hangar = null;
  let tab = "fabricator";
  let terminalShown = false;
  let fabFilter = "materials";
  let exFilter = "goods";
  let watching = "";
  let side = "buy";
  // What the pilot is typing, by field (a re-render keeps it).
  const drafts = {};
  const ask = (req) => send("hangar", { req });

  const esc = (t) => String(t ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
  const fmt = (n) => Number(n || 0).toLocaleString("en-US");
  const nameOf = (slug) => items.get(slug)?.name || slug;
  const isBulk = (slug) => !!items.get(slug)?.bulk;
  const amount = (slug, qty) => (isBulk(slug) ? `${fmt(qty)} kg` : fmt(qty));
  const unit = (slug) => (isBulk(slug) ? "CR/t" : "CR");
  const worth = (slug, price, qty) => Math.floor(isBulk(slug) ? (price * qty) / 1000 : price * qty);
  const secs = (s) =>
    s >= 3600 ? `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m` : s >= 60 ? `${Math.floor(s / 60)}m ${s % 60}s` : `${s}s`;
  const partName = (slug) => catalogue.parts.find((p) => p.slug === slug)?.name || slug;
  const stockOf = (slug) => {
    const v = hangar?.view;
    if (!v) return 0;
    const it = items.get(slug);
    if (it?.kind === "part") return v.parts.filter((u) => u.line === it.line && u.part === it.part).length;
    const e = v.stock.find(([i]) => i === slug);
    return e ? e[1] : 0;
  };
  const draft = (key, dflt) => (key in drafts ? drafts[key] : dflt);
  const button = (label, attrs, cls) =>
    `<button type="button" ${cls ? `class="${cls}"` : ""} ${Object.entries(attrs).map(([k, v]) => `data-${k}="${esc(v)}"`).join(" ")}>${label}</button>`;
  const input = (key, value, extra) => `<input data-key="${esc(key)}" value="${esc(value)}" ${extra || ""}>`;
  const systemName = (slug) => catalogue.systems.find((x) => x.slug === slug)?.name || slug;
  // What's damaged or failed inside a part, as a line under it.
  const faultNote = (faults) => {
    const list = Object.entries(faults || {});
    if (!list.length) return "";
    return `<div class="note fault">${list.map(([sys, level]) => `${esc(systemName(sys))} <b class="${level}">${level.toUpperCase()}</b>`).join(" · ")}</div>`;
  };
  const bar = (pct) => `<span class="bar ${pct < 35 ? "bad" : pct < 75 ? "worn" : ""}"><i style="width:${Math.max(2, pct)}%"></i></span>${pct}%`;

  function catalogueIn(json) {
    catalogue = JSON.parse(json);
    items.clear();
    for (const it of catalogue.items) items.set(it.slug, it);
    lines.clear();
    for (const l of catalogue.lines) lines.set(l.slug, l);
    if (terminalShown) renderTerminal();
  }

  function hangarIn(json) {
    hangar = JSON.parse(json);
    if (terminalShown) renderTerminal();
  }

  // The fabricator: what can be made (materials, each line's parts, weapons), and the queues.
  function renderFabricator() {
    const filters = [["materials", "MATERIALS"], ...catalogue.lines.map((l) => [l.slug, l.name.toUpperCase()]), ["weapons", "WEAPONS"], ["modules", "EQUIPMENT"], ["kits", "CONSUMABLES"]];
    let out = `<div class="filters">${filters
      .map(([f, label]) => button(label, { act: "fab-filter", f }, fabFilter === f ? "active" : ""))
      .join("")}</div>`;
    const shown = catalogue.recipes.filter((r) => {
      const it = items.get(r.output);
      if (!it) return false;
      if (fabFilter === "materials") return it.kind === "material";
      if (fabFilter === "weapons") return it.kind === "weapon";
      if (fabFilter === "modules") return it.kind === "module";
      if (fabFilter === "kits") return it.kind === "kit";
      return it.kind === "part" && it.line === fabFilter;
    });
    const line = lines.get(fabFilter);
    if (line?.gundam) {
      out += `<div class="status warn">GUNDAM TECHNOLOGY. Its armour is gundanium, which only the colony's zero-G foundry can make, and the colony won't trade it: OZ is hunting for it. Only pilots buy and sell it.</div>`;
    }
    out += `<table><tr><th>MAKES</th><th>A BATCH NEEDS</th><th>WHERE</th><th class="num">TIME</th><th class="num">BATCHES</th><th></th></tr>`;
    for (const r of shown) {
      const key = `batches:${r.output}`;
      const n = Math.max(1, Math.floor(Number(draft(key, "1")) || 1));
      const needs = r.inputs
        .map(([slug, q]) => `<span class="${stockOf(slug) >= q * n ? "have" : "short"}">${amount(slug, q)} ${esc(nameOf(slug))}</span>`)
        .join(", ");
      const fee = r.fee ? ` · ${fmt(r.fee)} CR` : "";
      const g = items.get(r.output)?.gundam ? " gundam" : "";
      const out_ = items.get(r.output);
      const what = out_?.summary
        ? `<div class="note">${esc(out_.summary)}${out_.part ? ` · fits the ${esc(partName(out_.part))}` : ""}</div>`
        : "";
      out += `<tr><td class="${g}">${amount(r.output, r.makes)} ${esc(nameOf(r.output))}${what}</td><td>${needs}</td>` +
        `<td class="dim">${esc(r.station_name)}${fee}</td><td class="num">${secs(r.secs)}</td>` +
        `<td class="num">${input(key, draft(key, "1"), 'inputmode="numeric" size="4"')}</td>` +
        `<td class="act">${button("MAKE", { act: "make", item: r.output })}</td></tr>`;
    }
    out += `</table>`;
    const jobs = hangar?.view?.jobs || [];
    out += `<section><h3>QUEUES</h3>`;
    if (!jobs.length) out += `<div class="note">Nothing being made.</div>`;
    else {
      out += `<table><tr><th>WHERE</th><th>MAKING</th><th class="num">BATCHES</th><th class="num">DONE IN</th><th></th></tr>`;
      const index = {};
      for (const j of jobs) {
        const k = (index[j.station] = (index[j.station] ?? -1) + 1);
        out += `<tr><td class="dim">${esc(j.station === "foundry" ? "Zero-G foundry" : "Fabricator")}</td>` +
          `<td>${esc(nameOf(j.item))}</td><td class="num">${j.done}/${j.batches}</td><td class="num">${secs(j.secs_left)}</td>` +
          `<td class="act">${button("CANCEL", { act: "cancel-job", station: j.station, index: k })}</td></tr>`;
      }
      out += `</table>`;
    }
    return out + `</section>`;
  }

  // The stores: bulk goods, weapons, and parts one by one.
  function renderStores() {
    const v = hangar?.view;
    if (!v) return `<div class="note">Waiting for the stores' inventory…</div>`;
    const fits = new Set(hangar.console?.fits || []);
    let out = `<section><h3>GOODS</h3><table><tr><th>ITEM</th><th class="num">HELD</th><th class="num">COLONY VALUE</th><th></th></tr>`;
    const goods = v.stock.filter(([slug]) => isBulk(slug));
    if (!goods.length) out += `<tr><td colspan="4" class="dim">None.</td></tr>`;
    for (const [slug, qty] of goods) {
      out += `<tr><td>${esc(nameOf(slug))}</td><td class="num">${amount(slug, qty)}</td>` +
        `<td class="num dim">${fmt(items.get(slug)?.value)} ${unit(slug)}</td>` +
        `<td class="act">${button("SELL", { act: "sell", item: slug })}</td></tr>`;
    }
    out += `</table></section><section><h3>WEAPONS</h3><table><tr><th>ITEM</th><th class="num">HELD</th><th></th></tr>`;
    const weapons = v.stock.filter(([slug]) => items.get(slug)?.kind === "weapon");
    if (!weapons.length) out += `<tr><td colspan="3" class="dim">None.</td></tr>`;
    for (const [slug, qty] of weapons) {
      out += `<tr><td>${esc(nameOf(slug))}</td><td class="num">${fmt(qty)}</td><td class="act">` +
        (fits.has(slug) ? button("FIT", { act: "fit", item: slug }) + " " : "") +
        button("SCRAP", { act: "scrap", item: slug }, "danger") + " " + button("SELL", { act: "sell", item: slug }) + `</td></tr>`;
    }
    out += `</table></section><section><h3>EQUIPMENT</h3><table><tr><th>MODULE</th><th class="num">HELD</th><th></th></tr>`;
    const gear = v.stock.filter(([slug]) => items.get(slug)?.kind === "module");
    if (!gear.length) out += `<tr><td colspan="3" class="dim">None.</td></tr>`;
    for (const [slug, qty] of gear) {
      out += `<tr><td>${esc(nameOf(slug))}<div class="note">${esc(items.get(slug)?.summary)}</div></td><td class="num">${fmt(qty)}</td><td class="act">` +
        (fits.has(slug) ? button("FIT", { act: "fit", item: slug }) + " " : "") +
        button("SCRAP", { act: "scrap", item: slug }) + " " + button("SELL", { act: "sell", item: slug }) + `</td></tr>`;
    }
    out += `</table></section><section><h3>CONSUMABLES</h3><div class="note">The suit's rack takes up to 3 of each at launch (keys 1–4 in flight); what's left comes home.</div><table><tr><th>ITEM</th><th class="num">HELD</th><th></th></tr>`;
    const kits = v.stock.filter(([slug]) => items.get(slug)?.kind === "kit");
    if (!kits.length) out += `<tr><td colspan="3" class="dim">None.</td></tr>`;
    for (const [slug, qty] of kits) {
      out += `<tr><td>${esc(nameOf(slug))}<div class="note">${esc(items.get(slug)?.summary)}</div></td><td class="num">${fmt(qty)}</td><td class="act">` +
        button("SELL", { act: "sell", item: slug }) + `</td></tr>`;
    }
    out += `</table></section><section><h3>PARTS</h3><table><tr><th>PART</th><th>CONDITION</th><th></th></tr>`;
    const parts = [...v.parts].sort((a, b) => (a.line + a.part).localeCompare(b.line + b.part) || b.condition - a.condition);
    if (!parts.length) out += `<tr><td colspan="3" class="dim">None.</td></tr>`;
    for (const u of parts) {
      const slug = `part.${u.line}.${u.part}`;
      out += `<tr><td class="${items.get(slug)?.gundam ? "gundam" : ""}">${esc(nameOf(slug))}${faultNote(u.faults)}</td><td>${bar(u.condition)}</td><td class="act">` +
        (fits.has(slug) ? button("FIT", { act: "fit", item: slug }) + " " : "") +
        button("SCRAP", { act: "scrap", item: slug }, "danger") + " " + button("SELL", { act: "sell", item: slug }) + `</td></tr>`;
    }
    return out + `</table></section>`;
  }

  // The suit's maintenance console: what's fitted, what could be, repairs, and the launch check.
  function renderSuit() {
    const v = hangar?.view;
    if (!v) return `<div class="note">Waiting for the bay…</div>`;
    const con = hangar.console || {};
    const fits = new Set(con.fits || []);
    const bay = v.bay;
    if (bay.state === "out") {
      return `<div class="status warn">YOUR ${esc(lines.get(bay.suit.line)?.name.toUpperCase())} IS OUT IN THE SECTOR. Bring it home: at rest inside the dock's ring of lights, press Enter.</div>`;
    }
    if (bay.state === "empty") {
      let out = `<div class="status bad">THE GANTRY IS EMPTY. A suit starts with its torso: fit one from the stores, or make one at the fabricator.</div><div class="slots">`;
      for (const slug of fits) {
        out += `<div class="slot"><div class="what">${esc(nameOf(slug))}</div><div>${button("FIT: A NEW SUIT", { act: "fit", item: slug })}</div></div>`;
      }
      return out + `</div>`;
    }
    const suit = bay.suit;
    const line = lines.get(suit.line);
    let out = con.launch
      ? `<div class="status bad">NOT READY TO LAUNCH: ${esc(con.launch)}.</div>`
      : `<div class="status">READY TO LAUNCH. Climb the stairs to the catwalk and board at the cockpit hatch (E). The tank and the magazines are topped up from the stores as it goes.</div>`;
    out += `<h3>${esc(line?.name.toUpperCase() || suit.line)}${line?.gundam ? " · GUNDAM" : ""}</h3><div class="slots">`;
    catalogue.parts.forEach((p, k) => {
      const c = suit.parts[k];
      const slug = `part.${suit.line}.${p.slug}`;
      if (c == null) {
        out += `<div class="slot empty"><div class="what">${esc(p.name.toUpperCase())}: NOT FITTED</div><div>` +
          (fits.has(slug) ? button("FIT", { act: "fit", item: slug }) : `<span class="note">none in the stores</span>`) + `</div></div>`;
        return;
      }
      const rep = (con.repairs || []).find((r) => r.part === p.slug);
      const cost = rep ? rep.cost.map(([s, q]) => `${amount(s, q)} ${esc(nameOf(s))}`).join(", ") : "";
      // Its systems: working, or what's wrong and what restoring it takes.
      const inside = catalogue.systems.filter((x) => x.part === p.slug);
      const broken = (con.overhauls || []).filter((o) => o.part === p.slug);
      const tags = inside.map((x) => {
        const o = broken.find((b) => b.system === x.slug);
        return `<span class="sys ${o ? o.level : "ok"}" title="${esc(x.name)}${o ? ": " + o.level : ""}">${esc(x.tag)}</span>`;
      }).join(" ");
      const ohCost = broken.map((o) => `${esc(systemName(o.system))}: ${o.cost.map(([s, q]) => `${amount(s, q)} ${esc(nameOf(s))}`).join(", ")}`).join("; ");
      out += `<div class="slot"><div class="what">${esc(p.name.toUpperCase())}</div><div>${bar(c)}</div><div class="systems">${tags}</div><div>` +
        (rep ? button("REPAIR", { act: "repair", part: p.slug }) + " " : "") +
        (broken.length ? button("OVERHAUL", { act: "overhaul", part: p.slug }) + " " : "") +
        button("STRIP", { act: "strip-part", part: p.slug }) + `</div>` +
        (rep ? `<div class="note">repair: ${cost}</div>` : "") +
        (broken.length ? `<div class="note">overhaul: ${ohCost}</div>` : "") + `</div>`;
    });
    // Equipment, on its parts' mounts.
    (catalogue.module_mounts || []).forEach((part, k) => {
      const kind = (suit.modules || [])[k];
      if (suit.parts[catalogue.parts.findIndex((p) => p.slug === part)] == null) return;
      if (!kind) {
        const can = [...fits].filter((slug) => items.get(slug)?.kind === "module" && items.get(slug)?.part === part);
        out += `<div class="slot empty"><div class="what">${esc(partName(part).toUpperCase())} MOUNT: EMPTY</div><div>` +
          (can.length ? can.map((slug) => button(`FIT ${esc(nameOf(slug).toUpperCase())}`, { act: "fit", item: slug })).join(" ") : `<span class="note">no equipment for it in the stores</span>`) +
          `</div></div>`;
        return;
      }
      const slug = `module.${kind}`;
      out += `<div class="slot"><div class="what">${esc(nameOf(slug).toUpperCase())}</div><div class="note">${esc(items.get(slug)?.summary)}</div><div>` +
        button("STRIP", { act: "strip-module", module: k }) + `</div></div>`;
    });
    (line?.mounts || []).forEach((m, k) => {
      if (!m) return;
      if (!suit.mounts[k]) {
        out += `<div class="slot empty"><div class="what">${esc(m.name.toUpperCase())}: NOT FITTED</div><div>` +
          (fits.has(m.weapon) ? button("FIT", { act: "fit", item: m.weapon }) : `<span class="note">none in the stores</span>`) + `</div></div>`;
        return;
      }
      const rounds = m.rounds ? `${fmt(suit.ammo[k])}/${fmt(m.rounds)} rounds` : "no rounds to load";
      out += `<div class="slot"><div class="what">${esc(m.name.toUpperCase())}</div><div class="note">${rounds}</div><div>` +
        button("STRIP", { act: "strip-mount", mount: k }) + `</div></div>`;
    });
    const tank = line?.tank || 0;
    const stores = stockOf("mat.propellant");
    out += `<div class="slot"><div class="what">PROPELLANT</div><div>${bar(Math.round((100 * suit.propellant) / Math.max(1, tank)))}</div>` +
      `<div class="note">${fmt(suit.propellant)}/${fmt(tank)} kg · ${fmt(stores)} kg in the stores</div></div></div>`;
    const st = con.stats;
    if (st) {
      const row = (k, v) => `<div><span class="dim">${k}</span> ${v}</div>`;
      out += `<section class="stats"><h3>AS IT WOULD LAUNCH</h3><div class="statgrid">` +
        row("DELTA-V", `${fmt(Math.round(st.delta_v))} m/s`) +
        row("ACCEL", `${st.accel_g.toFixed(1)} g (boost ${st.boost_g.toFixed(1)} g)`) +
        row("MASS", `${fmt(st.mass_kg)} kg`) +
        row("TANK", `${fmt(st.tank_kg)} kg`) +
        row("SENSORS", `${(st.sensor_m / 1000).toFixed(1)} km`) +
        row("SIGNATURE", `×${st.signature.toFixed(2)}`) +
        row("ENERGY", `${fmt(Math.round(st.energy))} (+${st.regen.toFixed(1)}/s)`) +
        row("HEAT SHED", `${st.heat.toFixed(1)}/s`) +
        row("HOLD", `${fmt(st.hold_kg)} kg`) +
        row("PILOT BEARS", `${st.g_tolerance.toFixed(1)} g`) +
        row("DAMAGE TAKEN", `×${st.armour.toFixed(2)}`) +
        `</div></section>`;
    }
    // Wear from use: how far through its service life each worn system is.
    const wear = con.wear || [];
    if (wear.length) {
      const serviceCost = (con.service_cost || []).map(([s, q]) => `${amount(s, q)} ${esc(nameOf(s))}`).join(", ");
      out += `<section><h3>SERVICE LIFE</h3><table>` + wear.map((w) => {
        const pct = Math.min(100, Math.round(w.used * 100));
        return `<tr><td>${esc(systemName(w.system))}</td><td><span class="bar ${pct >= 75 ? "bad" : pct >= 40 ? "worn" : ""}"><i style="width:${Math.max(2, pct)}%"></i></span>${pct}% used</td></tr>`;
      }).join("") + `</table><div class="note">Thruster hours, rounds fired and overheats wear systems down: at 100% one comes home a level worse. From ${con.service_from_pct}% OVERHAUL services it (${serviceCost} each) and its life starts again.</div></section>`;
    }
    const serviceable = wear.some((w) => w.used * 100 >= (con.service_from_pct || 100));
    out += `<div class="row">` + ((con.repairs || []).length ? button("REPAIR ALL", { act: "repair", part: "" }) : "") +
      ((con.overhauls || []).length || serviceable ? button("OVERHAUL ALL", { act: "overhaul", part: "" }) : "") +
      button("DISMANTLE", { act: "dismantle" }, "danger") + `<span class="note">Dismantling puts everything back in the stores.</span></div>`;
    return out;
  }

  // The Colony Exchange: quotes, the watched item's book and history, the order form, and the
  // pilot's orders.
  function renderExchange() {
    const m = hangar?.market;
    if (!m) return `<div class="note">Waiting for the exchange…</div>`;
    const filters = [["goods", "RAW & MATERIALS"], ["parts", "PARTS"], ["weapons", "WEAPONS"], ["modules", "EQUIPMENT"], ["kits", "CONSUMABLES"]];
    let left = `<div class="filters">${filters
      .map(([f, label]) => button(label, { act: "ex-filter", f }, exFilter === f ? "active" : ""))
      .join("")}</div><table><tr><th>ITEM</th><th class="num">BID</th><th class="num">ASK</th><th class="num">LAST</th><th class="num">VOL</th></tr>`;
    for (const q of m.quotes) {
      const it = items.get(q.item);
      if (!it) continue;
      const kind = it.kind === "ore" || it.kind === "material" ? "goods" : it.kind === "part" ? "parts" : it.kind === "module" ? "modules" : it.kind === "kit" ? "kits" : "weapons";
      if (kind !== exFilter) continue;
      const quiet = q.bid == null && q.ask == null && !q.volume && !stockOf(q.item);
      if (kind === "parts" && quiet) continue;
      left += `<tr class="pick${q.item === watching ? " chosen" : ""}" data-act="watch" data-item="${esc(q.item)}">` +
        `<td class="${it.gundam ? "gundam" : ""}">${esc(it.name)}</td>` +
        `<td class="num bid">${q.bid != null ? fmt(q.bid) : "-"}</td><td class="num ask">${q.ask != null ? fmt(q.ask) : "-"}</td>` +
        `<td class="num">${q.last != null ? fmt(q.last) : "-"}</td><td class="num dim">${fmt(q.volume)}</td></tr>`;
    }
    left += `</table><div class="note">Prices in credits a tonne for goods, a piece for parts and weapons.</div>`;

    let right = `<div class="note">Pick something to trade.</div>`;
    if (watching && items.has(watching)) {
      const it = items.get(watching);
      const q = m.quotes.find((x) => x.item === watching) || {};
      const book = hangar.book && hangar.book.depth.item === watching ? hangar.book : null;
      right = `<h3>${esc(it.name.toUpperCase())}</h3><div class="note">` +
        (it.gundam ? "Gundam technology: pilots only; the colony won't touch it." : it.colony_buys || it.colony_sells
          ? `The colony ${it.colony_buys && it.colony_sells ? "buys and sells" : it.colony_buys ? "buys" : "sells"} it; its prices follow its stock.`
          : "Pilots only.") + ` You hold ${amount(watching, stockOf(watching))}.</div>`;
      const hist = book?.history || [];
      if (hist.length > 1) {
        const lo = Math.min(...hist), hi = Math.max(...hist), span = Math.max(1, hi - lo);
        const pts = hist.map((p, i) => `${((i / (hist.length - 1)) * 100).toFixed(1)},${(55 - ((p - lo) / span) * 50).toFixed(1)}`).join(" ");
        right += `<svg class="spark" viewBox="0 0 100 60" preserveAspectRatio="none"><polyline points="${pts}" fill="none" style="stroke: var(--cyan)" stroke-width="1" vector-effect="non-scaling-stroke"/></svg>` +
          `<div class="note">last hour: ${fmt(lo)}–${fmt(hi)} ${unit(watching)}</div>`;
      }
      const levels = (list, cls) =>
        `<table>${(list || []).slice(0, 8).map((l) => `<tr><td class="num ${cls}">${fmt(l.price)}</td><td class="num">${amount(watching, l.qty)}</td><td class="dim">${l.colony ? "colony" : ""}</td></tr>`).join("") || `<tr><td class="dim">none</td></tr>`}</table>`;
      right += `<div class="book"><div><h3>BIDS</h3>${levels(book?.depth.bids, "bid")}</div><div><h3>ASKS</h3>${levels(book?.depth.asks, "ask")}</div></div>`;
      const dfltPrice = side === "buy" ? q.ask ?? q.last ?? it.value : q.bid ?? q.last ?? it.value;
      const priceKey = `price:${side}:${watching}`;
      const qtyKey = `qty:${side}:${watching}`;
      const dfltQty = side === "sell" ? Math.min(stockOf(watching), it.bulk ? 1000 : 1) || (it.bulk ? 1000 : 1) : it.bulk ? 1000 : 1;
      const price = Math.max(0, Math.floor(Number(draft(priceKey, String(dfltPrice || 0))) || 0));
      const qty = Math.max(0, Math.floor(Number(draft(qtyKey, String(dfltQty))) || 0));
      const total = worth(watching, price, qty);
      const fee = side === "sell" ? Math.floor((total * (m.fee_bp || 0)) / 10000) : 0;
      right += `<div class="form">${button("BUY", { act: "side", side: "buy" }, side === "buy" ? "primary" : "")}` +
        button("SELL", { act: "side", side: "sell" }, side === "sell" ? "primary" : "") + `</div>` +
        `<div class="form"><label>PRICE ${input(priceKey, draft(priceKey, String(dfltPrice || 0)), 'inputmode="numeric"')} ${unit(watching)}</label>` +
        `<label>${it.bulk ? "KG" : "PIECES"} ${input(qtyKey, draft(qtyKey, String(dfltQty)), 'inputmode="numeric"')}</label>` +
        `<label><input type="checkbox" data-key="rest" ${draft("rest", true) ? "checked" : ""}> REST ON THE BOOK</label></div>` +
        `<div class="form">${button(side === "buy" ? "PLACE BUY ORDER" : "PLACE SELL ORDER", { act: "order" }, "primary")}` +
        `<span class="note">${side === "buy" ? `up to ${fmt(total)} CR held until it fills` : `${fmt(total)} CR less a ${fmt(fee)} CR fee`}` +
        `${draft("rest", true) ? "; what doesn't fill at once waits on the book" : "; what doesn't fill at once is handed back"}</span></div>`;
    }
    let orders = `<section><h3>YOUR ORDERS</h3>`;
    if (!m.orders.length) orders += `<div class="note">None resting.</div>`;
    else {
      orders += `<table><tr><th>ITEM</th><th>SIDE</th><th class="num">PRICE</th><th class="num">LEFT</th><th></th></tr>`;
      for (const o of m.orders) {
        orders += `<tr><td>${esc(nameOf(o.item))}</td><td class="${o.side === "buy" ? "bid" : "ask"}">${o.side.toUpperCase()}</td>` +
          `<td class="num">${fmt(o.price)} ${unit(o.item)}</td><td class="num">${amount(o.item, o.qty)}</td>` +
          `<td class="act">${button("CANCEL", { act: "cancel-order", id: o.id })}</td></tr>`;
      }
      orders += `</table>`;
    }
    orders += `</section>`;
    return `<div class="split"><div>${left}</div><div>${right}${orders}</div></div>`;
  }

  // The Charter Board: contracts (the colony's, the militia's, other pilots'), posting one, the
  // colony's great works and the charter vote. Rewards are the server's: a pilot's are in escrow.
  const workItems = (w) => w.needs.map(([item]) => item);
  function contractRow(c) {
    const t = c.task;
    let job, act = "";
    if (t.kind === "supply") {
      const left = t.qty - t.delivered;
      job = `${esc(nameOf(t.item))}: ${amount(t.item, t.qty)}` +
        (t.delivered ? ` <span class="dim">(${amount(t.item, t.delivered)} in)</span>` : "");
      if (c.mine) act = button("WITHDRAW", { act: "withdraw", id: c.id });
      else {
        const key = `deliver:${c.id}`;
        const dflt = Math.min(left, stockOf(t.item));
        act = `${input(key, draft(key, String(dflt)), 'inputmode="numeric" class="narrow"')}` +
          button("DELIVER", { act: "deliver", id: c.id }, stockOf(t.item) ? "primary" : "");
      }
    } else {
      job = `PATROL: Dolls downed for ${fmt(t.bounty)} CR of bounties` +
        (c.holder ? ` <span class="dim">(${fmt(t.earned)} so far · ${esc(c.holder)})</span>` : "");
      act = c.held ? button("GIVE UP", { act: "drop-patrol", id: c.id }) : c.holder ? `<span class="dim">TAKEN</span>` : button("TAKE", { act: "take-patrol", id: c.id }, "primary");
    }
    const pays = t.kind === "supply" && c.paid ? `${fmt(c.reward)} <span class="dim">(${fmt(c.paid)} paid)</span>` : fmt(c.reward);
    return `<tr${c.mine || c.held ? ' class="chosen"' : ""}><td class="dim">#${c.id}</td><td class="${c.colony ? "bid" : ""}">${esc(c.issuer)}</td>` +
      `<td>${job}</td><td class="num">${pays} CR</td><td class="num dim">${secs(c.secs_left)}</td><td class="act">${act}</td></tr>`;
  }

  function renderCharter() {
    const b = hangar?.charter;
    if (!b) return `<div class="note">Waiting for the Charter Board…</div>`;
    let left = `<h3>CONTRACTS</h3><div class="note">The colony's pay ${b.supply_pct}% of its desks' value; a pilot's reward is held in escrow until it's delivered. Deliveries come from your stores, paid on the spot.</div>`;
    if (!b.contracts.length) left += `<div class="note">Nothing posted.</div>`;
    else {
      left += `<table><tr><th></th><th>FROM</th><th>JOB</th><th class="num">PAYS</th><th class="num">LEFT</th><th></th></tr>` +
        b.contracts.map(contractRow).join("") + `</table>`;
    }
    // Posting one.
    const goods = catalogue.items.filter((i) => i.kind === "ore" || i.kind === "material" || i.kind === "weapon" || i.kind === "module" || i.kind === "kit" || i.kind === "part");
    const pick = draft("post:item", "mat.steel");
    const qty = Math.max(0, Math.floor(Number(draft("post:qty", isBulk(pick) ? "1000" : "1")) || 0));
    const reward = Math.max(0, Math.floor(Number(draft("post:reward", String(worth(pick, items.get(pick)?.value || 0, qty)))) || 0));
    left += `<section><h3>POST A CONTRACT</h3><div class="form"><label>WANTED <select data-key="post:item">` +
      goods.map((i) => `<option value="${esc(i.slug)}"${i.slug === pick ? " selected" : ""}>${esc(i.name)}</option>`).join("") +
      `</select></label><label>${isBulk(pick) ? "KG" : "PIECES"} ${input("post:qty", draft("post:qty", isBulk(pick) ? "1000" : "1"), 'inputmode="numeric"')}</label>` +
      `<label>REWARD ${input("post:reward", draft("post:reward", String(reward)), 'inputmode="numeric"')} CR</label>` +
      `<label>HOURS ${input("post:hours", draft("post:hours", "24"), 'inputmode="numeric" class="narrow"')}</label></div>` +
      `<div class="form">${button("POST", { act: "post" }, "primary")}<span class="note">${fmt(reward)} CR into escrow; what isn't delivered in time comes back. Up to ${b.max_posted} posted at once.</span></div></section>`;

    let right = `<h3>${esc(b.era_name)}</h3><div class="note">Your standing with the colony: <b>${fmt(b.standing)}</b>. The great works pay ${b.works_pct}% of the colony's value, and in standing.</div>`;
    for (const w of b.works) {
      right += `<section class="work${w.done ? " done" : ""}"><h3>${esc(w.name.toUpperCase())}${w.done ? ' <span class="bid">FINISHED</span>' : ""}</h3><div class="note">${esc(w.summary)}</div>`;
      if (w.work === "charter_vote") {
        right += `<div class="note">Signatures: ${b.signatures} of ${b.signatures_needed}.${w.open ? "" : w.done ? "" : " The vote opens when the era's works are finished."}</div>`;
        if (w.open && !b.signed) right += `<div class="form">${button("SIGN THE CHARTER", { act: "sign" }, b.standing ? "primary" : "")}</div>`;
        if (b.signed) right += `<div class="note bid">You signed.</div>`;
      } else {
        right += `<table>` + w.needs.map(([item, need, have]) => {
          const pct = Math.min(100, Math.floor((have * 100) / Math.max(1, need)));
          const key = `give:${w.work}:${item}`;
          const dflt = Math.min(need - have, stockOf(item));
          const give = w.done || have >= need ? "" : `${input(key, draft(key, String(Math.max(0, dflt))), 'inputmode="numeric" class="narrow"')}` +
            button("DELIVER", { act: "contribute", work: w.work, item }, stockOf(item) ? "primary" : "");
          return `<tr><td>${esc(nameOf(item))}<div class="note">${amount(item, have)} / ${amount(item, need)}</div></td><td>${bar(pct)}</td><td class="act">${give}</td></tr>`;
        }).join("") + `</table>`;
      }
      if (w.top.length) {
        right += `<div class="note">Most given: ${w.top.map(([n, s]) => `${esc(n)} ${fmt(s)}`).join(" · ")}${w.mine ? ` · you ${fmt(w.mine)}` : ""}</div>`;
      }
      right += `</section>`;
    }
    return `<div class="split even"><div>${left}</div><div>${right}</div></div>${renderWanted(b)}`;
  }

  // The Most Wanted: Zodiac's aces, one out among the Dolls at a time, each with a bounty on it;
  // who downed each last, the ladder, and the pilot's terms (the bounty paid, or its wreck).
  function renderWanted(b) {
    if (!b.wanted || !b.wanted.length) return "";
    const now = Date.now() / 1000;
    const terms = (salvage, label) => button(label, { act: "ace-terms", salvage: salvage ? "1" : "0" }, !!b.salvage_terms === salvage ? "primary" : "");
    let out = `<section><h3>MOST WANTED</h3><div class="note">Zodiac's aces fly among the Dolls one at a time, each in a Doll tuned by hand. Down one and the Charter Board pays its bounty; or, on your terms, the tugs bring its wreck home to your stores instead, if nobody gets to it first.</div>` +
      `<div class="row">${terms(false, "TERMS: THE BOUNTY")} ${terms(true, "TERMS: ITS WRECK")}</div>`;
    out += `<table><tr><th>ACE</th><th class="num">BOUNTY</th><th>LAST DOWNED</th></tr>` +
      b.wanted.map((w) => `<tr${w.out ? ' class="chosen"' : ""}><td>${esc(w.name)}${w.out ? ' <span class="bid">OUT NOW</span>' : ""}</td>` +
        `<td class="num">${fmt(w.bounty)} CR</td>` +
        `<td class="dim">${w.last ? `${esc(w.last.by)}, ${secs(Math.max(0, Math.floor(now - w.last.at)))} ago` : "never"}</td></tr>`).join("") + `</table>`;
    const ladder = b.ladder || [];
    out += ladder.length
      ? `<div class="note">Aces downed: ${ladder.map(([n, k]) => `${esc(n)} ${fmt(k)}`).join(" · ")}${b.mine_aces ? ` · you ${fmt(b.mine_aces)}` : ""}</div>`
      : `<div class="note">Nobody has downed one yet.</div>`;
    return out + `</section>`;
  }

  // The Proving Ground's board, at the Blast Hall's desk: the day's best round the course and
  // through the drill (turning over at midnight UTC), the best ever, the pars, and the pilot's own
  // bests. Every time on it is one the server checked (crates/bc-econ/src/proving.rs).
  const clock = (ms) => {
    const t = Math.floor(ms / 100);
    return `${Math.floor(t / 600)}:${String(Math.floor(t / 10) % 60).padStart(2, "0")}.${t % 10}`;
  };
  const certificate = (ms, par) => (ms <= par ? "FIRST CLASS" : ms <= par * 1.5 ? "SECOND CLASS" : "THIRD CLASS");
  function boardColumn(title, how, rows, record, par, mine, none) {
    let out = `<h3>${title}</h3><div class="note">${how} Par is ${clock(par)}.</div>`;
    out += `<div class="note">Your best: ${mine ? `<b>${clock(mine)}</b> ${certificate(mine, par)}` : none}</div>`;
    if (!rows.length) out += `<div class="note">Nobody yet today.</div>`;
    else {
      out += `<table><tr><th></th><th>TODAY</th><th class="num">TIME</th><th></th></tr>` +
        rows.map((r, k) => `<tr${r.you ? ' class="chosen"' : ""}><td class="dim">${k + 1}</td><td>${esc(r.name)}</td>` +
          `<td class="num">${clock(r.ms)}</td><td class="dim">${certificate(r.ms, par)}</td></tr>`).join("") + `</table>`;
    }
    if (record) out += `<div class="note">The best ever: <b>${clock(record.ms)}</b> ${esc(record.name)}${record.you ? " (you)" : ""}</div>`;
    return out;
  }
  function renderProving() {
    const p = hangar?.proving;
    if (!p) return `<div class="note">The Proving Ground's board hangs in the Blast Hall, off Hub Gate's square, and is kept at its desk: ride the cap lift down to see it.</div>`;
    const course = boardColumn("THE COURSE", "Thirteen rings from by the inner gate down to Hub Gate's square, and land on its pad.",
      p.course, p.course_record, p.course_par_ms, p.mine?.course_ms, "not yet flown.");
    const drill = boardColumn("THE DRILL", "In the Blast Hall: twenty targets lit in turn against the clock, each struck putting time back on it.",
      p.drill, p.drill_record, p.drill_par_ms, p.mine?.drill_ms, "not yet cleared.");
    // The test range: what the gantry readies (the Board's Leo, the bay's build, any line's).
    const chosen = p.trainer || { kind: "board" };
    const pick = (build, label) => {
      const on = JSON.stringify(build) === JSON.stringify(chosen);
      return `<button class="${on ? "primary" : ""}" data-act="trainer" data-build='${esc(JSON.stringify(build))}'>${esc(label)}</button>`;
    };
    const trainers = [pick({ kind: "board" }, "THE BOARD'S LEO"), pick({ kind: "bay" }, "YOUR BAY'S BUILD")]
      .concat(frames.map((f) => pick({ kind: "line", line: f.slug }, `A NEW ${String(f.name).toUpperCase()}`)))
      .join(" ");
    const range = `<h3>THE TEST RANGE</h3><div class="note">Try a build before it counts: the gantry readies what you pick, new and full, and nothing of yours is taken.</div><div class="row">${trainers}</div>`;
    const how = `<div class="note">Board it at the gantry's hatch, by the blast doors (E), weapons free in the hall. Dock it back at rest on the gantry. From your bay, Q at the cockpit brings your own suit in by the inner gate.</div>`;
    return `<div class="split even"><div>${course}</div><div>${drill}</div></div>${range}${how}`;
  }

  // Replacing a focused field blurs it, and a blur can fire events that would render again from
  // inside this render: once at a time.
  let rendering = false;
  function renderTerminal() {
    if (rendering) return;
    rendering = true;
    try {
      drawTerminal();
    } finally {
      rendering = false;
    }
  }

  function drawTerminal() {
    for (const b of document.querySelectorAll("[data-tab]")) b.classList.toggle("active", b.dataset.tab === tab);
    $("term-credits").textContent = hangar?.view ? `${fmt(hangar.view.credits)} CR` : "";
    // Keep the field being typed in.
    const active = document.activeElement;
    const key = active && active.dataset ? active.dataset.key : null;
    const caret = key && typeof active.selectionStart === "number" ? active.selectionStart : null;
    const body = $("term-body");
    const scroll = body.scrollTop;
    const render = { fabricator: renderFabricator, stores: renderStores, suit: renderSuit, exchange: renderExchange, charter: renderCharter, proving: renderProving }[tab];
    body.innerHTML = render ? render() : "";
    body.scrollTop = scroll;
    if (key) {
      const el = body.querySelector(`[data-key="${CSS.escape(key)}"]`);
      if (el) {
        el.focus();
        if (caret != null && el.setSelectionRange) el.setSelectionRange(caret, caret);
      }
    }
    const log = $("term-log");
    log.innerHTML = (hangar?.log || [])
      .slice()
      .reverse()
      .map(([text, ok]) => `<div class="${ok ? "ok" : "no"}">${esc(ok ? text : `CAN'T: ${String(text).toUpperCase()}`)}</div>`)
      .join("");
  }

  function watch(slug) {
    if (slug === watching) return;
    watching = slug;
    ask({ t: "watch", item: slug || null });
  }

  // The board comes from the server only while its tab is open.
  let boardOn = false;
  function watchBoard(on) {
    if (on === boardOn) return;
    boardOn = on;
    ask({ t: "watch_board", on });
  }

  function openTab(t) {
    tab = t;
    watchBoard(t === "charter");
    if (t === "exchange" && !watching) {
      const held = hangar?.view?.stock?.find(([s]) => isBulk(s));
      watch(held ? held[0] : "ore.nickel_iron");
    }
    renderTerminal();
  }

  function onTerminalClick(e) {
    const el = e.target instanceof Element ? e.target.closest("[data-act]") : null;
    if (!el) return;
    const d = el.dataset;
    switch (d.act) {
      case "fab-filter":
        fabFilter = d.f;
        break;
      case "ex-filter":
        exFilter = d.f;
        break;
      case "make": {
        const n = Math.max(1, Math.floor(Number(draft(`batches:${d.item}`, "1")) || 1));
        ask({ t: "craft", item: d.item, batches: n });
        return;
      }
      case "cancel-job":
        ask({ t: "cancel_job", station: d.station, index: Number(d.index) });
        return;
      case "fit":
        ask({ t: "fit", item: d.item });
        return;
      case "scrap":
        ask({ t: "scrap", item: d.item });
        return;
      case "strip-part":
        ask({ t: "strip", slot: { kind: "part", part: d.part } });
        return;
      case "strip-mount":
        ask({ t: "strip", slot: { kind: "mount", mount: Number(d.mount) } });
        return;
      case "repair":
        ask(d.part ? { t: "repair", part: d.part } : { t: "repair" });
        return;
      case "overhaul":
        ask(d.part ? { t: "overhaul", part: d.part } : { t: "overhaul" });
        return;
      case "strip-module":
        ask({ t: "strip", slot: { kind: "module", module: Number(d.module) } });
        return;
      case "dismantle":
        if (confirm("Strip the suit bare, torso and all, into the stores?")) ask({ t: "dismantle" });
        return;
      case "sell":
        side = "sell";
        exFilter = { part: "parts", weapon: "weapons", module: "modules", kit: "kits" }[items.get(d.item)?.kind] || "goods";
        watch(d.item);
        tab = "exchange";
        break;
      case "watch":
        watch(d.item);
        break;
      case "side":
        side = d.side;
        break;
      case "order": {
        const it = items.get(watching);
        if (!it) return;
        const q = hangar?.market?.quotes.find((x) => x.item === watching) || {};
        const dfltPrice = side === "buy" ? q.ask ?? q.last ?? it.value : q.bid ?? q.last ?? it.value;
        const price = Math.floor(Number(draft(`price:${side}:${watching}`, String(dfltPrice || 0))) || 0);
        const dfltQty = side === "sell" ? Math.min(stockOf(watching), it.bulk ? 1000 : 1) || (it.bulk ? 1000 : 1) : it.bulk ? 1000 : 1;
        const qty = Math.floor(Number(draft(`qty:${side}:${watching}`, String(dfltQty))) || 0);
        if (price <= 0 || qty <= 0) return;
        ask({ t: "order", item: watching, side, price, qty, rest: !!draft("rest", true) });
        return;
      }
      case "cancel-order":
        ask({ t: "cancel_order", id: Number(d.id) });
        return;
      case "deliver": {
        const qty = Math.floor(Number(draft(`deliver:${d.id}`, "0")) || 0);
        const c = hangar?.charter?.contracts.find((x) => x.id === Number(d.id));
        const dflt = c && c.task.kind === "supply" ? Math.min(c.task.qty - c.task.delivered, stockOf(c.task.item)) : 0;
        const n = `deliver:${d.id}` in drafts ? qty : dflt;
        if (n > 0) ask({ t: "deliver", id: Number(d.id), qty: n });
        return;
      }
      case "withdraw":
        ask({ t: "withdraw", id: Number(d.id) });
        return;
      case "take-patrol":
        ask({ t: "take_patrol", id: Number(d.id) });
        return;
      case "trainer":
        ask({ t: "trainer", build: JSON.parse(d.build) });
        return;
      case "drop-patrol":
        ask({ t: "drop_patrol", id: Number(d.id) });
        return;
      case "contribute": {
        const w = hangar?.charter?.works.find((x) => x.work === d.work);
        const need = w?.needs.find(([i]) => i === d.item);
        const key = `give:${d.work}:${d.item}`;
        const dflt = need ? Math.min(need[1] - need[2], stockOf(d.item)) : 0;
        const n = key in drafts ? Math.floor(Number(drafts[key]) || 0) : dflt;
        if (n > 0) ask({ t: "contribute", work: d.work, item: d.item, qty: n });
        return;
      }
      case "sign":
        ask({ t: "sign" });
        return;
      case "ace-terms":
        ask({ t: "ace_terms", salvage: d.salvage === "1" });
        return;
      case "post": {
        const item = draft("post:item", "mat.steel");
        const qty = Math.floor(Number(draft("post:qty", isBulk(item) ? "1000" : "1")) || 0);
        const reward = Math.floor(Number(draft("post:reward", String(worth(item, items.get(item)?.value || 0, qty)))) || 0);
        const hours = Math.floor(Number(draft("post:hours", "24")) || 0);
        if (qty > 0 && reward > 0 && hours > 0) ask({ t: "post", item, qty, reward, hours });
        return;
      }
      default:
        return;
    }
    renderTerminal();
  }

  function onTerminalInput(e) {
    const el = e.target;
    if (!(el instanceof HTMLInputElement || el instanceof HTMLSelectElement) || !el.dataset.key) return;
    drafts[el.dataset.key] = el.type === "checkbox" ? el.checked : el.value;
    // A new item to post for: its quantity and reward start again from its value.
    if (el.dataset.key === "post:item") {
      delete drafts["post:qty"];
      delete drafts["post:reward"];
    }
    if (el.dataset.key === "post:qty") delete drafts["post:reward"];
    // Totals follow what's typed; the field itself is kept as it is.
    if (tab === "exchange" || tab === "fabricator" || tab === "charter") renderTerminal();
  }

  window.bcUi = { init, update, settings: renderSettings, catalogue: catalogueIn, hangar: hangarIn };

  // --- What the player does. ---
  document.addEventListener("DOMContentLoaded", () => {
    $("launch").addEventListener("submit", launch);
    $("wallet-button").addEventListener("click", connectWallet);
    $("guest-button").addEventListener("click", () => {
      address = null;
      remember(false);
      showWallet();
    });
    showWallet();
    // A wallet connected on an earlier visit is picked up without a prompt (eth_accounts asks
    // nothing); signing still waits for Launch.
    let wanted = false;
    try {
      wanted = localStorage.getItem(WALLET_KEY) === "on";
    } catch {}
    if (wanted) {
      wallet.accounts().then((a) => {
        if (a && a[0] && !address) {
          address = a[0];
          showWallet();
        }
      });
    }
    // Somebody switches account in the extension while the page is open.
    if (wallet.has() && typeof window.ethereum.on === "function") {
      window.ethereum.on("accountsChanged", (a) => {
        address = (a && a[0]) || null;
        showWallet();
      });
    }
    // The terminals: one listener for all their buttons and fields.
    // The radio's line: Enter says it, Esc closes it; what's typed here isn't the game's.
    $("chat-input").addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === "Escape") {
        e.preventDefault();
        const text = e.target.value.trim();
        if (e.key === "Enter" && text) send("say", { text });
        e.target.value = "";
        send("chat", { open: false });
        send("resume");
      }
      e.stopPropagation();
    });
    $("term-body").addEventListener("click", onTerminalClick);
    $("term-body").addEventListener("input", onTerminalInput);
    for (const b of document.querySelectorAll("[data-tab]")) {
      b.addEventListener("click", () => openTab(b.dataset.tab));
    }
    // Survival rules: nothing to choose on the title but a callsign.
    fetch("/status", { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : null))
      .then((st) => {
        survival = !!st && (st.game?.rules ?? st.rules) === "survival";
        show($("frames-field"), !survival);
        show($("survival-field"), survival);
      })
      .catch(() => {});
    $("title-controls").addEventListener("click", () => send("help", { show: true }));
    $("title-settings").addEventListener("click", () => send("settings", { show: true }));
    $("reload").addEventListener("click", () => location.reload());
    $("cancel").addEventListener("click", () => send("cancel"));
    for (const b of document.querySelectorAll("[data-cmd]")) {
      b.addEventListener("click", () => {
        const cmd = b.dataset.cmd;
        if (cmd === "controls") send("help", { show: true });
        else if (cmd === "help-close") send("help", { show: false });
        else if (cmd === "settings") send("settings", { show: true });
        else if (cmd === "settings-close") send("settings", { show: false });
        else send(cmd);
        if (cmd === "resume" || cmd === "help-close") canvas()?.focus();
      });
    }
  });

  // Sound: the game makes the AudioContext (window.bcAudio). Browsers start it only from a
  // gesture, so any click or key wakes it; it sleeps while the tab is hidden (the game's frames
  // stop, and an engine loop would drone on).
  const wakeAudio = () => {
    const a = window.bcAudio;
    if (a && a.state === "suspended" && !document.hidden) a.resume().catch(() => {});
  };
  window.addEventListener("pointerdown", wakeAudio, true);
  window.addEventListener("keydown", wakeAudio, true);
  document.addEventListener("visibilitychange", () => {
    const a = window.bcAudio;
    if (!a) return;
    (document.hidden ? a.suspend() : a.resume()).catch(() => {});
  });
  // Every menu button clicks (Launch confirms, above).
  document.addEventListener(
    "click",
    (e) => {
      const b = e.target instanceof Element ? e.target.closest("button") : null;
      if (b && b.id !== "launch-button") send("sfx", { cue: "click" });
    },
    true,
  );

  // Esc and F1 are the page's (Chrome keeps the Esc that ends a pointer lock to itself; the game
  // notices the lock going instead). Capture phase, so the game's canvas can't swallow them.
  window.addEventListener(
    "keydown",
    (e) => {
      if (e.key === "F1") {
        e.preventDefault();
        if (!e.repeat) send("help");
      } else if ((e.key === "e" || e.key === "E") && !e.repeat && view.panel === "terminal" &&
        !(e.target instanceof HTMLInputElement)) {
        // E again steps away from the terminal.
        e.preventDefault();
        send("resume");
        canvas()?.focus();
      } else if (e.key === "Escape" && !e.repeat && e.target !== $("chat-input")) {
        send(view.screen === "connecting" ? "cancel" : "back");
      }
    },
    true,
  );
})();
