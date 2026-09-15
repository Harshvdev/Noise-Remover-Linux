# Offline Voice & Singing Noise Remover
## Linux v1 Architecture and Implementation Plan

**Status:** Corrected architecture
**Target:** Ubuntu/Linux first; architecture prepared for Windows and Android
**Primary processing model:** CPU-first, offline, low-memory, full-band
**Audio scope:** Speech or singing + unwanted background noise; no music-preservation/source-separation requirement
**Reference hardware:** Ryzen 5 5500U-class CPU, 8 GB RAM, integrated graphics
**Baseline hardware target:** 4 GB RAM, CPU-only operation

---

# 1. Product Definition

The application records or imports speech/singing audio and reduces unwanted background noise while preserving the vocal recording as faithfully as possible.

Typical recordings:

- voice + laptop fan
- singing + air conditioner
- voice + electrical hum
- singing + hiss
- voice + traffic
- singing + room ambience
- voice + keyboard noise
- singing + miscellaneous environmental noise
- voice/singing + weak background music, with the music treated as unwanted background sound

The application is **not** designed for:

- music preservation
- separate vocal/music stem extraction
- music removal using source separation
- real-time noise cancellation
- cloud inference
- speech transcription
- voice conversion
- automatic mastering

The processing target is:

```text
unwanted background sound ↓
vocal alteration ↓
```

The first priority is preserving the vocal recording. Maximum possible noise removal is intentionally not the primary optimization target.

---

# 2. Architectural Principles

## 2.1 Least-destructive processing first

The application must not immediately send the complete recording through a neural network.

Instead:

```text
Original
  ↓
Analyze noise
  ↓
Remove predictable noise with deterministic DSP
  ↓
Measure residual noise
  ↓
Use one neural denoiser only when necessary
  ↓
Align original/processed signals
  ↓
Conservative preservation blend
  ↓
Output
```

This has several advantages:

- stationary fan/AC/hum can often be reduced without neural processing;
- CPU usage stays low on simple recordings;
- AI is not unnecessarily applied to clean vocal regions;
- the original signal remains available as a reference;
- model artifacts are easier to control.

Classical spectral suppression is not risk-free: aggressive spectral gating can cause musical-noise, metallic, watery, or other artifacts and can remove wanted signal. It is selected because it is more deterministic and controllable, not because it is guaranteed not to damage vocals.

---

## 2.2 Original audio is immutable

The recorder creates an original master:

```text
original.wav
```

This file is never modified.

Processing creates separate files:

```text
original.wav
cleaned.wav
removed_noise.wav
```

This allows A/B comparison and makes experimentation safe.

---

## 2.3 Offline processing is intentional

Recording is real-time only because microphone capture must happen in real time.

Noise removal itself is batch/offline.

This allows:

- larger processing context;
- quality-oriented algorithms;
- sequential model loading;
- lower memory usage than keeping the entire file in RAM;
- no real-time deadline for expensive processing.

---

## 2.4 CPU-first architecture

The application must work without CUDA, ROCm, or a dedicated GPU.

GPU acceleration may be added later as an optional optimization, but the core product must not require it.

This is essential for portability across:

- low-end Linux PCs;
- Windows laptops;
- integrated-graphics systems;
- Android devices with different GPU vendors.

---

## 2.5 Full-band processing

The primary processing representation is:

```text
48,000 Hz
Float32
mono
```

A full-band 48 kHz denoiser is preferred so that processing does not discard everything above 8 kHz.

This is particularly important for singing because high-frequency vocal information contributes to:

- sibilance;
- breath;
- brightness;
- vocal harmonics;
- perceived air.

The standard `gtcrn_simple.onnx` model distributed through sherpa-onnx is a 16 kHz model, so it is **not** the primary model for the 48 kHz full-band pipeline. A 48 kHz GTCRN deployment should not be assumed without an independently verified 48 kHz checkpoint.

---

# 3. High-Level Architecture

```text
                         ┌──────────────────────┐
                         │        GUI           │
                         │      egui/eframe     │
                         └──────────┬───────────┘
                                    │
                                    ▼
                         ┌──────────────────────┐
                         │   Application Core   │
                         │        Rust          │
                         └──────────┬───────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              │                     │                     │
              ▼                     ▼                     ▼
       ┌──────────────┐      ┌──────────────┐      ┌──────────────┐
       │   Recorder  │      │    Analyzer  │      │  Job Manager │
       └──────┬───────┘      └──────┬───────┘      └──────┬───────┘
              │                     │                     │
              ▼                     ▼                     │
           CPAL              Noise characterization      │
              │                     │                     │
          ring buffer               │                     │
              │                     │                     │
              ▼                     └──────────┬──────────┘
        original.wav                            │
                                               ▼
                                      ┌──────────────────┐
                                      │ Processing Core  │
                                      └────────┬─────────┘
                                               │
                                       Canonical audio
                                               │
                                               ▼
                                         DSP pass
                                               │
                                               ▼
                                      Residual analysis
                                               │
                              ┌────────────────┴────────────────┐
                              │                                 │
                           low residual                      high residual
                              │                                 │
                              ▼                                 ▼
                           finish                      neural backend
                                                               │
                                                    ┌──────────┴──────────┐
                                                    │                     │
                                              DPDFNet2-48k         DeepFilterNet3
                                                    │                     │
                                                    └──────────┬──────────┘
                                                               │
                                                          one selected
                                                               │
                                                               ▼
                                                       latency alignment
                                                               │
                                                               ▼
                                                      vocal preservation
                                                               │
                                                       ┌───────┴───────┐
                                                       │               │
                                                       ▼               ▼
                                                  cleaned.wav   removed_noise.wav
```

---

# 4. Required Components Before Selecting Libraries

The product requires the following capabilities:

| Capability | Purpose |
|---|---|
| Microphone capture | Record the source vocal signal |
| Input-device enumeration | Select a microphone |
| Capture configuration | Choose a stable native device format |
| Calibration recording | Capture background-noise-only reference |
| Real-time level meter | Show recording level without affecting capture |
| Clipping detector | Detect overloaded microphone input |
| Ring buffer | Transfer audio safely out of the audio callback |
| Disk writer | Save samples without blocking capture |
| WAV reader/writer | Store lossless audio |
| Sample conversion | Convert device data to Float32 |
| Resampling | Convert processing data to 48 kHz |
| Channel conversion | Convert to mono processing representation |
| STFT/FFT | Analyze and modify time-frequency content |
| Noise profile | Estimate background noise |
| Stationarity detector | Determine whether noise is stable |
| Tonal detector | Detect hum and narrow-band noise |
| Vocal/activity detector | Protect likely vocal regions |
| Classical suppression | Remove predictable noise cheaply |
| Neural inference | Remove residual complex noise |
| Latency accounting | Keep dry/wet paths synchronized |
| Preservation mixer | Limit unwanted vocal alteration |
| Chunked processing | Keep RAM usage low |
| Stateful inference | Avoid restarting recurrent model state per chunk |
| Playback | Listen to source/result |
| A/B comparison | Compare original and cleaned versions |
| Noise-only playback | Inspect what the algorithm removed |
| Waveform cache | Efficient waveform display |
| Output export | Save final audio |
| Error handling | Survive device and processing failures |

The architecture must be designed from these requirements outward rather than selecting libraries first and forcing the product around them.

---

# 5. Technology Selection

