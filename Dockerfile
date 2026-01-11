# =========================
# Builder stage
# =========================
FROM rustlang/rust:nightly AS builder

WORKDIR /app
COPY . .

# Build release binaries
RUN cargo build --release -p panel --bins


# =========================
# Runtime stage
# =========================
FROM debian:trixie-slim

# TLS + wait-for-db (netcat)
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Main server
COPY --from=builder /app/target/release/panel /app/panel
COPY --from=builder /app/target/release/create_user /app/create_user
COPY --from=builder /app/public /app/public

# Entrypoint
COPY entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

EXPOSE 8080
ENV RUST_LOG=info

ENTRYPOINT ["/app/entrypoint.sh"]
