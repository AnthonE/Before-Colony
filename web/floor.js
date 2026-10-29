// The Exchange floor as the store's trade page shows it (`?showcase=floor`, docs/FLOOR.md): every
// two seconds, the game server's `/floor.json` (who stands in the room now: pilots, and the
// store's traders), kept as text in `window.__bcFloor` for the game to draw
// (bc_client_core::floor). `?feed=url` reads another copy (a test's). Nothing here runs outside
// that scene. A file of its own, so the page keeps to `script-src 'self'` (the store's CSP).
(function () {
  var q = new URLSearchParams(location.search);
  var scene = (q.get("showcase") || "").toLowerCase();
  if (scene !== "floor" && scene !== "exchange") return;
  var http = q.get("http") || (window.BC_SERVER && window.BC_SERVER.http) || "/";
  var url = q.get("feed") || http.replace(/\/?$/, "/") + "floor.json";
  var EVERY_MS = 2000;
  function read() {
    if (document.hidden) return;
    fetch(url, { cache: "no-store" })
      .then(function (r) { return r.ok ? r.text() : null; })
      .then(function (text) { if (text) window.__bcFloor = text; })
      .catch(function () { /* the last read stands; the next beat tries again */ });
  }
  read();
  setInterval(read, EVERY_MS);
  document.addEventListener("visibilitychange", read);
})();
