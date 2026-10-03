<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { themeColors } from './theme';

  export let isRecording = false;
  export let isCalibrating = false;
  export let level = 0.0; // External level from backend mic meter
  export let spectrum: number[] | null = null; // Real 128-band frequency spectrum from Rust CPAL

  $: void level;

  let canvas: HTMLCanvasElement;
  let animId: number;

  // Web Audio API & FFT configuration
  let audioCtx: AudioContext | null = null;
  let mediaStream: MediaStream | null = null;
  let analyser: AnalyserNode | null = null;
  let freqData: Uint8Array | null = null;

  // Exactly 128 bars representing MIDI notes 0–127
  const NUM_BARS = 128;
  const FFT_SIZE = 2048;

  // Real-time smoothed display levels for each bar (0.0 to 1.0)
  const currentLevels = new Float32Array(NUM_BARS);
  const rawTargets = new Float32Array(NUM_BARS);
  const smoothedTargets = new Float32Array(NUM_BARS);

  // Precomputed frequency band bounds for each of the 128 MIDI notes
  interface BandDef {
    m: number;
    fCenter: number;
    startBin: number;
    endBin: number;
    weight: number;
    subFilter: number;
  }
  let bands: BandDef[] = [];

  function computeBands(sampleRate: number) {
    const binHz = sampleRate / FFT_SIZE;
    bands = [];

    for (let m = 0; m < NUM_BARS; m++) {
      // 128 MIDI Note Frequencies: f(m) = 440 * 2^((m - 69) / 12)
      const fCenter = 440 * Math.pow(2, (m - 69) / 12);
      const fLow = 440 * Math.pow(2, (m - 0.5 - 69) / 12);
      const fHigh = 440 * Math.pow(2, (m + 0.5 - 69) / 12);

      const startBin = Math.max(1, Math.floor(fLow / binHz));
      const endBin = Math.max(startBin + 1, Math.ceil(fHigh / binHz));

      // Pink noise / equal-loudness tilt compensation:
      // High-register harmonics naturally carry lower acoustic amplitude than low fundamentals.
      // Progressive acoustic weighting ensures voice harmonics rise with clear visual presence.
      const normM = m / (NUM_BARS - 1);
      const weight = 0.88 + Math.pow(normM, 0.72) * 0.54;

      // Sub-bass filter (< 35 Hz):
      // Human vocal fundamentals begin at ~65Hz. We attenuate sub-acoustic vibrations to prevent desk/fan rumble.
      let subFilter = 1.0;
      if (fCenter < 18) {
        subFilter = 0.0;
      } else if (fCenter < 40) {
        subFilter = Math.pow((fCenter - 18) / 22, 1.6);
      }

      bands.push({ m, fCenter, startBin, endBin, weight, subFilter });
    }
  }

  // Background noise cloud particles (matches misc/panel-1.png)
  interface NoiseParticle {
    xNorm: number;
    yNorm: number; // -1 to 1 relative to centerline
    size: number;
    baseAlpha: number;
    speed: number;
  }
  let noiseCloud: NoiseParticle[] = [];

  function initNoiseCloud() {
    noiseCloud = [];
    const count = 90;
    for (let i = 0; i < count; i++) {
      noiseCloud.push({
        xNorm: Math.random(),
        yNorm: (Math.random() - 0.5) * 1.5,
        size: Math.random() < 0.2 ? 1.8 : 1.2,
        baseAlpha: 0.04 + Math.random() * 0.12,
        speed: 0.3 + Math.random() * 0.7,
      });
    }
  }

  async function initMicrophone() {
    // In Tauri (desktop and Android), CPAL directly captures hardware audio
    // and feeds real-time spectrum & level data. Calling getUserMedia in WebView
    // contends with native AAudio HAL and blocks initial audio capture on Android.
    if (typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window)) {
      return;
    }

    try {
      const AudioContextClass =
        window.AudioContext ||
        (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      if (!AudioContextClass) return;

      audioCtx = new AudioContextClass();
      computeBands(audioCtx.sampleRate || 48000);

      // Request raw live microphone input
      const stream = await navigator.mediaDevices?.getUserMedia({
        audio: {
          echoCancellation: false,
          noiseSuppression: false,
          autoGainControl: false,
        },
      });
      mediaStream = stream;

      const sourceNode = audioCtx.createMediaStreamSource(stream);
      analyser = audioCtx.createAnalyser();
      analyser.fftSize = FFT_SIZE;
      // Speech dynamic window: -85 dBFS noise floor to -25 dBFS speech peaks
      analyser.minDecibels = -85;
      analyser.maxDecibels = -25;
      analyser.smoothingTimeConstant = 0.0;

      sourceNode.connect(analyser);
      freqData = new Uint8Array(analyser.frequencyBinCount);

      if (audioCtx.state === 'suspended') {
        const resumeAudio = async () => {
          if (audioCtx && audioCtx.state === 'suspended') {
            await audioCtx.resume();
          }
          window.removeEventListener('click', resumeAudio);
          window.removeEventListener('pointerdown', resumeAudio);
          window.removeEventListener('keydown', resumeAudio);
        };
        window.addEventListener('click', resumeAudio);
        window.addEventListener('pointerdown', resumeAudio);
        window.addEventListener('keydown', resumeAudio);
      }
    } catch (err) {
      console.warn('Live mic Web Audio stream not directly available; fallback to backend meter:', err);
    }
  }

  onMount(() => {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    computeBands(48000);
    initNoiseCloud();

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(rect.width * dpr);
      canvas.height = Math.round(rect.height * dpr);
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.scale(dpr, dpr);
    };

    resize();
    window.addEventListener('resize', resize);
    initMicrophone();

    let lastTime = performance.now();

    const render = () => {
      const now = performance.now();
      const dt = Math.min(0.05, Math.max(0.001, (now - lastTime) / 1000));
      lastTime = now;

      // Ensure AudioContext is active during recording or calibration
      if (audioCtx && audioCtx.state === 'suspended' && (isRecording || isCalibrating)) {
        audioCtx.resume().catch(() => {});
      }

      const rect = canvas.getBoundingClientRect();
      const width = rect.width;
      const height = rect.height;
      const centerY = height / 2;

      ctx.clearRect(0, 0, width, height);

      // 1. Process Genuine Live Audio Spectrum from Rust Backend (CPAL hardware mic)
      if (spectrum && spectrum.length === NUM_BARS) {
        for (let m = 0; m < NUM_BARS; m++) {
          rawTargets[m] = spectrum[m] || 0.0;
        }
      } else {
        rawTargets.fill(0);
      }

      // 2. Alternatively / additionally process Web Audio AnalyserNode (e.g. in browser)
      if (analyser && freqData) {
        analyser.getByteFrequencyData(freqData as unknown as Uint8Array<ArrayBuffer>);

        for (let m = 0; m < NUM_BARS; m++) {
          const band = bands[m];
          if (!band) continue;

          let maxVal = 0;
          let sumVal = 0;
          let count = 0;

          const end = Math.min(freqData.length, band.endBin);
          for (let b = band.startBin; b < end; b++) {
            const val = freqData[b];
            if (val > maxVal) maxVal = val;
            sumVal += val;
            count++;
          }

          const avgVal = count > 0 ? sumVal / count : 0;
          const blended = (0.75 * maxVal + 0.25 * avgVal) / 255.0;
          let signal = blended * band.weight * band.subFilter;

          const NOISE_FLOOR = 0.055;
          if (signal <= NOISE_FLOOR) {
            signal = 0.0;
          } else {
            signal = (signal - NOISE_FLOOR) / (1.0 - NOISE_FLOOR);
            signal = Math.min(1.0, Math.pow(signal, 0.76) * 1.38);
          }

          if (signal > rawTargets[m]) {
            rawTargets[m] = signal;
          }
        }
      }

      // 3. Organic spatial smoothing across adjacent micro-semitones
      // Delivers silky fluid movement during vocal vibrato while preserving independent harmonics
      for (let m = 0; m < NUM_BARS; m++) {
        const left = m > 0 ? rawTargets[m - 1] : rawTargets[m];
        const center = rawTargets[m];
        const right = m < NUM_BARS - 1 ? rawTargets[m + 1] : rawTargets[m];
        smoothedTargets[m] = 0.18 * left + 0.64 * center + 0.18 * right;
      }

      // 4. Exact Dynamic Envelopes:
      // - Fast Attack: 20-40 ms (tactile rise when sound starts)
      // - Fast Fall: 150-180 ms (quick decay to dots when sound stops)
      const ATTACK_RATE = 0.65; // ~90% rise in ~30 ms
      const DECAY_RATE = 0.22; // ~90% drop in ~160 ms

      let maxActiveLevel = 0.0;
      for (let m = 0; m < NUM_BARS; m++) {
        const target = smoothedTargets[m];
        if (target > currentLevels[m]) {
          currentLevels[m] += (target - currentLevels[m]) * ATTACK_RATE;
        } else {
          currentLevels[m] += (target - currentLevels[m]) * DECAY_RATE;
          // Drop cleanly to zero in silence
          if (currentLevels[m] < 0.012) {
            currentLevels[m] = 0.0;
          }
        }
        if (currentLevels[m] > maxActiveLevel) {
          maxActiveLevel = currentLevels[m];
        }
      }

      // 5. Draw Noise Cloud Background (from misc/panel-1.png)
      // Subtle grey/green static grain showing "noise being separated from voice"
      const cloudAmp = Math.max(0.12, maxActiveLevel);
      for (let i = 0; i < noiseCloud.length; i++) {
        const p = noiseCloud[i];
        const px = p.xNorm * width;
        const py = centerY + p.yNorm * (24 + cloudAmp * 32);

        // Faint noise grain
        const pAlpha = p.baseAlpha * (0.6 + cloudAmp * 0.8) * $themeColors.particleAlphaBase;
        ctx.fillStyle = `rgba(${$themeColors.particleRgbPrefix}${pAlpha.toFixed(3)})`;
        ctx.beginPath();
        ctx.arc(px, py, p.size, 0, Math.PI * 2);
        ctx.fill();
      }

      // 6. Draw 128 Symmetrical Bars using active theme accent
      const barSpacing = width / (NUM_BARS + 1);
      const barWidth = Math.min(3.4, Math.max(2.2, barSpacing * 0.44));
      const dotRadius = barWidth / 2;
      const maxBarHeight = height * 0.78;

      ctx.fillStyle = $themeColors.accentHex;
      ctx.shadowBlur = 0;

      for (let m = 0; m < NUM_BARS; m++) {
        const x = (m + 1) * barSpacing;
        const amp = currentLevels[m];

        if (amp <= 0.001) {
          // Silence: clean circular dot along the horizontal centerline
          ctx.beginPath();
          ctx.arc(x, centerY, dotRadius, 0, Math.PI * 2);
          ctx.fill();
        } else {
          // Sound: symmetrical capsule extending upward and downward from centerline
          const h = barWidth + amp * (maxBarHeight - barWidth);
          const top = centerY - h / 2;

          ctx.beginPath();
          if (typeof ctx.roundRect === 'function') {
            ctx.roundRect(x - dotRadius, top, barWidth, h, dotRadius);
          } else {
            ctx.arc(x, top + dotRadius, dotRadius, Math.PI, 0);
            ctx.arc(x, top + h - dotRadius, dotRadius, 0, Math.PI);
            ctx.closePath();
          }
          ctx.fill();
        }
      }

      ctx.shadowBlur = 0;
      animId = requestAnimationFrame(render);
    };

    animId = requestAnimationFrame(render);

    return () => {
      cancelAnimationFrame(animId);
      window.removeEventListener('resize', resize);
    };
  });

  onDestroy(() => {
    if (animId) cancelAnimationFrame(animId);
    if (mediaStream) {
      mediaStream.getTracks().forEach((track) => track.stop());
    }
    if (audioCtx && audioCtx.state !== 'closed') {
      audioCtx.close().catch(() => {});
    }
  });
</script>

<div class="waveform-container">
  <canvas bind:this={canvas}></canvas>
</div>

<style>
  .waveform-container {
    width: 100%;
    height: clamp(80px, 18vh, 140px);
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    /* Soft gradient vignette on extreme edges matching panel-1.png */
    mask-image: linear-gradient(
      to right,
      transparent 0%,
      black 36px,
      black calc(100% - 36px),
      transparent 100%
    );
    -webkit-mask-image: linear-gradient(
      to right,
      transparent 0%,
      black 36px,
      black calc(100% - 36px),
      transparent 100%
    );
  }

  @media (max-height: 540px) {
    .waveform-container {
      height: clamp(60px, 14vh, 90px);
    }
  }

  canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
