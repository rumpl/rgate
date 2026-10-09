#!/bin/zsh
set -euo pipefail
root="${0:A:h:h}"
profile="${1:-web}"
[[ "$profile" == web || "$profile" == dev ]] || { print -u2 "Usage: $0 [web|dev]"; exit 1; }
# Cargo installs tools here even when the shell PATH only includes Homebrew.
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
command -v wasm-bindgen >/dev/null || { print -u2 "wasm-bindgen was not found on PATH or in ${CARGO_HOME:-$HOME/.cargo}/bin. Install it with: cargo install wasm-bindgen-cli --version 0.2.129 --locked"; exit 1; }

compiler="${RUSTC:-$(command -v rustc)}"
if [[ ! -d "$("$compiler" --print target-libdir --target wasm32-unknown-unknown)" ]]; then
  # A Homebrew compiler cannot see targets installed into a rustup toolchain.
  # Use an already-installed compatible rustup compiler; never install implicitly.
  if [[ -z "${RUSTC:-}" ]]; then
    for candidate in ${RUSTUP_HOME:-$HOME/.rustup}/toolchains/*/bin/rustc(N); do
      minor=$("$candidate" --version | cut -d. -f2)
      if [[ "$minor" -ge 98 && -d "$("$candidate" --print target-libdir --target wasm32-unknown-unknown)" ]]; then
        compiler="$candidate"
        export RUSTC="$compiler"
        export RUSTDOC="${compiler:h}/rustdoc"
        print "Using WASM toolchain: $("$compiler" --version) ($compiler)"
        break
      fi
    done
  fi
  [[ -d "$("$compiler" --print target-libdir --target wasm32-unknown-unknown)" ]] || { print -u2 "The selected compiler ($compiler) has no WASM target. Install Rust 1.98+ through rustup and run: rustup target add wasm32-unknown-unknown"; exit 1; }
fi
if [[ "$(uname -s)" == Darwin ]]; then
  # rust-lld in some rustup macOS distributions needs the toolchain LLVM library.
  export DYLD_LIBRARY_PATH="$("$compiler" --print sysroot)/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
fi
expected=$(python3 - "$root/Cargo.lock" <<'PYVERSION'
import sys, tomllib
with open(sys.argv[1], 'rb') as source:
    packages = tomllib.load(source)['package']
print(next(package['version'] for package in packages if package['name'] == 'wasm-bindgen'))
PYVERSION
)
[[ "$(wasm-bindgen --version)" == "wasm-bindgen $expected" ]] || { print -u2 "wasm-bindgen CLI must match Cargo.lock: cargo install wasm-bindgen-cli --version $expected --locked"; exit 1; }
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"
cargo build --locked --manifest-path "$root/Cargo.toml" -p rgate-web --target wasm32-unknown-unknown --profile "$profile"
output="$profile"
[[ "$profile" == dev ]] && output=debug
wasm="$CARGO_TARGET_DIR/wasm32-unknown-unknown/$output/rgate_web.wasm"
dist="$root/target/web"
rm -f "$dist/licenses/TKGATE-GPL-2.0"
mkdir -p "$dist/pkg" "$dist/licenses"
wasm-bindgen "$wasm" --target web --out-dir "$dist/pkg" --out-name rgate_web
cp "$root/web/index.html" "$root/web/app.js" "$dist/"
cp "$root/LICENSE" "$root/NOTICE" "$dist/"
python3 "$root/scripts/package-licenses.py" "$dist/licenses"
print "Built $dist"
print "Serve with: python3 -m http.server 8080 --directory '$dist'"
