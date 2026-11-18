#!/bin/bash -eu
# Build every fuzz target in fuzz/ (cargo-fuzz, with the sanitizer flags the
# OSS-Fuzz environment sets) and package each target's seed corpus.
cd "$SRC/rust-mirror-gui"
cargo fuzz build -O --fuzz-dir fuzz
release=fuzz/target/x86_64-unknown-linux-gnu/release
for target in $(cargo fuzz list --fuzz-dir fuzz); do
  cp "$release/$target" "$OUT/"
  if [ -d "fuzz/seeds/$target" ]; then
    (cd "fuzz/seeds/$target" && zip -q -r "$OUT/${target}_seed_corpus.zip" .)
  fi
done