## 5.1 Application language: Rust

Rust is selected for the main application because:

- the recording layer is native;
- the DSP layer can be native;
- memory ownership is explicit;
- there is no garbage-collector pause;
- the same codebase can target Linux and Windows;
- Android support is possible through native Rust/NDK integration;
- the ecosystem already contains the required audio primitives.

Rust is not selected because every audio algorithm must be handwritten. Existing native/model components remain preferable wherever they are small, mature, and verifiable.

---

## 5.2 GUI: egui + eframe

Use `egui` with `eframe` for Linux v1.

`eframe` is the official framework around `egui` and currently supports native Linux, Windows, macOS and Android builds. It also has Linux Wayland support. This makes it suitable for the intended future platform set.

GUI responsibilities:

- microphone selection;
- recording controls;
- calibration indicator;
- level meter;
- waveform display;
- processing settings;
- processing progress;
- original/cleaned/noise-only playback;
- export controls;
- error display.

The GUI must never perform heavy DSP or neural inference on its rendering thread.

### Linux rendering

The first build should use the default native renderer configuration supplied by eframe unless the target environment demonstrates a rendering problem. eframe supports Wayland on Linux.

---

# 6. Recorder Architecture

The recorder is part of the application, but it should not contain the denoising system.

The recorder's only job is to capture a high-quality source.

```text
                 Microphone
                     │
                     ▼
              Operating system
                     │
                     ▼
                    CPAL
                     │
               audio callback
                     │
                     ▼
                ring buffer
                     │
          ┌──────────┴──────────┐
          │                     │
          ▼                     ▼
     writer thread          statistics
          │                     │
          ▼                     ▼
      original.wav            GUI
```

---

# 7. Recorder Component: CPAL

Use **CPAL** as the initial Rust audio I/O library.

CPAL provides low-level cross-platform audio I/O and currently supports Linux audio backends including ALSA and PipeWire, Windows WASAPI, and Android AAudio.

This is a strong fit because the capture abstraction can remain the same while platform-specific audio systems change underneath.

### Important limitation

The application can avoid adding its own processing, but it cannot universally guarantee that a microphone, driver, operating-system audio source, or hardware device has never applied processing before CPAL receives the data. The implementation should therefore:

1. avoid adding application-level AGC/noise suppression/AEC/compression;
2. prefer a normal raw/least-processed capture source where the platform exposes one;
3. document that hardware/driver processing may still exist.

---

# 8. Recorder Device Enumeration

At startup:

```text
CPAL host
  ↓
input-device enumeration
  ↓
supported configurations
  ↓
default input
  ↓
GUI device list
```

The UI should show something similar to:

```text
Input Device

● Built-in Microphone
  48000 Hz · Mono

○ USB Microphone
  44100 Hz · Mono

○ Headset Microphone
  48000 Hz · Stereo
```

The application should inspect supported configurations instead of assuming that every microphone supports 48 kHz Float32 mono.

---

# 9. Native Capture Format

The recorder should normally use a stable format supported naturally by the selected device.

Do not force every device into 48 kHz inside the audio callback.

Examples:

```text
Device A:
48 kHz / Float32 / Mono
      ↓
use directly
```

```text
Device B:
44.1 kHz / i16 / Mono
      ↓
capture as supported
      ↓
convert later
```

```text
Device C:
48 kHz / i16 / Stereo
      ↓
capture as supported
      ↓
convert later
```

The recording callback should do only lightweight conversion necessary to transfer samples safely.

---

# 10. Audio Callback Rules

The audio callback is a real-time context.

The following operations should **not** occur in it:

- FFT/STFT;
- neural inference;
- disk I/O;
- dynamic allocation;
- mutex-heavy coordination;
- resampling of large blocks;
- file encoding;
- waveform generation;
- UI rendering.

The callback should effectively perform:

```text
receive device buffer
      ↓
convert sample representation if necessary
      ↓
push into preallocated ring buffer
      ↓
update minimal atomic counters
      ↓
return
```

This isolates capture from everything that can block.

---

# 11. Ring Buffer: ringbuf

Use the Rust `ringbuf` crate as the capture-to-worker transport.

It provides a lock-free SPSC FIFO ring buffer.

Architecture:

```text
CPAL callback
    │
    ▼
Producer
    │
    ▼
┌───────────────────┐
│ preallocated FIFO │
└───────────────────┘
    │
    ▼
Consumer
    │
    ▼
writer / monitor worker
```

For this project, a single producer and single consumer are sufficient.

The buffer size should be selected from measured device callback behavior rather than arbitrarily made enormous. It should absorb normal scheduling jitter without becoming a substitute for proper thread design.

---

# 12. Disk Writer

The disk writer runs on a separate worker.

```text
ring buffer
    ↓
accumulate block
    ↓
WAV writer
    ↓
original.wav
```

The writer also updates:

```text
sample_count
peak
RMS
clipping_count
```

The GUI receives only small periodic statistics updates.

---

# 13. WAV: Hound

Use `hound` for initial WAV reading/writing.

WAV is ideal for v1 because it is:

- lossless;
- simple;
- easy to inspect;
- easy to test;
- compatible with the chosen processing ecosystem.

Internal processing should operate on Float32 samples even if the source WAV is integer PCM.

---

# 14. Recording Sequence

When the user presses Record:

```text
Record pressed
      ↓
Initialize capture stream
      ↓
Initialize ring buffer
      ↓
Start calibration
      ↓
Capture 1.5–2 seconds of noise
      ↓
Validate calibration
      ↓
Start actual recording
      ↓
Capture vocal audio
      ↓
Stop
      ↓
Finalize WAV
```

The calibration segment must be stored separately.

```text
session/
    original.wav
    noise_reference.wav
```

The calibration audio should not be appended to the final vocal recording.

---

# 15. Calibration UX

The first screen should show:

```text
Stay quiet

Calibrating background noise...
████████████████░░░░

Keep the microphone in the same position.
```

After successful calibration:

```text
Calibration complete

Recording...
```

The calibration period should preferably use the same microphone position and environment that will exist during the actual recording.

Example:

```text
Laptop fan ON
AC ON
Singer quiet
       ↓
2 sec calibration
       ↓
sing
```

The resulting profile is therefore representative of the actual background.

---

# 16. Calibration Validation

Calibration should not blindly become the noise model.

The calibration validator should look for:

- unexpectedly high energy;
- transient spikes;
- strong harmonic vocal-like structure;
- unstable spectrum;
- probable speech/singing activity.

A valid reference might be:

```text
constant fan
low-level room noise
50/60 Hz hum
```

An invalid reference might be:

```text
someone speaking
someone singing
keyboard strike
clap
chair impact
```

If calibration is invalid, the application should discard that profile rather than corrupting the noise estimate.

---

# 17. Noise Reference Representation

Do not repeatedly analyze the complete calibration waveform.

Convert it into a compact noise profile:

```text
NoiseProfile {
    sample_rate
    noise_power_per_bin
    robust_noise_level
    tonal_peaks
    stationarity_score
}
```

The main representation is the estimated noise power spectral density:

```text
N[k]
```

for frequency bin `k`.

A robust median or similarly resistant statistic should be used across calibration frames so a brief accidental sound does not become part of the permanent profile.

---

# 18. Audio Canonicalization

After recording/import:

```text
source format
     ↓
Float32
     ↓
channel conversion
     ↓
48 kHz
     ↓
mono processing stream
```

