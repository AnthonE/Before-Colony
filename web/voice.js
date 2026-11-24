// Proximity voice in the page (`window.bcVoice`): the microphone through the browser's own Opus
// (WebCodecs) out to the game, and each talker's frames decoded and played a little behind, so a
// late one still fits (60 ms of preroll, never more than 300 ms behind). Ported from oneshot's
// voice.js (itself after Gates' talk.rs). The game (Rust, `crates/bc-client/src/voice.rs`) decides
// the rest: when the pilot is on the air (`setKeyed`), who is heard and from where (`hear`,
// `place`), and who is muted. The server relays a frame only to who's near and never decodes one.
// Nothing is recorded.
//
// Through the air (on foot), a voice plays through a PannerNode the game moves each frame, at the
// loudness it says. Over a suit's radio (in space) there's no air: it plays through a band-pass, a
// radio's 300 Hz to 3.4 kHz, with a little drive; the game plays the squelch.
//
// It plays into `window.bcVoiceBus` (the game's mix, ahead of its low-pass) on `window.bcAudio`, the
// game's context at 48 kHz; frames go out through `window.bcVoiceUp(seq, end, bytes)`.
(() => {
  "use strict";
  const PREROLL_S = 0.06;
  const MAX_LAG_S = 0.3;
  // Lanes nobody has spoken on for this long go (their decoder closed), ms.
  const IDLE_MS = 5000;
  // The server relays at most 8 talkers to anyone; a ninth lane would be a stale one.
  const LANES = 8;

  const works = () => typeof AudioEncoder === "function" && typeof AudioDecoder === "function"
    && !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia);

  // What a failed getUserMedia (or the rest of opening) comes to.
  function whyNot(e) {
    switch (e && e.name) {
      case "NotAllowedError":
      case "SecurityError":
        return "denied";
      case "NotFoundError":
      case "OverconstrainedError":
        return "none";
      case "NotReadableError":
      case "AbortError":
        return "busy";
      case "NotSupportedError":
        return "unsupported";
      default:
        return "failed";
    }
  }

  // A little drive for the radio: soft clipping, as a cheap set's speaker does.
  function driveCurve() {
    const n = 1024;
    const c = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      const x = (i / (n - 1)) * 2 - 1;
      c[i] = Math.tanh(2.2 * x) / Math.tanh(2.2);
    }
    return c;
  }

  const voice = {
    // The game's switch (SETTINGS > SOUND): off, the microphone is let go and nothing plays.
    on: true,
    // The talk key, as the game has it.
    keyed: false,
    // The microphone: off, asking, on, denied, none, busy, unsupported, failed.
    mic: works() ? "off" : "unsupported",
    lanes: new Map(), // key -> {dec, next, gain, panner, t, heard, radio}
    capture: null, // {stream, node, enc, src, sink}
    opening: null,
    seq: 0,
    // Each frame's end flag, by its timestamp: an encoder that loses a frame never shifts the flag
    // onto another.
    ends: new Map(),
    // Frames played and sent (the dev hooks and the tests read them).
    heard: 0,
    sent: 0,
    curve: null,

    ctx() {
      const c = window.bcAudio;
      return c && window.bcVoiceBus ? c : null;
    },

    configure(on) {
      this.on = !!on;
      if (!this.on) {
        this.keyed = false;
        this.micOff();
        for (const key of [...this.lanes.keys()]) this.drop(key);
      }
    },

    setKeyed(keyed) {
      this.keyed = !!keyed && this.on;
      // The first press asks for the microphone (and the browser asks the player).
      if (this.keyed && !this.capture) this.micOn();
    },

    async micOn() {
      if (this.capture || this.opening || !this.on) return;
      if (!works()) {
        this.mic = "unsupported";
        return;
      }
      this.mic = "asking";
      this.opening = this.open().then(
        () => {
          this.mic = "on";
          // Turned off while it was opening: let it go at once.
          if (!this.on) this.micOff();
        },
        (e) => {
          this.mic = whyNot(e);
        },
      ).finally(() => {
        this.opening = null;
      });
    },

    async open() {
      const ctx = this.ctx();
      if (!ctx) throw Object.assign(new Error("no audio"), { name: "NotSupportedError" });
      if (ctx.state === "suspended") {
        try { await ctx.resume(); } catch { /* the gesture that keyed it may yet count */ }
      }
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 },
      });
      try {
        await this.openWith(ctx, stream);
      } catch (e) {
        // The browser's microphone light goes out with what failed.
        for (const tr of stream.getTracks()) tr.stop();
        throw e;
      }
    },

    async openWith(ctx, stream) {
      await ctx.audioWorklet.addModule("voice-worklet.js");
      const src = ctx.createMediaStreamSource(stream);
      const node = new AudioWorkletNode(ctx, "bc-mic", { processorOptions: { frame: Math.round(ctx.sampleRate / 50) } });
      const sink = ctx.createGain();
      sink.gain.value = 0;
      src.connect(node).connect(sink).connect(ctx.destination);
      const output = (chunk) => {
        const bytes = new Uint8Array(chunk.byteLength);
        chunk.copyTo(bytes);
        // Its own flag, and any of frames the encoder lost before it.
        let end = false;
        for (const [ts, e] of this.ends) {
          if (ts > chunk.timestamp) break;
          end = end || e;
          this.ends.delete(ts);
        }
        const up = window.bcVoiceUp;
        if (typeof up === "function" && up(this.seq, end, bytes)) this.sent++;
        this.seq = (this.seq + 1) & 0xffff;
      };
      const config = { codec: "opus", sampleRate: ctx.sampleRate, numberOfChannels: 1, bitrate: 24000 };
      const encoder = () => {
        // An encoder error closes it (WebCodecs): a new one takes over at the next frame.
        const e = new AudioEncoder({ output, error: () => {} });
        try {
          e.configure({ ...config, opus: { frameDuration: 20000, application: "voip", signal: "voice" } });
        } catch {
          e.configure({ ...config, opus: { frameDuration: 20000 } });
        }
        return e;
      };
      let enc = encoder();
      let t = 0;
      let was = false;
      // Each frame goes back to the tap once it's copied (AudioData copies what it's made from).
      const giveBack = (pcm) => {
        try { node.port.postMessage(pcm, [pcm.buffer]); } catch { /* gone */ }
      };
      node.port.onmessage = (e) => {
        const pcm = e.data;
        const want = this.keyed && this.on;
        if (!want && !was) {
          giveBack(pcm);
          return;
        }
        // The last frame of a go at talking: silence, flagged, so the listeners' marks go out.
        const end = was && !want;
        const data = end ? new Float32Array(pcm.length) : pcm;
        if (enc.state === "closed" && this.capture) {
          this.ends.clear();
          try { enc = encoder(); this.capture.enc = enc; } catch { /* the next frame tries again */ }
        }
        if (enc.state === "configured") {
          this.ends.set(t, end);
          enc.encode(new AudioData({
            format: "f32-planar", sampleRate: ctx.sampleRate, numberOfFrames: data.length,
            numberOfChannels: 1, timestamp: t, data,
          }));
          t += Math.round((data.length / ctx.sampleRate) * 1e6);
        }
        giveBack(pcm);
        was = want;
      };
      this.capture = { stream, node, enc, src, sink };
    },

    micOff() {
      const c = this.capture;
      this.capture = null;
      this.ends.clear();
      if (c) {
        for (const tr of c.stream.getTracks()) tr.stop();
        try { c.src.disconnect(); c.node.disconnect(); c.sink.disconnect(); c.enc.close(); } catch { /* gone already */ }
      }
      if (this.mic === "on" || this.mic === "asking") this.mic = "off";
    },

    // A talker's lane: a decoder of their own, and how they're heard.
    lane(key, radio) {
      const ctx = this.ctx();
      let l = this.lanes.get(key);
      if (l && l.radio === radio) return l;
      if (l) this.drop(key);
      if (this.lanes.size >= LANES) {
        // The quietest goes.
        let oldest = null;
        for (const [k, x] of this.lanes) if (!oldest || x.heard < this.lanes.get(oldest).heard) oldest = k;
        this.drop(oldest);
      }
      const gain = ctx.createGain();
      let panner = null;
      if (radio) {
        const hp = ctx.createBiquadFilter();
        hp.type = "highpass";
        hp.frequency.value = 300;
        const lp = ctx.createBiquadFilter();
        lp.type = "lowpass";
        lp.frequency.value = 3400;
        const drive = ctx.createWaveShaper();
        this.curve = this.curve || driveCurve();
        drive.curve = this.curve;
        gain.connect(hp).connect(lp).connect(drive).connect(window.bcVoiceBus);
        gain.gain.value = 0.9;
      } else {
        panner = ctx.createPanner();
        panner.panningModel = "equalpower";
        panner.distanceModel = "linear";
        panner.rolloffFactor = 0;
        panner.positionZ.value = -1;
        gain.connect(panner).connect(window.bcVoiceBus);
        gain.gain.value = 0;
      }
      l = { next: 0, gain, panner, radio, t: 0, heard: performance.now(), dec: null, pcm: null };
      l.dec = new AudioDecoder({
        output: (ad) => this.play(l, ad),
        // Made again by their next frame.
        error: () => this.drop(key),
      });
      l.dec.configure({ codec: "opus", sampleRate: 48000, numberOfChannels: 1 });
      this.lanes.set(key, l);
      return l;
    },

    drop(key) {
      const l = this.lanes.get(key);
      if (!l) return;
      this.lanes.delete(key);
      try { l.dec.close(); } catch { /* closed already */ }
      try { l.gain.disconnect(); if (l.panner) l.panner.disconnect(); } catch { /* gone already */ }
    },

    // A relayed frame (`key`: how it came and who, `air:12`, `radio:301`), decoded and played
    // behind their last one.
    hear(key, radio, seq, end, bytes) {
      const ctx = this.ctx();
      // Asleep (no gesture yet), or off: dropped, not piled up to play at once later.
      if (!this.on || !ctx || ctx.state !== "running" || typeof AudioDecoder !== "function") return;
      const now = performance.now();
      for (const [k, x] of this.lanes) if (now - x.heard > IDLE_MS) this.drop(k);
      const l = this.lane(key, !!radio);
      l.heard = now;
      if (l.dec.state === "configured") {
        l.dec.decode(new EncodedAudioChunk({ type: "key", timestamp: l.t, data: bytes }));
        l.t += 20000;
      }
    },

    play(l, ad) {
      const ctx = this.ctx();
      if (!ctx) { ad.close(); return; }
      const n = ad.numberOfFrames;
      const buf = ctx.createBuffer(1, n, ad.sampleRate);
      if (!l.pcm || l.pcm.length !== n) l.pcm = new Float32Array(n);
      try {
        ad.copyTo(l.pcm, { planeIndex: 0, format: "f32-planar" });
      } catch {
        try { ad.copyTo(l.pcm, { planeIndex: 0 }); } catch { ad.close(); return; }
      }
      ad.close();
      buf.copyToChannel(l.pcm, 0);
      const now = ctx.currentTime;
      // Behind by more than the cap, or caught up: start again at the preroll.
      if (l.next < now || l.next - now > MAX_LAG_S) l.next = now + PREROLL_S;
      const s = ctx.createBufferSource();
      s.buffer = buf;
      s.connect(l.gain);
      s.start(l.next);
      l.next += n / ad.sampleRate;
      this.heard++;
    },

    // Where each voice through the air is now, from the camera (x right, y up, z behind: the
    // listener's own axes), and how loud: `at` holds x, y, z and the gain for each of `keys`.
    place(keys, at) {
      const ctx = this.ctx();
      if (!ctx) return;
      const when = ctx.currentTime;
      for (let i = 0; i < keys.length; i++) {
        const l = this.lanes.get(keys[i]);
        if (!l || !l.panner) continue;
        const k = i * 4;
        l.panner.positionX.setTargetAtTime(at[k], when, 0.03);
        l.panner.positionY.setTargetAtTime(at[k + 1], when, 0.03);
        l.panner.positionZ.setTargetAtTime(at[k + 2], when, 0.03);
        l.gain.gain.setTargetAtTime(at[k + 3], when, 0.05);
      }
    },
  };

  window.bcVoice = voice;

  // The talk key includes the mouse's back button, which a browser would otherwise take as Back,
  // out of the game (and its forward button as Forward). After Gates' app.js.
  for (const type of ["mousedown", "mouseup", "auxclick"]) {
    addEventListener(type, (e) => {
      if (e.button === 3 || e.button === 4) e.preventDefault();
    });
  }
})();
