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

  function launch(e) {
    if (e) e.preventDefault();
    send("play", { name: $("callsign").value.trim(), frame: chosen });
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
    if (s === "connecting") msg.textContent = "CONNECTING TO THE SECTOR…";
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
        (v.message ? `\n${v.message}` : "");
    }

    show($("pause"), s === "playing" && v.panel === "pause");
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

  window.bcUi = { init, update };

  // --- What the player does. ---
  document.addEventListener("DOMContentLoaded", () => {
    $("launch").addEventListener("submit", launch);
    $("title-controls").addEventListener("click", () => send("help", { show: true }));
    $("reload").addEventListener("click", () => location.reload());
    $("cancel").addEventListener("click", () => send("cancel"));
    for (const b of document.querySelectorAll("[data-cmd]")) {
      b.addEventListener("click", () => {
        const cmd = b.dataset.cmd;
        if (cmd === "controls") send("help", { show: true });
        else if (cmd === "help-close") send("help", { show: false });
        else send(cmd);
        if (cmd === "resume" || cmd === "help-close") canvas()?.focus();
      });
    }
  });

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