The original file remains untouched.

The processing engine receives a canonical representation:

```text
48,000 Hz
Float32
mono
```

---

# 19. Resampling: rubato

Use `rubato` for offline sample-rate conversion.

Example:

```text
44.1 kHz input
      ↓
rubato
      ↓
48 kHz processing stream
```

Resampling occurs outside the audio callback.

The resampler should be exercised with known test tones and frequency sweeps to ensure that the implementation does not introduce unacceptable aliasing or level errors.

---

# 20. Channel Conversion

For processing, ordinary vocal recordings are treated as mono.

If stereo is captured:

```text
Left + Right
     ↓
controlled mono downmix
     ↓
mono processing
```

The original stereo master remains untouched.

If stereo output becomes a product requirement later, the architecture can be extended to apply a common control mask to both channels or preserve a suitable stereo representation. It should not be invented into v1 unnecessarily.

---

# 21. DSP Analysis Layer

The analyzer converts audio into overlapping time-frequency frames using STFT.

```text
PCM
 ↓
window
 ↓
FFT
 ↓
complex spectrum X(t,k)
```

Each frame can provide:

- magnitude;
- power;
- phase;
- spectral flatness;
- spectral flux;
- harmonicity;
- similarity to noise profile.

---

# 22. FFT: rustfft + realfft

Use:

```text
rustfft
realfft
```

`rustfft` provides a pure-Rust FFT implementation and can use CPU SIMD paths on supported architectures. `realfft` provides convenient real-input/real-output transforms on top of RustFFT.

This combination is appropriate because the same Rust DSP layer can later run on:

```text
x86-64 Linux/Windows
ARM64 Android
```

without requiring a platform-specific FFT library.

---

# 23. STFT Configuration

The exact STFT parameters must eventually be tuned against the selected neural backend and DSP quality tests.

For the classical DSP path, a reasonable starting point is a short audio analysis window with overlap, e.g. roughly 20–40 ms depending on the stage.

For the neural model, **the model's required frame size and state representation must take precedence**. The application must not impose arbitrary external framing that conflicts with the model's expected streaming API.

---

# 24. Noise Characterization

The analyzer estimates:

```text
noise_level
noise_spectrum
stationarity
tonal_peaks
vocal_activity
residual_noise
```

The classifier is not intended to identify every sound semantically.

Its primary purpose is to answer:

1. Is the background predictable?
2. Can DSP safely handle a large part of it?
3. Is a neural backend worth invoking?
4. Which regions need more protection from aggressive attenuation?

---

# 25. Stationary vs Non-Stationary Noise

### Stationary examples

```text
fan
AC
constant hiss
refrigerator
computer hum
```

These tend to have relatively stable spectral characteristics.

### Non-stationary examples

```text
car passing
keyboard
person moving
changing wind
door closing
occasional external speech
```

These vary over time.

The processing strategy should depend on this distinction.

---

# 26. Tonal Noise Detector

The analyzer searches for persistent narrow-band peaks.

Examples:

```text
50 Hz
100 Hz
150 Hz
...
```

or:

```text
60 Hz
120 Hz
180 Hz
...
```

A tonal component is represented approximately as:


```text
TonalPeak {
    frequency
    strength
    confidence
}
```

Only strong, persistent, high-confidence peaks should receive narrow notch filtering.

The application must not blindly remove all energy around 50/60 Hz because vocal fundamentals can exist in this region.

---

# 27. DC Removal

A simple DC blocker/high-pass stage removes unwanted DC offset.

This should be extremely conservative and should not become an arbitrary vocal high-pass filter.

No broad vocal-frequency cutoff should be used merely to remove background noise.

---

# 28. Hum Removal

For a strong electrical hum:

```text
50/60 Hz fundamental
       ↓
narrow notch
       ↓
harmonics where confirmed
```

The notch bandwidth should be narrow enough to avoid removing useful vocal energy.

A detected tone should also be tested against the noise reference. If the tone exists only in the vocal recording and not in the calibration/profile, it should not automatically be classified as noise.

---

# 29. Classical Noise Suppression

The principal classical method is a conservative spectral/Wiener-style suppression system.

Conceptually:

```text
Noisy spectrum X(t,k)
        +
Noise estimate N(k)
        ↓
Signal/noise estimate
        ↓
Soft gain G(t,k)
        ↓
Smoothed gain
        ↓
Original complex spectrum × gain
        ↓
iSTFT
```

The gain should normally remain in a range such as:

```text
1.0 = untouched
0.8 = small reduction
0.5 = moderate reduction
0.2 = strong reduction
```

A hard binary gate is explicitly avoided.

---

# 30. Noise Profile and Wiener Gain

A simplified starting point is:

```text
P_signal = max(P_input - P_noise, 0)
```

then derive an SNR estimate and Wiener-style gain:

```text
xi = estimated signal-to-noise ratio
G  = xi / (1 + xi)
```

The production implementation should smooth the estimates over time and use conservative floors/limits.

The exact estimator is a tunable component, not a fixed mathematical commitment for all future versions.

---

# 31. Mask Smoothing

Raw time-frequency masks should be smoothed in both dimensions.

```text
raw mask
   ↓
frequency smoothing
   ↓
time smoothing
   ↓
final attenuation mask
```

This reduces rapid isolated changes that can sound like:

- bubbling;
- metallic ringing;
- musical noise;
- unstable background texture.

---

# 32. Vocal Activity Protection

Ordinary speech VAD should not be the sole protection mechanism because the application must handle singing.

A lightweight singing-safe activity estimate should combine:

```text
RMS / energy
+
spectral flatness
+
spectral flux
+
harmonicity
+
noise-profile similarity
```

A region with:

```text
stable harmonic structure
+
changing pitch
```

is likely wanted vocal material even if it has low energy.

A region with:

```text
low energy
+
strong similarity to known fan spectrum
+
low harmonicity
```

is more likely background noise.

The goal is not perfect singer detection. The goal is preventing the suppressor from treating obvious vocal structure as disposable background.

---

# 33. Harmonic Protection

Singing has a structured harmonic series:

```text
fundamental
  ├─ 2nd harmonic
  ├─ 3rd harmonic
  ├─ 4th harmonic
  └─ ...
```

A later preservation stage can derive a harmonicity/protection mask.

```text
STFT
 ↓
harmonicity estimate
 ↓
protected vocal regions
 ↓
constrain noise suppression
```

This is an important singing-specific feature.

It should be added after the basic DSP pipeline works because the first implementation should remain simple enough to benchmark.

---

# 34. Adaptive Noise Model

The initial calibration profile provides:

```text
N0(k)
```

The actual recording may change:

```text
fan speed changes
AC changes
room changes
```

A slow adaptive update can therefore be added:

```text
N0
 ↓
find high-confidence non-vocal noise region
 ↓
update slowly
 ↓
N1
```

The update must not occur merely because the singer becomes quiet.

A region should be considered for model updating only when multiple conditions agree:

```text
low vocal probability
AND
low harmonicity
AND
reasonable similarity to existing noise
AND
no strong transient
```

---

# 35. DSP Output and Residual Analysis

After the DSP pass:

```text
original
    ↓
DSP
    ↓
dsp_cleaned
```

Now estimate the remaining noise.

Possible result:

```text
noise level before = high
noise level after  = low
```

Then finish without AI.

Another result:

```text
noise level before = high
noise level after  = moderate/high
```

