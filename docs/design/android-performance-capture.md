# Android performance capture

The Android ARM64 test build on `android/poco-m8-optimization` is a diagnostic build.
It enables the engine's existing span profiler and bounded frame telemetry before Bevy
starts. This is deliberately separate from desktop defaults.

## What is collected

- **Frame timings:** a bounded 60-second JSONL capture, with startup, loading and gameplay
  phases separated. It includes frame-time percentiles and slow-frame counts.
- **CPU spans:** five-second windows of Bevy system, schedule, render-system and render-graph
  spans. Frames over 33 ms also get a per-hitch span breakdown.
- **Scene/render workload:** mesh and material counts, visibility population, changed
  transforms, changed assets and their writers, shadow casters, render batches and pipeline
  compilation counts.
- **Physics and AI:** broad-phase refresh counts, AI/path counters, physics solver stage
  timings and workload samples.
- **GPU:** render-pass timestamp measurements when the Vulkan adapter exposes the required
  timestamp-query features. A missing GPU timestamp is reported as unavailable, never
  interpreted as zero GPU cost.
- **Device utilization:** the FPS overlay's Android sampler reads machine CPU and process
  CPU time from `/proc`, process RSS from `/proc/self/status`, and Qualcomm KGSL GPU busy
  time when the kernel exposes a readable `gpubusy` or percentage node. Some Android
  vendor kernels restrict these counters; unavailable values remain explicitly unavailable.

The capture is opt-in at the application level only in the Android test binary: the Android
startup boundary supplies defaults for `NVF_PLAY_TEST_PROFILE`,
`NVF_PLAY_TEST_PROFILE_COUNTS`, `NVF_PLAY_TEST_SAMPLE`,
`NVF_PLAY_TEST_FRAME_TELEMETRY` and `NVF_PLAY_TEST_FRAME_TELEMETRY_GPU`. Explicit
process values take precedence. The profiling feature is compiled only into this Android
test build by `tools/android/build.py`; ordinary desktop builds do not gain it.

## How to collect a useful comparison

1. Install the diagnostic APK as an update; do not uninstall the existing app.
2. Launch the same save and stand in the same location used for the previous FPS test.
3. Wait at least 60 seconds so the bounded frame capture completes. Avoid opening menus or
   changing graphics settings during the capture.
4. Exit the game normally and return to the launcher.
5. On the Home page, choose **Export performance diagnostics** and save the ZIP.
6. Attach that ZIP to the conversation.

The ZIP contains the latest `frame-telemetry-*.jsonl` capture and the latest
`nvforever-*.log` engine log. Nothing is uploaded automatically; export is a user action.
The log may contain hardware/driver details, scene labels and engine diagnostics. Review it
before sharing if those details are sensitive.

## Interpretation

The first comparison is the frame-time distribution and CPU span breakdown. If process CPU
time is close to elapsed frame time multiplied by the number of active cores, the process is
CPU-busy; if CPU time is low while frames are long and GPU timestamps are high, the GPU is
the stronger suspect. Main-thread, render-thread and wait spans distinguish game update,
render submission and back-pressure. Physics, animation, transforms, asset changes and render
batches then narrow the next code-level optimization.

GPU timestamps and KGSL utilization are separate signals: a device may expose one and not
the other. Neither missing signal is treated as a zero.
