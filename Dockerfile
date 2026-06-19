FROM rust:1.80-slim as builder

WORKDIR /app
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/insight /usr/local/bin/insight
ENV DATABASE_URL=mysql://root:root@localhost:3306/philand
ENV GRPC_HOST=0.0.0.0
ENV GRPC_PORT=50108
ENV HTTP_HOST=0.0.0.0
ENV HTTP_PORT=9108
EXPOSE 50108 9108
CMD ["insight"]