Then invoke the neural backend.

This is the central resource-saving decision in the architecture.

---

# 36. Neural Backend Architecture

The neural system must be replaceable:

```text
                    Denoiser API
                         │
             ┌───────────┴───────────┐
             │                       │
       DPDFNet2-48k             DeepFilterNet3
```

The rest of the application must not depend on model-specific tensor names or inference logic.

A conceptual Rust interface:

```rust
trait DenoiserBackend {
    fn name(&self) -> &'static str;
    fn sample_rate(&self) -> u32;
    fn latency_samples(&self) -> usize;
    fn reset(&mut self) -> Result<(), DenoiserError>;
    fn process(&mut self, input: &[f32], output: &mut Vec<f32>) -> Result<(), DenoiserError>;
    fn flush(&mut self, output: &mut Vec<f32>) -> Result<(), DenoiserError>;
}
```

The actual interface can differ, but sample rate, state and latency must be explicit.

---

# 37. Primary Neural Candidate: DPDFNet2 48 kHz

The corrected architecture uses **DPDFNet2 48 kHz** as the first neural model to benchmark.

Current DPDFNet documentation lists:

```text
dpdfnet2_48khz_hr
2.58M parameters
2.42G MACs
10.0 MB ONNX
11.6 MB TFLite
48 kHz
```

It is specifically provided as a high-resolution 48 kHz enhancement model. The current project also provides stateful streaming support and ONNX/TFLite inference.

This is a much better fit for the project than assuming the standard 16 kHz GTCRN checkpoint can be used directly on a 48 kHz singing recording.

DPDFNet is still a speech-enhancement system rather than a singing-specific model. Therefore this selection is a deployment/architecture choice, not a claim that it is proven to sound best on singing.

---

# 38. Secondary Neural Candidate: DeepFilterNet3

DeepFilterNet3 remains a major benchmark candidate.

DeepFilterNet is a low-complexity full-band 48 kHz speech-enhancement framework with native Rust components (`libDF`) and a standalone native processing path.

The architecture should benchmark:

```text
DPDFNet2-48k
vs
DeepFilterNet3
```

using the project's actual singing/voice corpus.

The final primary model is selected from:

```text
vocal preservation
noise attenuation
CPU use
RAM use
processing speed
artifact rate
```

rather than generic speech benchmark scores alone.

---

# 39. GTCRN's Role

GTCRN should not be the mandatory primary backend in Linux v1.

The standard `gtcrn_simple.onnx` model distributed by sherpa-onnx is 16 kHz and approximately 523 KB. It is extremely attractive from a resource perspective, and the GTCRN project is specifically designed for lightweight speech enhancement.

However:

```text
16 kHz
    ↓
8 kHz Nyquist
```

means a direct 48 kHz → 16 kHz → 48 kHz pipeline discards everything above 8 kHz.

That is undesirable for full-band singing.

GTCRN may be investigated later as:

- a 16 kHz low-resource mode;
- a sub-band research experiment;
- a future verified full-band checkpoint if a suitable model is obtained.

It should not be silently inserted into the full-band 48 kHz pipeline.

---

# 40. Neural Runtime Selection

## Initial integration: sherpa-onnx

For the first implementation, use **sherpa-onnx** where it provides the desired enhancement model and stateful API.

The reason is practical: it already exposes speech-enhancement models and examples, including DPDFNet and GTCRN, and provides Rust APIs.

Using sherpa-onnx avoids immediately rebuilding model-specific streaming state logic and tensor handling.

## Alternative: direct ONNX Runtime binding

`ort` can later be evaluated if direct ONNX Runtime integration gives measurable benefits in:

- binary size;
- build complexity;
- deployment control;
- inference performance.

The reason not to switch merely for theoretical cleanliness is that `ort` remains a binding to native ONNX Runtime; it does not remove the underlying native runtime dependency.

The first milestone should therefore optimize for a working, correct model pipeline rather than minimizing FFI abstractions prematurely.

---

# 41. Stateful Neural Processing

The neural backend must preserve its internal state across the logical recording stream.

Incorrect:

```text
8 sec chunk
 → reset model
 → process

8 sec chunk
 → reset model
 → process
```

Correct:

```text
recording
   ↓
frame stream
   ↓
model state maintained
   ↓
continuous enhanced stream
```

The file-processing layer may still read/write the file in chunks to limit memory usage, but those chunks must not automatically imply model-state resets.

---

# 42. Chunking Architecture

Long files must be processed without loading the entire recording into RAM.

```text
input.wav
   │
   ├── file block 1
   ├── file block 2
   ├── file block 3
   └── ...
```

Inside the processing engine:

```text
file block
   ↓
model frames
   ↓
stateful processing
   ↓
output block
```

For STFT-only DSP, overlapping windows can be handled with overlap-add.

For a stateful neural backend, the application's chunk boundaries must not reset model state.

The exact block size should be benchmarked. A reasonable starting point for file I/O is several seconds rather than tens of seconds, but model-specific streaming state determines the true processing structure.

---

# 43. Latency Must Be First-Class

This is a critical correction to the previous architecture.

Every processing backend must expose its algorithmic delay.

Example:

```text
original path:   X(t)
processed path:  Y(t - Δ)
```

Before combining them:

```text
X → delay compensation → aligned X
Y ----------------------→ Y
```

Then:

```text
Output = (1 - α) aligned_X + α Y
```

Without alignment, mixing the original and processed streams can create comb-filtering/phasiness because the signals are not describing exactly the same instant.

Latency must therefore be part of the backend contract.

---

# 44. Preservation Layer

The preservation layer exists because no denoiser can guarantee that every alteration belongs exclusively to the noise.

Let:

```text
X = aligned original
Y = denoised signal
```

Then use:

```text
Output = X + α(Y - X)
```

where:

```text
α = 0       original
α = 1       processed
```

The first implementation can use a global conservative alpha.

The eventual implementation should move toward a time/frequency-dependent preservation control:

```text
α(t,k)
```

based on:

- vocal confidence;
- noise confidence;
- model attenuation;
- harmonic protection.

---

# 45. Why Original + Processed Blending Is Not the Whole Preservation Strategy

A global blend cannot repair an already misaligned or phase-incompatible result.

Therefore the required order is:

```text
processed output
      ↓
latency alignment
      ↓
preservation calculation
      ↓
blend
```

Not:

```text
blend first
↓
try to align later
```

For later versions, an explicit time-frequency gain applied to the original complex spectrum may provide even finer control, but the initial time-domain blend should remain simpler and auditable.

---

# 46. Removed-Noise Signal

Generate:

```text
removed_noise.wav
```

using the **aligned** signals:

```text
removed_noise = aligned_original - cleaned
```

This file is valuable for both development and the final UI.

Successful example:

```text
removed noise:
fan
hiss
hum
room ambience
```

Bad example:

```text
removed noise:
main vocal
sibilance
breath
vibrato
```

The latter indicates excessive attenuation or a preservation failure.

---

# 47. A/B Playback Architecture

The playback layer should provide three sources:

```text
Original
Cleaned
Removed noise
```

The playback position must remain synchronized.

Example UI:

```text
┌─────────────────────────────────────────┐
│  ▶ Original    ▶ Cleaned    ▶ Removed  │
│                                         │
│  00:37 ─────────●────────── 01:42       │
└─────────────────────────────────────────┘
```

