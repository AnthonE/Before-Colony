// Where this page's game server is. Empty, as here (scripts/dev.sh, and a server serving its own
// page): the server that served the page. A build published to another site rewrites this file
// (scripts/publish-web.sh), e.g. for the store's copy at elopros.com:
//   window.BC_SERVER = { wt: "game.elopros.com:61240", http: "https://game.elopros.com/before-colony/" };
// `wt`: the WebTransport host:port, whose certificate is CA-issued (nothing pinned, unless `pin`);
// `http`: the game server's HTTP side (`status`, `cert-hash`), ending in "/".
// `?server=host:port` and `?http=url` on the page's address override them; `?pin=1` pins the
// certificate the server's `cert-hash` names (a self-signed dev server on another host).
// `reports`: where the page files a player's report (F7) and a crash, if not the game server's
// `bugs`: the Elo Pros intake (`https://elopros.com/api/bugs`) takes the same body.
window.BC_SERVER = window.BC_SERVER || {};
// What the page was built from, `{version, commit}` (window.BC_BUILD): scripts/build-web.sh appends
// it to the copy in web/dist, and scripts/publish-web.sh writes it into the one it stages. A report
// names it; without it a report says `dev` and `unknown`.
