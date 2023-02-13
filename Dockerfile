FROM lukemathwalker/cargo-chef:latest-rust-1.65.0 as chef
WORKDIR /app

FROM chef as planner
COPY . .
# Compute a lock-file
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE true
RUN cargo build --release --bin email-all

# Runtime stage
FROM debian:bullseye-slim AS runtime
WORKDIR /app

RUN apt-get update -y \
	&& apt-get install -y --no-install-recommends openssl ca-certificates \
	&& apt-get autoremove -y \
	&& apt-get clean -y \
	&& rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/api api
COPY configuration configuration

ENV APP_ENVIRONMENT production
ENTRYPOINT ["./api"]