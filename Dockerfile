FROM rust:1-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
COPY static ./static
RUN cargo build --release

FROM debian:bookworm-slim

RUN useradd --system --uid 10001 --create-home quantify
WORKDIR /app
COPY --from=builder /app/target/release/quantify /usr/local/bin/quantify
COPY static ./static
RUN mkdir -p /app/data/uploads && chown -R quantify:quantify /app

USER quantify
ENV APP_BIND_ADDR=0.0.0.0:8080
ENV DATABASE_URL=sqlite:/app/data/quantify.db
ENV UPLOAD_DIR=/app/data/uploads
EXPOSE 8080

CMD ["quantify"]
