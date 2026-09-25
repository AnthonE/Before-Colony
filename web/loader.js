// Boots the Bevy client: asks the dev server for the WebTransport port and certificate hash,
// picks the WebGPU or WebGL2 build and a graphics tier, and hands the launch config to Rust via
// window.BC_CONFIG. `?showcase=<scene>` runs an offline scene and needs no game server.
const params = new URLSearchParams(location.search);
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

// `?quality=low|medium|high|ultra`, or `auto` (default): Low on software rasterisers, else High.
function pickQuality(renderer) {
  const q = (params.get("quality") || "auto").toLowerCase();
  if (["low", "medium", "high", "ultra"].includes(q)) return q;
  if (renderer === "none" || /swiftshader|llvmpipe|softpipe|software|basic render/i.test(renderer)) {
    return "low";
  }
  return "high";
}

async function main() {
  const showcase = params.get("showcase") || "";
  const renderer = rendererName();
  const config = {
    autopilot: params.get("autopilot") === "1",
    echo: params.get("mode") === "echo",
    lowQuality: params.get("quality") === "low",
    quality: pickQuality(renderer),
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
      status("this browser has no WebTransport: use Chrome or Edge");
      return;
    }
    const info = await (await fetch("/cert-hash")).json();
    config.wtUrl = `https://${location.hostname}:${info.port}${info.path}`;
    config.certHash = info.hash;
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