This makes audible comparison much easier than relying on memory.

---

# 48. Waveform Rendering

A GUI must never scan millions of audio samples during every render pass.

For a five-minute 48 kHz recording:

```text
5 × 60 × 48,000
= 14,400,000 samples
```

Instead, create a peak cache.

For example:

```text
1024 samples
   ↓
minimum + maximum
```

The cache can then provide multiple resolutions:

```text
level 0 → 1 peak pair / 64 samples
level 1 → 1 / 256
level 2 → 1 / 1024
level 3 → 1 / 4096
```

The GUI chooses the appropriate resolution according to zoom level.

This is effectively a waveform mipmap.

The cache should be produced after recording/import and updated after processing, not recomputed continuously by the renderer.

---

# 49. Level Meter

The recording meter should use the same captured stream statistics already being computed by the writer/monitor worker.

Display:

```text
RMS
Peak
Clipping
```

A reasonable update rate is around 10–20 GUI updates per second. There is no reason to send every audio sample to the UI.

---

# 50. Clipping Detection

During recording, count samples that approach or exceed full scale.

Example:

```text
Peak: -3.4 dBFS
Clipping: 0
```

or:

```text
Peak: 0.0 dBFS
Clipping: detected
```

The application can warn about input overload, but denoising cannot reliably reconstruct information that was already clipped.

---

# 51. Processing Profiles

The user-facing controls should remain simple.

Initial profiles:

```text
Light
Standard
Strong
```

Internally they control:

- DSP attenuation limits;
- neural invocation threshold;
- neural attenuation limit where supported;
- preservation strength;
- adaptive-noise update aggressiveness.

They should not map directly to arbitrary percentage values in the UI.

---

# 52. Suggested Processing Semantics

## Light

```text
noise profile
 ↓
hum/tonal cleanup
 ↓
conservative spectral suppression
 ↓
finish unless noise remains clearly problematic
```

## Standard

```text
noise profile
 ↓
hum/tonal cleanup
 ↓
conservative spectral suppression
 ↓
residual-noise test
 ↓
DPDFNet2-48k if necessary
 ↓
preservation
```

## Strong

```text
noise profile
 ↓
hum/tonal cleanup
 ↓
stronger but bounded DSP
 ↓
DPDFNet2-48k
 ↓
stronger bounded preservation control
```

"Strong" must not mean unlimited attenuation.

---

# 53. Automatic Model Selection

The application can later derive an automatic decision:

```text
Analyze
  ↓
noise severity
  ↓
stationarity
  ↓
residual noise after DSP
  ↓
choose
```

Example:

```text
Stationary fan
Noise low after DSP
     ↓
DSP only
```

Example:

```text
Traffic + fan + changing background
Noise remains high
     ↓
DPDFNet2-48k
```

This avoids loading a neural model unnecessarily.

---

# 54. Sequential Model Loading

Never load multiple heavyweight neural models simultaneously on the target class of low-RAM machines.

Correct:

```text
Analyze
  ↓
load selected model
  ↓
process
  ↓
unload model
  ↓
finish
```

Avoid:

```text
load DPDFNet
+
load DeepFilterNet
+
load GTCRN
```

at the same time.

---

# 55. Model Selection Should Be Data-Driven

No model should become permanent merely because its generic benchmark is high.

The test corpus should include:

### Speech

- male speech
- female speech
- soft speech
- loud speech
- whisper/breathy speech where relevant
- fast speech

### Singing

- low notes
- high notes
- sustained notes
- vibrato
- breathy singing
- soft singing
- loud singing
- rapid lyrics

### Noise

- fan
- AC
- hiss
- electrical hum
- traffic
- keyboard
- wind
- room ambience
- distant people
- mixed noise

### Noise strength

- very low
- low
- medium
- high
- very high

---

# 56. Objective Evaluation

Where clean source material exists:

```text
clean vocal
   +
known noise
   ↓
synthetic noisy recording
```

Then compare:

```text
expected clean
vs
processed result
```

Metrics can include:

- SI-SDR;
- noise attenuation;
- signal error;
- spectral distortion;
- peak/clipping changes;
- processing time;
- RAM use.

These metrics are diagnostic rather than the sole model-selection criterion.

---

# 57. Perceptual Evaluation

Every candidate must undergo listening tests.

Compare:

```text
A = Original
B = DSP only
C = DPDFNet2-48k
D = DSP + DPDFNet2-48k
E = DeepFilterNet3
F = DSP + DeepFilterNet3
```

For singing, explicitly inspect:

- sibilants;
- breathing;
- sustained vowels;
- vibrato;
- high notes;
- low notes;
- note transitions;
- consonants;
- quiet endings;
- vocal brightness.

The model that sounds best on actual target recordings becomes the primary backend.

---

# 58. Resource Budget

The target runtime should fit comfortably into a 4 GB RAM class system where practical.

The processing architecture should keep only:

```text
current audio block
noise profile
STFT buffers
one model
output block
small metadata
```

in active memory.

Memory usage must not grow linearly with recording duration.

For example:

```text
1 minute recording → small working memory
60 minute recording → approximately the same working memory
```

The processing time grows; the memory footprint should remain bounded.

---

# 59. CPU Threading

The reference Ryzen 5 5500U provides 12 logical CPUs, but the baseline target is weaker.

The first implementation should use conservative worker counts.

Suggested starting point:

```text
GUI             1 thread
capture         audio callback
writer          1 worker
analysis        1–2 workers
DSP             1–4 workers as needed
neural model    controlled worker count
```

The exact model threading should be benchmarked.

The application should provide a preference such as:

```text
Processing CPU usage

Low
Balanced
Fast
```

where appropriate.

---

# 60. Thermal and Battery Awareness

The architecture is intended to work on devices below the reference laptop.

For mobile later, processing should be explicitly offline and controllable.

The engine should expose:

```text
estimated CPU cost
model selected
processing progress
```

The Android version can later select a smaller inference backend or a more conservative profile when thermal/battery conditions demand it.

---

# 61. Linux v1 Process Model

The application should use:

```text
UI thread
     │
     ├── command channel ─────────────┐
     │                                ▼
     │                         Processing worker
     │                                │
     │                                ├── analyzer
     │                                ├── DSP
     │                                ├── model
     │                                └── output
     │
     └── state updates ←──────────────┘
```

The UI communicates through commands/events rather than directly invoking long-running processing.

Possible commands:

```text
StartCalibration
StartRecording
StopRecording
StartProcessing
CancelProcessing
PlayOriginal
PlayCleaned
PlayRemovedNoise
Export
```

Possible events:

```text
CalibrationStarted
CalibrationComplete
RecordingStarted
PeakUpdated
RecordingStopped
AnalysisProgress
ProcessingProgress
ProcessingComplete
ProcessingError
```

---

# 62. Project Structure

Recommended Rust workspace:

