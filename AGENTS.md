# Nix workflow

1. Run `nix flake check` before starting work and again after making changes.
2. Nix exclude untracked files. Stage only new files required by the build/check before Nix verification; do not stage unrelated changes.
3. Use Nix devshells to obtain toolchain.
4. Write Nix files as flake-parts modules.

# Documentation

- Keep documentation concise and useful; prefer one-line rustdoc where sufficient.
- Put API behavior and examples in rustdoc, not duplicated README explanations.

# References

- General Rust guidelines: https://rust-lang.github.io/api-guidelines/about.html
- Designing error types in Rust: https://mmapped.blog/posts/12-rust-error-handling
- treefmt: https://github.com/numtide/treefmt-nix
- flake-parts: https://flake.parts/options/flake-parts.html
