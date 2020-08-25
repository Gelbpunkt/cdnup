# Mount rclone config as rclone.conf at /
FROM docker.io/library/alpine:edge AS builder

RUN apk add --no-cache curl gcc musl-dev perl make openssl-dev && \
    curl -sSf https://sh.rustup.rs | sh -s -- --profile minimal --default-toolchain nightly -y

WORKDIR /build
COPY . .

RUN set -ex && \
    source $HOME/.cargo/env && \
    cargo build --release && \
    strip /build/target/release/cdnup

FROM docker.io/library/alpine:edge

COPY --from=builder /build/target/release/cdnup /usr/bin/cdnup

ENTRYPOINT /usr/bin/cdnup