```text
voice-cleaner/
│
├── Cargo.toml
├── Cargo.lock
│
├── crates/
│   ├── app/
│   │   └── src/
│   │       ├── main.rs
│   │       ├── ui/
│   │       ├── state/
│   │       └── commands/
│   │
│   ├── audio-core/
│   │   └── src/
│   │       ├── format.rs
│   │       ├── pcm.rs
│   │       ├── chunk.rs
│   │       ├── resample.rs
│   │       └── errors.rs
│   │
│   ├── recorder/
│   │   └── src/
│   │       ├── recorder.rs
│   │       ├── device.rs
│   │       ├── callback.rs
│   │       ├── calibration.rs
│   │       └── meter.rs
│   │
│   ├── dsp/
│   │   └── src/
│   │       ├── stft.rs
│   │       ├── noise_profile.rs
│   │       ├── spectral_gate.rs
│   │       ├── wiener.rs
│   │       ├── notch.rs
│   │       ├── stationarity.rs
│   │       ├── activity.rs
│   │       └── harmonicity.rs
│   │
│   ├── denoiser/
│   │   └── src/
│   │       ├── backend.rs
│   │       ├── dpdfnet.rs
│   │       └── deepfilter.rs
│   │
│   ├── pipeline/
│   │   └── src/
│   │       ├── analyzer.rs
│   │       ├── processor.rs
│   │       ├── preservation.rs
│   │       ├── latency.rs
│   │       ├── residual.rs
│   │       └── job.rs
│   │
│   ├── playback/
│   │   └── src/
│   │       ├── player.rs
│   │       └── sync.rs
│   │
│   └── testkit/
│       └── src/
│           ├── fixtures.rs
│           ├── metrics.rs
│           └── audio_compare.rs
│
├── models/
│   └── dpdfnet2_48khz_hr.onnx
│
├── recordings/
├── test-audio/
│   ├── clean/
│   ├── noisy/
│   └── generated/
│
└── docs/
    └── architecture.md
```

The workspace does not have to contain this exact number of crates from day one. The critical boundaries are:

```text
capture
analysis
DSP
neural inference
pipeline
UI
```

---

# 63. Dependency Plan

## Initial application dependencies

```toml
[dependencies]
eframe = "current-stable"
ecpal = "current-stable"
ringbuf = "current-stable"
hound = "current-stable"
rubato = "current-stable"
realfft = "current-stable"
rustfft = "current-stable"
```

Exact versions should be pinned by `Cargo.lock` once the project is created.

## Neural inference

Integrate `sherpa-onnx` after the recorder and DSP pipeline are functional.

Model assets should be treated as separate release artifacts so the model can be benchmarked or replaced without restructuring the application.

---

# 64. Ubuntu/Linux Development Dependencies

For Ubuntu 24.04, begin with the native build/audio requirements documented by the selected libraries.

A starting setup is:

```bash
sudo apt update

sudo apt install \
    build-essential \
    pkg-config \
    libssl-dev \
    libasound2-dev \
    libpipewire-0.3-dev \
    libxcb-render0-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libxkbcommon-dev
```

The exact set can be reduced after the first successful build if optional backends are not enabled.

The application should prefer the PipeWire path where appropriate on the target Ubuntu/KDE system, while retaining ALSA compatibility.

---

# 65. Initial Project Setup

Create the project:

```bash
mkdir -p ~/Projects
cd ~/Projects

cargo new voice-cleaner --bin
cd voice-cleaner
```

Initialize source control immediately:

```bash
git init
git add .
git commit -m "Initial project"
```

Create the basic module structure before implementing any AI model.

---

# 66. Phase 1 — Recorder MVP

Implement only:

```text
microphone enumeration
↓
selected input
↓
2-second calibration
↓
record
↓
stop
↓
original.wav
noise_reference.wav
```

Also implement:

```text
live RMS
peak
clipping counter
```

Do not add denoising yet.

### Acceptance criteria

- no dropped samples during normal recording;
- WAV file is valid;
- duration is correct;
- calibration file is valid;
- level meter does not interfere with recording;
- selected microphone is actually used;
- disconnecting a device produces an explicit error.

---

# 67. Phase 2 — Audio Foundation

Add:

```text
WAV loading
Float32 conversion
channel conversion
48 kHz resampling
chunk reader
chunk writer
peak cache
```

Acceptance criteria:

- 44.1 kHz input converts correctly;
- 48 kHz input remains unchanged;
- mono/stereo conversion is deterministic;
- test tones survive conversion without unexpected artifacts.

---

# 68. Phase 3 — Noise Analyzer

Implement:

```text
STFT
noise PSD
noise level
stationarity
persistent tonal peaks
basic vocal/activity confidence
```

Display the analyzer internally first; the end-user does not need all these diagnostics in the main UI.

Acceptance criteria:

```text
fan recording → high stationarity
hum recording → tonal peaks
changing traffic → low stationarity
clean recording → low estimated noise
```

---

# 69. Phase 4 — DSP Noise Removal

Implement:

```text
DC blocker
↓
tonal notch
↓
noise-profile spectral/Wiener suppression
↓
mask smoothing
```

Create:

```text
dsp_cleaned.wav
removed_noise.wav
```

Acceptance criteria:

- constant fan is reduced;
- stable hum is reduced;
- voice remains natural;
- singing remains natural;
- no severe musical noise at standard settings.

---

# 70. Phase 5 — Residual Noise Decision

Calculate a post-DSP noise score.

Example:

```text
DSP residual = low
    ↓
finish
```

or:

```text
DSP residual = high
    ↓
neural backend
```

This establishes the resource-saving architecture before AI is added.

---

# 71. Phase 6 — DPDFNet2-48k

Integrate:

```text
sherpa-onnx / native model runtime
+
dpdfnet2_48khz_hr.onnx
```

Do not convert the recording to 16 kHz.

The model operates in the 48 kHz branch.

Implement:

```text
state initialization
stateful streaming
latency reporting
flush
error handling
```

Acceptance criteria:

- model output is 48 kHz;
- long files process without OOM;
- state is continuous across file blocks;
- latency is measurable/documented;
- CPU/RAM usage is recorded.

---

# 72. Phase 7 — Preservation Layer

Add:

```text
latency alignment
+
original/processed blending
+
removed-noise calculation
```

First use a global alpha for testing.

Then introduce:

```text
vocal-protection confidence
harmonic protection
frequency/time-dependent alpha
```

Acceptance criteria:

- no comb filtering from dry/wet misalignment;
- no obvious phasing;
- vocal remains close to original when denoising is light;
- removed-noise track does not contain obvious wanted vocals at normal settings.

---

# 73. Phase 8 — DeepFilterNet3 Benchmark

Integrate DeepFilterNet3 as a second backend.

Run the same corpus through:

```text
DSP + DPDFNet2-48k
DSP + DeepFilterNet3
```

Measure:

```text
CPU
RAM
processing time
noise attenuation
vocal preservation
artifact rate
```

The better result becomes the default quality backend.

The other backend can remain as an optional advanced mode only if its maintenance cost is justified.

---

# 74. Phase 9 — Automatic Processing

Once manual profiles are reliable, enable:

```text
Analyze
 ↓
choose DSP-only or neural
 ↓
process
```

Automatic mode should always have a conservative bias.

If the analyzer is uncertain whether a region is voice or noise:

```text
preserve more
```

rather than:

```text
remove more
```

---

# 75. Phase 10 — GUI Refinement

Main screen:

```text
┌─────────────────────────────────────────────┐
│ Voice Cleaner                               │
│                                             │
│ Input: Built-in Microphone          [▼]    │
│                                             │
│                ● RECORD                     │
│                                             │
│ Level      ▂▃▅▆▅▃▂                          │
│                                             │
│ 00:42                                        │
│                                             │
│ ───────────────────────────────────────     │
│ waveform                                    │
│                                             │
│ Noise Reduction:  Standard                  │
│                                             │
│                 [ Process ]                 │
└─────────────────────────────────────────────┘
```

