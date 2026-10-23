// Boots the Bevy client: picks the WebGPU or WebGL2 build and a graphics tier, and hands the
// launch config to Rust via window.BC_CONFIG. The game asks window.bcDiscover() where to connect on
// every dial (a restarted dev server has a new certificate). `?showcase=<scene>` runs an offline
// scene and needs no game server; `?gallery` loads no game at all, only the page's screens with
// made-up data (gallery.js).
const params = new URLSearchParams(location.search);

// Where to connect (window.bcServer: config.js and the page's query, boot.js). A named server
// (`wt`) has a CA-issued certificate the browser checks itself: dialled as it is, nothing pinned.
// Otherwise the server's `cert-hash` says its port and the hash to pin (null when its certificate
// is CA-issued), fresh on every dial. Relative, so it works under a path prefix too.
window.bcDiscover = async () => {
  const srv = window.bcServer || { wt: "", pin: false, url: (p) => p };
  if (srv.wt && !srv.pin) return { wtUrl: `https://${srv.wt}/bc`, certHash: null };
  const res = await fetch(srv.url("cert-hash"), { cache: "no-store" });
  if (!res.ok) throw new Error(`the server answered ${res.status}`);
  const info = await res.json();
  const host = srv.wt || `${new URL(srv.url(""), location.href).hostname}:${info.port}`;
  return { wtUrl: `https://${host}${info.path}`, certHash: info.hash };
};
const status = (msg) => {
  const el = document.getElementById("boot-status");
  if (el) el.textContent = msg;
};
// The loading screen's step (the watchdog in index.html: it says which, and notices one that
// stops moving); a page without the watchdog just shows the line.
const step = (name, line, fraction) => (window.bcBoot ? window.bcBoot.step(name, line, fraction) : status(line));

// Resolves to `fallback` if `promise` hasn't settled in `ms`.
const within = (promise, ms, fallback) =>
  Promise.race([promise, new Promise((done) => setTimeout(() => done(fallback), ms))]);

async function pickGraphics() {
  const forced = params.get("gfx");
  if (forced === "webgl2" || forced === "webgpu") return forced;
  try {
    // Some drivers never answer the adapter request: past a few seconds, WebGL 2 it is.
    if (navigator.gpu && (await within(navigator.gpu.requestAdapter(), 4000, null))) {
      // Only use WebGPU if that build was shipped.
      const probe = await fetch("./webgpu/bc.js", { method: "HEAD" });
      if (probe.ok) return "webgpu";
    }
  } catch (_) {
    /* fall through */
  }
  return "webgl2";
}

// The GPU's name as WebGL reports it ("" when the browser hides it).
function rendererName() {
  try {
    const gl = document.createElement("canvas").getContext("webgl2");
    if (!gl) return "none";
    const ext = gl.getExtension("WEBGL_debug_renderer_info");
    return ext ? String(gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)) : "";
  } catch (_) {
    return "none";
  }
}

const TIERS = ["low", "medium", "high", "ultra"];

// The tier for this GPU: Low on software rasterisers, else High. (`?quality=` overrides it, and in
// game mode so does the pilot's saved setting.)
function autoQuality(renderer) {
  if (renderer === "none" || /swiftshader|llvmpipe|softpipe|software|basic render/i.test(renderer)) {
    return "low";
  }
  return "high";
}

// Fades the loading screen out, then hides it for good (called when the first frame is drawn).
window.bcHideBoot = () => {
  const boot = document.getElementById("boot");
  if (window.bcBoot) window.bcBoot.done();
  if (!boot || boot.classList.contains("fading") || boot.classList.contains("failed")) return;
  boot.classList.add("fading");
  setTimeout(() => boot.classList.add("hidden"), 650);
};

