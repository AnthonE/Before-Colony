// The page's gallery (`?gallery`, or `?gallery=<scene>`): every screen and panel the page draws,
// filled with made-up data that looks like the game's, without the game. The scenes are
// `bc_client_core::gallery`'s (written to gallery.json by scripts/build-web.sh); each is a view as
// the game would send it to `bcUi.update`, with the hangar's word for a terminal's tab. Change
// style.css or ui.js and reload: every panel can be looked at in a few seconds, no flying to the
// desk. [ and ] step through the scenes; what the page would send the game shows in the corner.
// The `gallery` e2e suite photographs each one (`window.bcGallery`).

const ready = () =>
  new Promise((done) => (document.readyState === "loading" ? document.addEventListener("DOMContentLoaded", done) : done()));

async function main() {
  await ready();
  document.body.classList.add("gallery");
  document.body.classList.toggle("gallery-bare", new URLSearchParams(location.search).has("bare"));
  window.bcHideBoot?.();
  const res = await fetch("gallery.json", { cache: "no-store" });
  if (!res.ok) throw new Error(`gallery.json: the server answered ${res.status} (scripts/build-web.sh writes it)`);
  const g = await res.json();
  const ui = window.bcUi;
  ui.init(g.init);
  ui.settings(g.settings);
  ui.catalogue(JSON.stringify(g.catalogue));

  // The toolbar: the scene, what it is, and the last command the page sent.
  const bar = document.createElement("div");
  bar.id = "gallery-bar";
  const prev = document.createElement("button");
  prev.type = "button";
  prev.textContent = "[";
  prev.title = "the scene before ([)";
  const pick = document.createElement("select");
  for (const s of g.scenes) {
    const o = document.createElement("option");
    o.value = s.name;
    o.textContent = s.name;
    pick.append(o);
  }
  const next = document.createElement("button");
  next.type = "button";
  next.textContent = "]";
  next.title = "the next scene (])";
  const again = document.createElement("button");
  again.type = "button";
  again.textContent = "AGAIN";
  again.title = "show it again (a toast or the news fades)";
  const about = document.createElement("div");
  about.className = "about";
  const sent = document.createElement("div");
  sent.className = "sent";
  bar.append(prev, pick, next, again, about, sent);
  document.body.append(bar);

  // The seqs move each time a scene is shown, so its toast, news and payout sheet show again.
  let seq = 0;
  let current = 0;
  const neutral = { screen: "playing", panel: "none", toastSeq: 0, newsSeq: 0, debriefSeq: 0, radioSeq: 0 };
  async function show(name) {
    const i = Math.max(0, g.scenes.findIndex((s) => s.name === name));
    const s = g.scenes[i];
    current = i;
    pick.value = s.name;
    about.textContent = s.about;
    for (const id of ["toast", "news", "debrief"]) document.getElementById(id)?.classList.add("hidden");
    // Off whatever was open (a terminal opens on its tab only as it opens).
    ui.update({ ...g.scenes[0].view, ...neutral, toastSeq: seq, newsSeq: seq, debriefSeq: seq, radioSeq: seq });
    if (s.hangar) ui.hangar(JSON.stringify(s.hangar));
    seq += 1;
    const v = { ...s.view };
    for (const k of ["toastSeq", "newsSeq", "debriefSeq", "radioSeq"]) if (v[k]) v[k] = seq;
    ui.update(v);
    const url = new URL(location.href);
    url.searchParams.set("gallery", s.name);
    history.replaceState(null, "", url);
    // Two frames: the map's canvases size themselves to the laid-out page.
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    // And its fade-ins done (the looping ones, a hazard stripe's crawl, never are).
    const finite = document.getAnimations().filter((a) => a.effect?.getComputedTiming().iterations !== Infinity);
    await Promise.all(finite.map((a) => a.finished.catch(() => {})));
    return s.expect;
  }
  const step = (d) => show(g.scenes[(current + d + g.scenes.length) % g.scenes.length].name);
  prev.addEventListener("click", () => step(-1));
  next.addEventListener("click", () => step(1));
  again.addEventListener("click", () => step(0));
  pick.addEventListener("change", () => show(pick.value));
  window.addEventListener("keydown", (e) => {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
    if (e.key === "[") step(-1);
    else if (e.key === "]") step(1);
  });

  // No game drains the inbox: say what it would have got.
  const inbox = (window.bcInbox = window.bcInbox || []);
  setInterval(() => {
    if (!inbox.length) return;
    const last = inbox.filter((c) => c.cmd !== "sfx").pop();
    if (last) sent.textContent = `→ ${JSON.stringify(last)}`;
    inbox.length = 0;
  }, 150);

  window.bcGallery = { scenes: g.scenes.map((s) => ({ name: s.name, expect: s.expect })), show };
  await show(new URLSearchParams(location.search).get("gallery") || g.scenes[0].name);
}

main().catch((e) => {
  console.error(e);
  if (window.bcBoot) window.bcBoot.fail(String(e.message || e));
});
