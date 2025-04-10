# ferrisfollow

A program to attempt to construct a visual of Rust function calls starting from `main`.

From the directory with your `Cargo.toml` file, it will:

- Follow the call chain through functions and methods
- Handle cross-module calls
- Track struct method calls
- Create a visually appealing call graph with colored sequence indicators
