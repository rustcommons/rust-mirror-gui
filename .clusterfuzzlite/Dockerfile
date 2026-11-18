# ClusterFuzzLite build image. The OSS-Fuzz Rust base image is pinned by
# digest, which also pins its nightly Rust toolchain and cargo-fuzz:
# sanitizer builds need nightly, so the project's stable 1.95.0 can't be used
# here. Dependabot (docker ecosystem) proposes digest updates.
FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:3428d0a90b1e7efbe2ba947bd1d0c9a70594986b4b6553804547f1fb24cfc92f
COPY . $SRC/rust-mirror-gui
WORKDIR $SRC/rust-mirror-gui
COPY .clusterfuzzlite/build.sh $SRC/