Result screen:

```text
Original     ▶
Cleaned      ▶
Removed      ▶

Noise Reduction: Standard

[ Export WAV ]
```

The application should not expose model names or FFT settings to ordinary users.

---

# 76. Processing State Machine

The processing system should have explicit states:

```text
Idle
 ↓
Calibrating
 ↓
Ready
 ↓
Recording
 ↓
FinalizingRecording
 ↓
Analyzing
 ↓
DspProcessing
 ↓
ResidualAnalysis
 ↓
NeuralProcessing (optional)
 ↓
LatencyAlignment
 ↓
Preservation
 ↓
OutputWriting
 ↓
Complete
```

Any stage can transition to:

```text
Error
```

or, where safe:

```text
Cancelled
```

---

# 77. Cancellation

Because processing is offline, cancellation is important.

The worker should periodically check:

```text
cancel_flag
```

For long neural inference sections, cancellation should occur at model-safe frame/block boundaries rather than forcing unsafe interruption.

Partial outputs should be marked as incomplete and should not silently replace the final output.

---

# 78. Error Handling

Required errors include:

```text
no microphone
unsupported format
microphone disconnected
ring buffer overflow
WAV write failure
WAV read failure
invalid calibration
resampling failure
model missing
model load failure
model inference failure
insufficient memory
output write failure
```

Errors should include a human-readable explanation plus an internal error category for diagnostics.

---

# 79. Logging

Logging should be available in a development mode.

Example:

```text
[INFO] Input device: Built-in Microphone
[INFO] Capture format: 48000 Hz / f32 / mono
[INFO] Calibration: valid
[INFO] Noise stationarity: 0.91
[INFO] Tonal peaks: 50.0 Hz, 100.0 Hz
[INFO] DSP residual: moderate
[INFO] Neural backend: DPDFNet2-48k
[INFO] Model latency: XXXX samples
[INFO] Processing RTF: X.XX
[INFO] Peak RAM: XXX MB
```

No unnecessary user data should be uploaded or logged remotely.

---

# 80. Privacy and Offline Requirements

Normal operation must not require:

```text
internet
cloud API
user account
telemetry server
remote inference
```

Audio remains local.

The application can optionally provide an explicit diagnostic export containing metadata, but it must never silently upload recordings.

---

# 81. Packaging for Linux

After the core is stable, package as a normal Linux desktop application.

Possible first distribution methods:

```text
AppImage
.deb
```

AppImage is useful for a self-contained early release.

The exact packaging method should be selected after the native model runtime is working because the model/runtime library packaging is more important than the GUI packaging at this stage.

---

# 82. Model Asset Packaging

The Linux package should contain the selected model locally.

Example:

```text
/usr/lib/voice-cleaner/
    voice-cleaner
    models/
        dpdfnet2_48khz_hr.onnx
```

No model download should be necessary for the standard offline installation.

The exact model file and its license must be verified before redistribution.

---

# 83. Cross-Platform Boundary

Although Linux is v1, platform-specific code should remain limited to capture/playback and UI glue.

Conceptually:

```text
                  Shared Processing Core
                         Rust
                           │
       ┌───────────────────┼───────────────────┐
       │                   │                   │
     Linux               Windows            Android
       │                   │                   │
   CPAL backend        CPAL backend       CPAL backend
       │                   │                   │
      GUI                 GUI                 GUI
```

The future processing core should not contain Linux-specific logic.

---

# 84. Future Android Architecture

The Android target can reuse:

```text
analysis
DSP
resampling
noise profile
pipeline
preservation
model API
```

Only the platform layer needs substantial adaptation:

```text
Android UI
Android audio capture
Android application lifecycle
Android native model packaging
```

DPDFNet's current model set includes both ONNX and TFLite artifacts, which gives the later Android implementation an additional deployment option.

The Android version should be validated separately for:

- ARM64 performance;
- thermal throttling;
- battery consumption;
- 4 GB RAM devices;
- microphone route changes;
- headset/Bluetooth behavior.

---

# 85. Future Windows Architecture

Windows should retain the same processing engine.

CPAL can use WASAPI for capture/playback.

The UI remains conceptually identical.

The primary additional testing requirement is the diversity of Windows microphone/audio-driver paths and any processing already applied by the system or hardware.

---

# 86. Why the Architecture Does Not Use Music Separation

Music/source separation is intentionally absent.

The assumed input is:

```text
vocal
+
background noise
```

not:

```text
vocal
+
music stem
+
noise
```

Therefore applying Demucs or another source separator would introduce unnecessary computation and another possible source of vocal artifacts.

A small amount of music in the background is simply treated as unwanted complex background sound. The current denoiser may reduce some of it, but perfect removal is not guaranteed.

---

# 87. Why Not Use a Huge End-to-End Model

A large model could potentially remove more difficult environmental noise, but it would conflict with the product constraints:

```text
low RAM
CPU-only
cross-platform
offline
fast enough on weak devices
preserve vocals
```

A smaller architecture with deterministic preprocessing and one targeted neural backend gives better control over these constraints.

---

# 88. Why Not Use AI During Recording

Real-time AI is unnecessary because the product is explicitly offline.

During recording:

```text
capture
 ↓
save
```

During processing:

```text
analyze
 ↓
clean
```

This reduces:

- recorder complexity;
- real-time deadline risk;
- CPU pressure;
- battery consumption;
- debugging difficulty.

---

# 89. Reference Processing Algorithm

The complete initial algorithm is:

```text
INPUT
 │
 ├── original recording
 │
 └── optional noise reference
 │
 ▼
CANONICALIZE
 │
 ├── Float32
 ├── 48 kHz
 └── mono processing stream
 │
 ▼
ANALYZE
 │
 ├── noise PSD
 ├── noise level
 ├── stationarity
 ├── tonal peaks
 ├── vocal/activity confidence
 └── harmonicity
 │
 ▼
DSP PASS
 │
 ├── DC correction
 ├── confirmed tonal notches
 ├── conservative spectral/Wiener suppression
 └── mask smoothing
 │
 ▼
RESIDUAL ANALYSIS
 │
 ├───────────────┐
 │               │
LOW             HIGH
 │               │
 ▼               ▼
DONE       SELECT NEURAL BACKEND
                    │
             ┌──────┴───────┐
             │              │
       DPDFNet2-48k    DeepFilterNet3
             │              │
             └──────┬───────┘
                    │
                    ▼
             stateful inference
                    │
                    ▼
              latency alignment
                    │
                    ▼
             vocal preservation
                    │
              ┌─────┴─────┐
              ▼           ▼
          cleaned      removed noise
              │           │
              ▼           ▼
        cleaned.wav  removed_noise.wav
```

---

# 90. Final Component Table

