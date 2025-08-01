## Cargo Commands

- NEVER USE `2>&1` in command line for cargo
- build with `cargo build -p mevbase --profile=release`

## PRIME DIRECTIVES
- No mocks. No stubs. No simplifications. Create code that works for productions.
- Use `cargo build` to check for compilation errors. Do it OFTEN. Don't add functionality until the code compiles.
- Source code for reth, alloy, and rollup-boost is in ~/.cargo/git/checkouts/.  Use grep to find the source code for a function or struct.
- Document changes to architecture, design, and interfaces into CLAUDE.md
- Refer to this document every time you /compact
- Compile OFTEN.