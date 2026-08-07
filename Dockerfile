# Multi-stage Chimera node image (untested locally — no Docker on build host).
FROM rust:1.86-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p chimera -p chimeractl

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /src/target/release/chimera /usr/local/bin/chimera
COPY --from=builder /src/target/release/chimeractl /usr/local/bin/chimeractl
ENV RUST_LOG=chimera=info
EXPOSE 7400 7401 7600 7410/udp
VOLUME ["/data"]
ENTRYPOINT ["chimera"]
CMD ["--name", "node", "--data-dir", "/data", "--no-tui", "--mgmt-bind", "0.0.0.0:7600"]
