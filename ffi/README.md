# rcode-ffi

Stable C ABI (ABI version 1) over the Rust `rcode` decoder. `rcode_decode_luma8`
accepts 8-bit luma, dimensions, and row stride. On success it returns an opaque
owned result; payload bytes and other borrowed data remain valid until
`rcode_result_free`. Freeing `NULL` is safe. Getters return `RCODE_INVALID_ARGUMENT`
for null pointers or out-of-range landmark indices. ECC ids are L=0, M=1, Q=2,
H=3. Status messages are static strings; there is no mutable global error state.

Build host artifacts with `cargo build -p rcode-ffi --release`. To build the
installed Android and Apple mobile slices, run `scripts/build-mobile-core.sh`
from the repository root. It places shared Android libraries, Apple static
libraries, and `include/rcode.h` under `dist/core/`; if `xcodebuild` is available,
it also combines the Apple slices into an XCFramework.

Include `include/rcode.h` from C, Objective-C, Swift bridging headers, or JNI
native code. The caller owns camera permissions, capture lifecycle, and pixel
buffer validity; this ABI only decodes the supplied frame.
