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

  // --- The game's view. ---
  function update(v) {
    view = v;
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
    $("pause-who").textContent = v.signedIn && address
      ? `Signed in as ${short(address)}.`
      : "Flying as a guest: your suit is lost when you leave.";
    show($("settings"), v.panel === "settings");
    const hint = $("hint");
    hint.textContent = v.hint || "";
    show(hint, s === "playing" && !!v.hint && v.panel === "none");
    show($("help"), v.help);
    show($("prompt"), s === "playing" && v.clickToFly && !v.help);
    $("prompt-main").textContent = v.refused ? "CLICK AGAIN TO FLY" : "CLICK TO FLY";

    if (v.toastSeq !== toastSeq) {
      toastSeq = v.toastSeq;
      if (v.toast) {
        const t = $("toast");
        t.textContent = v.toast;
        show(t, true);
        clearTimeout(toastTimer);
        toastTimer = setTimeout(() => show(t, false), 2500);
      }
    }
  }

  function init(data) {
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
    for (const r of data.rows) {
      const row = settingRows.get(r.key);
      if (!row) continue;
      if (row.kind === "range") {
        if (document.activeElement !== row.input) row.input.value = r.value;
        const n = Number(r.value);
        row.value.textContent = r.key === "fov" ? `${n}°` : r.max <= 1 ? `${Math.round(n * 100)}%` : `${n.toFixed(2)}×`;
      } else if (row.kind === "toggle") {
        row.input.checked = r.value === "true";
        row.value.textContent = r.value === "true" ? "ON" : "OFF";
      } else {
        row.input.value = r.value;
        row.value.textContent = "";
      }
    }
  }

  window.bcUi = { init, update, settings: renderSettings };

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
      } else if (e.key === "Escape" && !e.repeat) {
        send(view.screen === "connecting" ? "cancel" : "back");
      }
    },
    true,
  );
})();
