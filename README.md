# Before Colony

A Gundam Wing space MMO prototype: free-aim, Newtonian mobile-suit combat in the Earth Sphere,
Mobile Doll AI, AI agents as first-class pilots, and the ZERO System as a predictive combat AI.

**Status:** Milestone 1 (playable vertical slice) in progress. See `docs/` for design and architecture.

- Server: Rust, one allocation-free, lock-free simulation thread per sector (`bc-sim`, `bc-sector`)
- Client: Bevy 0.19 compiled to WebAssembly, in the browser
- Transport: WebTransport over HTTP/3 (QUIC): unreliable datagrams for inputs and snapshots

Fan project. Gundam Wing and its names are © Sotsu · Sunrise; canon names live only behind the
`canon-names` feature of `bc-sim`, and all art is procedural.