async function main() {
  if (params.has("gallery")) {
    await import("./gallery.js");
    return;
  }
  const showcase = params.get("showcase") || "";
  const renderer = rendererName();
  const qualityParam = (params.get("quality") || "").toLowerCase();
  const qualityAuto = autoQuality(renderer);
  const config = {
    // `1`: the drone brain; `lander[:plan]`: a lander (see config.rs).
    autopilot: params.get("autopilot") || "",
    autoplay: params.get("autoplay") || "",
    echo: params.get("mode") === "echo",
    lowQuality: params.get("quality") === "low",
    quality: TIERS.includes(qualityParam) ? qualityParam : qualityAuto,
    qualityAuto,
    qualityParam: TIERS.includes(qualityParam) ? qualityParam : "",
    renderer,
    name: params.get("name") || "",
    frame: params.get("frame") || "",
    showcase,
    t: Number(params.get("t") || 0),
    cam: Number(params.get("cam") || 1),
    realtime: params.get("realtime") === "1",
    hold: Number(params.get("hold") || 0),
    perf: params.get("perf") === "1",
    calm: params.get("calm") === "1" || matchMedia("(prefers-reduced-motion: reduce)").matches,
    tonemap: params.get("tonemap") || "",
    // `?look=0`: the plain look (no grade, vignette or lit smoke), to compare against.
    look: params.get("look") !== "0",
    // `?life=0`: no traffic or people in the colony's city, to compare and to measure.
    life: params.get("life") !== "0",
    // `?ssao=0`: no screen-space ambient occlusion (WebGPU), to compare against.
    ssao: params.get("ssao") !== "0",
    // `?hz=N`: the showcase's fixed clock rate (default 60), to see effects at a low frame rate.
    hz: Number(params.get("hz") || 60),
    // `?clean=1`: the showcase without its line of text in the corner, for clips.
    clean: params.get("clean") === "1",
  };
  if (!showcase) {
    if (!("WebTransport" in window)) {
      // The game still loads (the title screen says what's wrong); the echo spike can't.
      if (config.echo) {
        if (window.bcBoot) window.bcBoot.fail("this browser has no WebTransport: use Chrome or Edge");
        else status("this browser has no WebTransport: use Chrome or Edge");
        return;
      }
      config.noWebTransport = true;
    } else if (config.echo) {
      Object.assign(config, await window.bcDiscover());
    }
  }
  window.BC_CONFIG = config;
  step("graphics", "choosing WebGPU or WebGL 2…");
  const gfx = await pickGraphics();
  window.BC_CONFIG.gfx = gfx;
  step("client", `fetching the ${gfx} client (${config.quality})…`);
  const mod = await import(`./${gfx}/bc.js`);
  const wasm = fetchCounted(new URL(`./${gfx}/bc_bg.wasm`, location.href), (got, total) =>
    step("client", `fetching the ${gfx} client · ${(got / 1048576).toFixed(1)} MB`, total ? got / total : null),
  );
  // Bevy's App::run hands control to the browser event loop by throwing a control-flow
  // exception on some targets; that is expected. The loading screen stays up until the client's
  // first frame (`bcHideBoot`, from `dev_hooks`).
  try {
    await mod.default({ module_or_path: wasm });
  } catch (e) {
    if (!String(e).includes("Using exceptions for control flow")) throw e;
  }
}

// Fetches the client's wasm, telling `progress` the bytes so far (and the whole, when the server
// says it and the bytes aren't compressed in flight: then `total` is 0). Once it's all in, the
// step moves on to starting it.
async function fetchCounted(url, progress) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url.pathname}: the server answered ${res.status}`);
  const compressed = !!res.headers.get("content-encoding");
  const total = compressed ? 0 : Number(res.headers.get("content-length")) || 0;
  const started = () => step("start", "starting the client…");
  if (!res.body) {
    started();
    return res;
  }
  const reader = res.body.getReader();
  let got = 0;
  const body = new ReadableStream({
    async pull(ctrl) {
      const { done, value } = await reader.read();
      if (done) {
        started();
        ctrl.close();
        return;
      }
      got += value.byteLength;
      progress(got, total);
      ctrl.enqueue(value);
    },
    cancel(reason) {
      return reader.cancel(reason);
    },
  });
  return new Response(body, { status: res.status, statusText: res.statusText, headers: res.headers });
}

main().catch((e) => {
  console.error(e);
  if (window.bcBoot) window.bcBoot.fail(String(e.message || e));
  else status(`failed: ${e}`);
});
