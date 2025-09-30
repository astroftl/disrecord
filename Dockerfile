ARG RUST_VERSION=1.90.0
FROM rust:${RUST_VERSION}-slim-bullseye AS build
WORKDIR /app

RUN --mount=type=bind,source=src,target=src \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    <<EOF
set -e
apt-get update
apt-get install -y cmake
rustup update nightly
rustup default nightly
cargo build --locked --release
cp ./target/release/disrecord /bin/disrecord
EOF

FROM debian:bullseye-slim AS final

COPY --from=build /bin/disrecord /bin/

CMD ["/bin/disrecord"]