| Layer | Component | Role | Status |
|---|---|---|---|
| Language | Rust | Main application and DSP core | Selected |
| GUI | egui/eframe | Native cross-platform GUI | Selected |
| Audio I/O | CPAL | Microphone/speaker access | Selected |
| Capture buffer | ringbuf | Lock-free callback-to-worker transport | Selected |
| WAV | hound | Initial lossless file format | Selected |
| Resampling | rubato | Offline sample-rate conversion | Selected |
| FFT | rustfft + realfft | STFT/FFT | Selected |
| Noise profile | Custom Rust | Explicit noise representation | Selected |
| Tonal filtering | Custom biquad/notch | Hum/whine cleanup | Selected |
| Spectral suppression | Custom Rust | Predictable-noise reduction | Selected |
| Reference implementation | noisereduce/noisereduce-cpp | Algorithm validation only | Optional/reference |
| Neural runtime | sherpa-onnx | Initial model integration | Selected for v1 |
| Primary neural candidate | DPDFNet2 48 kHz | Full-band low-resource enhancement | First benchmark |
| Quality candidate | DeepFilterNet3 | Full-band 48 kHz alternative | Benchmark |
| Lightweight experimental model | GTCRN | 16 kHz low-resource research path | Not primary |
| Playback | CPAL | A/B listening | Selected |
| Waveform | custom peak cache | Efficient rendering | Selected |

---

# 91. What Is Not Selected

## PyTorch at runtime

Not selected because a native inference deployment is more appropriate for the low-resource product.

PyTorch may still be used in model research/benchmark scripts where required by an upstream project.

## Original Python Demucs

Not relevant to the current product scope because music/source separation has been removed.

## RNNoise as the primary full-band model

Too limited for the target full-band singing pipeline and unnecessary when a suitable 48 kHz backend can be used.

## GTCRN 16 kHz as the 48 kHz default

Rejected because the standard model operates at 16 kHz and would discard information above 8 kHz if the full signal were downsampled to it.

## Large RoFormer/separation models

Unnecessary for the current problem and inconsistent with the low-resource design target.

## GPU dependence

Rejected for the baseline architecture.

---

# 92. Critical Invariants

The implementation should enforce these rules:

### Invariant 1

`original.wav` is never modified.

### Invariant 2

The audio callback never performs heavy processing or blocking I/O.

### Invariant 3

Processing memory does not grow with recording duration.

### Invariant 4

A stateful neural backend is not reset at arbitrary file-chunk boundaries.

### Invariant 5

Dry/wet blending is impossible until latency is accounted for.

### Invariant 6

The standard 16 kHz GTCRN model is not used as a direct 48 kHz full-band denoiser.

### Invariant 7

AI is optional; the DSP path can finish independently.

### Invariant 8

When uncertain whether content is voice or noise, the system should favor preservation.

### Invariant 9

No network connection is required for recording or processing.

### Invariant 10

Model choice is validated using actual speech and singing recordings, not generic benchmark scores alone.

---

# 93. Linux v1 Definition of Done

Linux v1 is complete when all of the following work:

```text
[x] microphone enumeration
[x] microphone selection
[x] calibration
[x] recording
[x] live RMS/peak meter
[x] clipping detection
[x] lossless original WAV
[x] noise-reference WAV
[x] audio canonicalization
[x] 48 kHz processing path
[x] noise profile
[x] stationarity analysis
[x] tonal-noise analysis
[x] conservative DSP suppression
[x] residual-noise decision
[ ] DPDFNet2-48k backend
[ ] latency measurement/alignment
[ ] preservation blend
[x] cleaned WAV
[x] removed-noise WAV
[x] original/cleaned/noise-only playback
[x] waveform peak cache
[ ] processing cancellation
[ ] error handling
[ ] long-recording memory test
[ ] CPU usage test
[ ] singing preservation test
[ ] clean-input regression test
[ ] no-internet processing test
```

---

# 94. First Coding Milestone

The first implementation should stop at the recorder.

The first executable goal is:

```text
Launch application
      ↓
Select microphone
      ↓
Calibrate for 2 seconds
      ↓
Record voice/singing
      ↓
Stop
      ↓
Save:
    original.wav
    noise_reference.wav
      ↓
Display waveform + peak information
      ↓
Play recording
```

Only after this is stable should the DSP engine be implemented.

The neural model should be introduced after the DSP path can independently demonstrate correct noise reduction.

---

# 95. Current Architecture Decision Summary

The corrected architecture is therefore:

```text
                    RECORD RAW AUDIO
                           │
                    1.5–2s CALIBRATION
                           │
                           ▼
                    NOISE CHARACTERIZE
                           │
                           ▼
                  CONSERVATIVE DSP FIRST
                           │
                     RESIDUAL CHECK
                           │
                   ┌───────┴────────┐
                   │                │
                 enough           not enough
                   │                │
                   ▼                ▼
                 OUTPUT       48 kHz NEURAL
                                  │
                       ┌──────────┴──────────┐
                       │                     │
                 DPDFNet2-48k          DeepFilterNet3
                       │                     │
                       └──────────┬──────────┘
                                  │
                           ONE MODEL ONLY
                                  │
                                  ▼
                          LATENCY ALIGNMENT
                                  │
                                  ▼
                         VOCAL PRESERVATION
                                  │
                      ┌───────────┴───────────┐
                      ▼                       ▼
                  cleaned.wav          removed_noise.wav
```

The most important corrections from the previous architecture are:

1. **GTCRN 16 kHz is no longer the default full-band model.**
2. **DPDFNet2 48 kHz is the first neural model to benchmark.**
3. **DeepFilterNet3 is the main alternative quality backend.**
4. **Neural model state must remain continuous across file chunks.**
5. **Latency is a mandatory part of the processing backend API.**
6. **Dry/wet blending occurs only after delay alignment.**
7. **DSP remains the first line of defense, but is explicitly treated as capable of artifacts.**
8. **Waveform data is cached rather than rescanned every GUI frame.**
9. **The recorder is fully specified as an independent capture subsystem using CPAL + a lock-free ring buffer + a separate writer.**
10. **The project remains CPU-first, offline, local, low-memory, and structured for later Windows/Android ports.**

---

# 96. External Component References

These references should be checked again at implementation time because library APIs, model exports, and packaging details can change.

- CPAL: https://github.com/RustAudio/cpal
- eframe: https://docs.rs/eframe/latest/
- ringbuf: https://docs.rs/ringbuf/latest/ringbuf/
- Hound: https://docs.rs/hound/latest/hound/
- Rubato: https://docs.rs/rubato/latest/rubato/
- RustFFT: https://docs.rs/rustfft/latest/rustfft/
- realfft: https://docs.rs/realfft/latest/realfft/
- DPDFNet: https://github.com/ceva-ip/DPDFNet
- DPDFNet sherpa-onnx documentation: https://github.com/k2-fsa/sherpa/blob/master/docs/source/onnx/speech-enhancement/dpdfnet.rst
- sherpa-onnx: https://github.com/k2-fsa/sherpa-onnx
- GTCRN reference: https://github.com/Xiaobin-Rong/gtcrn
- GTCRN sherpa-onnx model documentation: https://csukuangfj.github.io/sherpa/onnx/speech-enhancement/models.html
- DeepFilterNet: https://github.com/Rikorose/DeepFilterNet
- Spectral noise reduction reference: https://github.com/timsainb/noisereduce

---

# 97. Final Engineering Position

The product is not an AI audio enhancer with a recorder attached.

It is an **offline audio-cleaning engine with a recorder frontend and replaceable enhancement backends**.

The architecture intentionally follows:

```text
capture accurately
      ↓
understand the noise
      ↓
remove what can be identified confidently
      ↓
apply one efficient neural model only when useful
      ↓
align and preserve the original vocal
      ↓
verify the removed component
      ↓
export
```

That structure is the foundation for a Linux v1 that can later be ported to Windows and Android without redesigning the fundamental audio-processing pipeline.
