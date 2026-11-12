ARG ALIA_RUST_IMAGE=rust:1.99.1-bookworm
ARG ALIA_BASE_IMAGE=debian:bookworm-slim
FROM node:22.16.0-bookworm-slim AS web
WORKDIR /build/web
COPY web/package*.json ./
RUN npm ci
COPY web/ ./
COPY config/persona.example.json config/runtime.example.json /build/config/
COPY evals/decision.example.json /build/evals/decision.example.json
COPY schemas/runtime.schema.json /build/schemas/runtime.schema.json
RUN npm run build

FROM ${ALIA_RUST_IMAGE} AS backend
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src/ src/
COPY migrations/ migrations/
COPY schemas/ schemas/
COPY config/embedding-assets.json config/embedding-assets.json
ARG ALIA_CARGO_FEATURES=""
RUN cargo build --release --locked --bin alia --features "$ALIA_CARGO_FEATURES"

FROM ${ALIA_BASE_IMAGE}
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libstdc++6 && rm -rf /var/lib/apt/lists/* && useradd -u 10001 -m alia
WORKDIR /app
COPY --from=backend /build/target/release/alia /app/alia
COPY --from=web /build/web/dist /app/web/dist
USER alia
ENV ALIA_BIND=0.0.0.0:3000 ALIA_WEB_DIR=/app/web/dist
EXPOSE 3000
CMD ["/app/alia"]
