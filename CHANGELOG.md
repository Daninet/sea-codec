## 0.8.0 (2026-07-19)

- Add configurable VBR encoder effort levels, from fast scalar encoding through ultra beam-search refinement, while retaining the existing VBR chunk format.
- Add the `--vbr-effort <fast|low|mid|high|ultra>` option to `seaconv`.
- Expose VBR effort through the C API and WebAssembly demo.
- Add VBR bitrate and effort controls to the web demo, plus responsive single-column layout on mobile.
- Display the Cargo package version in the web demo, generated during the web build.

## 0.7.0 (2026-01-24)

- Add --resample option to seaconv CLI tool and web demo
