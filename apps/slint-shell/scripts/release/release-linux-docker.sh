#!/bin/sh
# Build every Linux release artifact in an ubuntu:22.04 Docker container, like
# the release workflow, so the binary runs on the oldest supported glibc.
# The result replaces <cargo-target>/release-artifacts on the host.
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd -- "$repo_dir"
workflow=.github/workflows/release.yml
nfpm_version=$(sed -n 's/^  NFPM_VERSION: *//p' "$workflow")
nfpm_sha256=$(sed -n 's/^  NFPM_SHA256: *//p' "$workflow")
channel=$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)
if [ -z "$nfpm_version" ] || [ -z "$nfpm_sha256" ] || [ -z "$channel" ]; then
    echo "Cannot read the nfpm or Rust version from $workflow / rust-toolchain.toml." >&2
    exit 1
fi
target_dir=$(cargo metadata --locked --format-version 1 --no-deps \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
image="tecloid-release-build:rust-$channel-nfpm-$nfpm_version"

# Same system packages as the workflow; the toolchain is baked in so the build
# can run as the host user and leave no root-owned files.
docker build -t "$image" \
    --build-arg "CHANNEL=$channel" \
    --build-arg "NFPM_VERSION=$nfpm_version" \
    --build-arg "NFPM_SHA256=$nfpm_sha256" - <<'DOCKERFILE'
FROM ubuntu:22.04
ARG CHANNEL
ARG NFPM_VERSION
ARG NFPM_SHA256
ENV DEBIAN_FRONTEND=noninteractive RUSTUP_HOME=/opt/rustup PATH=/opt/cargo/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends build-essential \
      ca-certificates clang cmake curl git ninja-build pkg-config python3 xz-utils \
      libfontconfig1-dev libfreetype-dev libwayland-dev libxkbcommon-dev lintian \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSLo /tmp/nfpm.tar.gz \
      "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_Linux_x86_64.tar.gz" \
    && echo "${NFPM_SHA256}  /tmp/nfpm.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/nfpm.tar.gz -C /usr/local/bin nfpm && rm /tmp/nfpm.tar.gz
RUN curl -fsSL https://sh.rustup.rs | CARGO_HOME=/opt/cargo sh -s -- -y --no-modify-path \
      --profile minimal --default-toolchain "$CHANNEL" -c rustfmt -c clippy \
    && chmod -R a+rX /opt/rustup /opt/cargo
DOCKERFILE

# A separate target and Cargo home: host build outputs link against the host
# glibc and must not be reused inside the container.
build_dir="$target_dir/release-container"
mkdir -p -- "$build_dir/cargo-home"
docker run --rm --user "$(id -u):$(id -g)" \
    -v "$repo_dir:/src" -v "$build_dir:/build" -w /src \
    -e HOME=/tmp -e CARGO_HOME=/build/cargo-home -e CARGO_TARGET_DIR=/build/target \
    "$image" sh -c 'apps/slint-shell/scripts/release/release-linux.sh &&
        lintian --fail-on error /build/target/release-artifacts/*.deb'

rm -rf -- "$target_dir/release-artifacts"
cp -R -- "$build_dir/target/release-artifacts" "$target_dir/release-artifacts"
echo "Release artifacts: $target_dir/release-artifacts"
