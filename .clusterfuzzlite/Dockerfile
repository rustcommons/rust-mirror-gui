# ClusterFuzzLite build image: the OSS-Fuzz Rust base image, pinned by
# digest (Dependabot proposes digest updates). Its built-in nightly is older
# than the project's minimum Rust (1.95), so the fuzz build uses a pinned,
# dated nightly instead: sanitizer builds need nightly, so the stable 1.95.0
# can't be used. The same nightly is used for local fuzzing (docs/FUZZING.md).
FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:3428d0a90b1e7efbe2ba947bd1d0c9a70594986b4b6553804547f1fb24cfc92f
ENV RUSTUP_TOOLCHAIN=nightly-2026-11-15
RUN rustup toolchain install nightly-2026-11-15 --profile minimal --component rust-src
COPY . $SRC/rust-mirror-gui
WORKDIR $SRC/rust-mirror-gui
COPY .clusterfuzzlite/build.sh $SRC/
