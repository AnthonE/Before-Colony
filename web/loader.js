// Boots the Bevy client: asks the dev server for the WebTransport port and certificate hash,
// picks the WebGPU or WebGL2 build, and hands the launch config to Rust via window.BC_CONFIG.
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

async function main() {
  if (!("WebTransport" in window)) {
    status("this browser has no WebTransport: use Chrome or Edge");
    return;
  }
  const info = await (await fetch("/cert-hash")).json();
  window.BC_CONFIG = {
    wtUrl: `https://${location.hostname}:${info.port}${info.path}`,
    certHash: info.hash,
    autopilot: params.get("autopilot") === "1",
    echo: params.get("mode") === "echo",
    lowQuality: params.get("quality") === "low",
    name: params.get("name") || "",
    frame: params.get("frame") || "",
  };
  const gfx = await pickGraphics();
  window.BC_CONFIG.gfx = gfx;
  status(`loading ${gfx} build…`);
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
