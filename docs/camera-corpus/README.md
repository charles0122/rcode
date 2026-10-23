# Camera regression corpus

This directory defines the small, reviewable corpus format for milestone 2.
Do not commit private or identifying camera frames. `manifest.example.json` is
schema documentation only: its image is intentionally absent and it is not a
passing dataset. Generated renderer fixtures used by tests are not camera
evidence.

## Layout and manifest

Store samples below this directory (or a local, untracked directory with the
same layout) and keep paths relative to the manifest. Each sample has a unique
`id`, an image path, and an `expected` value. Positive samples set `decode` to
true and provide the exact payload as standard Base64 plus `spokes` and `ecc`.
Negative samples set `decode` to false and must omit those three fields. Unknown
fields, unsupported versions/profiles, duplicate IDs, absolute paths and parent
directory traversal are rejected. Supported images are those accepted by the
image decoder (PNG/JPEG).

Run `cargo run --release -- camera-bench docs/camera-corpus/manifest.json`.
The command decodes every entry, prints positive success rate, negative
rejection count, median and nearest-rank P95 decode latency, and exits nonzero
for an expectation mismatch or invalid/unreadable input. Latency includes the
image decode call and excludes file reading. The command reports dataset
results; it does not assert device or 720p performance thresholds on behalf of
the operator.

## Five-minute queue and memory check

On each target phone, use the SDK integration's production frame callback and
latest-frame-only policy. Count frames received, decode starts/completions,
dropped frames, and maximum simultaneous/in-flight frames. Sample process RSS
or platform memory metrics every 10 seconds. Start from a settled preview,
scan continuously for five minutes with a supported RCode present, then keep
the camera preview running for one more minute without a code. Save device/OS,
build, camera format/resolution, counters and memory samples with the run.

Pass only if the in-flight analysis queue never exceeds one, no backlog grows
over time, and memory has no sustained upward trend after warm-up (compare the
last minute's range/median with the first settled minute and investigate any
monotonic growth). Record raw samples and explain any platform memory noise;
do not infer this result from a desktop benchmark.

## Real-device capture and initial gates

For every capture session record device model, OS, camera, capture resolution,
orientation, image format, distance or code-to-short-edge ratio, carrier
(printed paper, matte paper, or another phone display), horizontal/vertical
angle, lighting/glare, motion condition, and expected payload/profile. Capture
the complete supported matrix from `mobile-sdk-plan.md`, including failures
and consecutive scan attempts; do not select only successful frames. Include
negative QR, concentric-circle, random-texture and no-code scenes. Remove
personal data and obtain permission before committing frames; otherwise keep
the images in a separately controlled artifact and retain the manifest and
checksums with the run.

Initial milestone gates remain those in `mobile-sdk-plan.md`: at least 95%
success on supported positive samples, zero negative false positives, 720p
single-frame P95 at or below 100 ms on the baseline device, and the five-minute
queue/memory check above. A desktop run validates corpus plumbing only and
cannot satisfy any real-device gate.
