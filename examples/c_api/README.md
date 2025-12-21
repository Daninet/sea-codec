# C API Example for sea-codec

This directory contains an example of how to use the C API of `sea-codec` from a C application.

## Prerequisites

- Rust (cargo)
- CMake
- A C compiler (GCC, Clang, MSVC)

## Structure

- `main.c`: A simple C program that generates a sine wave, encodes it using `sea-codec`, and decodes it back.
- `CMakeLists.txt`: A CMake build file demonstrating how to link against the `sea-codec` library.

## Building and Running

1. **Build the Rust library:**

   From the root of the repository:

   ```bash
   cargo build --release
   ```

   This will generate the static and dynamic libraries in `target/release`.

2. **Build the C example:**

   ```bash
   cd examples/c_api
   mkdir build
   cd build
   cmake -DCMAKE_BUILD_TYPE=Release ..
   cmake --build .
   ```

3. **Run the example:**

   On Linux/macOS:
   ```bash
   ./c_example
   ```

   On Windows:
   ```bash
   .\Release\c_example.exe
   ```
   (Note: If linking dynamically, ensure `sea_codec.dll` is in the same directory or in PATH).

## Header File

The C header file is located at `include/sea_codec.h`. You should include this in your C/C++ projects.

## Cross-compilation

To build a static library for a different platform (e.g., ARM Cortex-M3), use `rustup` to add the target and `cargo` to build:

```sh
rustup target add thumbv7m-none-eabi
cargo build --release --target thumbv7m-none-eabi --features c-api
```
