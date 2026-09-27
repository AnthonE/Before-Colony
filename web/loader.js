// Boots the Bevy client: picks the WebGPU or WebGL2 build and a graphics tier, and hands the
// launch config to Rust via window.BC_CONFIG. The game asks window.bcDiscover() for the server's
// WebTransport port and certificate hash on every dial (a restarted dev server has a new
// certificate). `?showcase=<scene>` runs an offline scene and needs no game server.
const params = new URLSearchParams(location.search);

// Where to connect, fresh from the server that served this page.
window.bcDiscover = async () => {
  const res = await fetch("/cert-hash", { cache: "no-store" });
  if (!res.ok) throw new Error(`the server answered ${res.status}`);
  const info = await res.json();
  return { wtUrl: `https://${location.hostname}:${info.port}${info.path}`, certHash: info.hash };
};
const status = (msg) => {
  const el = document.getElementById("boot-status");
  if (el) el.textContent = msg;
};

async function pickGraphics() {
  const forced = params.get("gfx");
  if (forced === "webgl2" || forced === "webgpu") return forced;
  try {
    if (navigator.gpu && (await navigator.gpu.requestAdapter())) {
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

async function main() {
  const showcase = params.get("showcase") || "";
  const renderer = rendererName();
  const qualityParam = (params.get("quality") || "").toLowerCase();
  const qualityAuto = autoQuality(renderer);
  const config = {
    autopilot: params.get("autopilot") === "1",
    autoplay: params.get("autoplay") === "1",
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
  };
  if (!showcase) {
    if (!("WebTransport" in window)) {
      // The game still loads (the title screen says what's wrong); the echo spike can't.
      if (config.echo) {
        status("this browser has no WebTransport: use Chrome or Edge");
        return;
      }
      config.noWebTransport = true;
    } else if (config.echo) {
      Object.assign(config, await window.bcDiscover());
    }
  }
  window.BC_CONFIG = config;
  const gfx = await pickGraphics();
  window.BC_CONFIG.gfx = gfx;
  status(`loading ${gfx} build (${config.quality})…`);
  const mod = await import(`./${gfx}/bc.js`);
  // Bevy's App::run hands control to the browser event loop by throwing a control-flow
  // exception on some targets; that is expected.
  try {
    await mod.default();
  } catch (e) {
    if (!String(e).includes("Using exceptions for control flow")) throw e;
  }
  document.getElementById("boot")?.classList.add("hidden");
}

main().catch((e) => {
  console.error(e);
  status(`failed: ${e}`);
});
