ARG RUST_VERSION=1.90.0
FROM rust:${RUST_VERSION}-trixie AS build
WORKDIR /app

RUN rustup update nightly && rustup default nightly
RUN apt-get update && apt-get install -y cmake
RUN --mount=type=bind,source=src,target=src \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    cargo build --release && cp ./target/release/disrecord /bin/disrecord

FROM debian:trixie-slim AS final
ENV RECORD_DIR=/recordings

RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg && rm -rf /var/lib/apt/lists/*
COPY --from=build /bin/disrecord /bin/

CMD ["/bin/disrecord"]
