# syntax=docker/dockerfile:1
#
# Peridot artifact image (Middleware '26).
#
# Build from the REPOSITORY ROOT, not from artifact/:
#     docker build -f artifact/Dockerfile -t peridot-ae .
#
# The image carries the runtime, the Overseer, every built-in context, the
# example plugin, the external batcher and all prebuilt guest modules, so a
# reviewer never needs Rust, protoc, cmake or wasi-sdk.

# ---------------------------------------------------------------- builder ---
# Pinned by digest, not just by tag: tags are mutable, and an artifact has to
# build the same way years after publication. The tag is kept in the comment
# so a reader can tell what this is.
# rust:1.93-bookworm
FROM rust@sha256:7c4ae649a84014c467d79319bbf17ce2632ae8b8be123ac2fb2ea5be46823f31 AS builder

# protoc  - peridot-overseer-grpc compiles its protobufs at build time
# cmake   - aws-lc-sys, pulled in by the AWS SDK the s3 and batch contexts use
RUN apt-get update && apt-get install -y --no-install-recommends \
        protobuf-compiler \
        cmake \
        build-essential \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .

# One cargo invocation for the runtime AND the plugin, deliberately.
#
# Contexts cross the plugin boundary as ordinary Rust types, which have no
# stable ABI, so both sides must link one identical build of `peridot`. Cargo
# resolves features per invocation, so splitting this into two RUN layers can
# silently produce two different builds of `peridot` from the same lockfile,
# and Peridot would then refuse to load the plugin at run time. See the
# "Plugins" section of docs/peridot.md.
#
# --locked: build exactly the dependency graph in Cargo.lock, including the
# pinned runwasi revision. If this fails, the lockfile and manifests disagree
# and that is a bug worth knowing about, not something to paper over.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked \
        -p peridot-cli \
        -p peridot-overseer \
        -p tensor_batcher \
        -p peridot-trace-plugin \
 && mkdir -p /out \
 && cp target/release/peridot \
       target/release/peridot-overseer \
       target/release/tensor_batcher /out/ \
 && cp target/release/libperidot_trace_plugin.so /out/

# ---------------------------------------------------------------- runtime ---
# debian:bookworm-slim
FROM debian@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251

# python3 drives the plotting and the S3 bookkeeping the experiments need.
RUN apt-get update && apt-get install -y --no-install-recommends \
        python3 \
        python3-pip \
        ca-certificates \
        procps \
    && rm -rf /var/lib/apt/lists/*

# Pinned here too, so a figure regenerated later looks like one regenerated now.
COPY artifact/plots/requirements.txt /tmp/requirements.txt
RUN pip3 install --no-cache-dir --break-system-packages -r /tmp/requirements.txt \
    && rm /tmp/requirements.txt

COPY --from=builder /out/peridot          /usr/local/bin/peridot
COPY --from=builder /out/peridot-overseer /usr/local/bin/peridot-overseer
COPY --from=builder /out/tensor_batcher   /usr/local/bin/tensor_batcher
COPY --from=builder /out/libperidot_trace_plugin.so /usr/local/lib/

WORKDIR /artifact
# Guests are copied in already compiled. Nothing in this image rebuilds them;
# artifact/bin/build-guests.sh regenerates them from source on a machine that
# has wasi-sdk, which is how they stay auditable.
COPY artifact/guests       /artifact/guests
COPY artifact/configs      /artifact/configs
COPY artifact/experiments  /artifact/experiments
COPY artifact/plots        /artifact/plots
COPY artifact/results/reference /artifact/results/reference
COPY artifact/CLAIMS.txt         /artifact/CLAIMS.txt
COPY artifact/CLAIMS-SUMMARY.txt /artifact/CLAIMS-SUMMARY.txt

# The guide, the licence and the reference manual sit at the image root, in the
# same arrangement as the repository, so every relative link in the guide
# resolves identically inside the image and in a checkout. A distributed image
# should also carry its own licence terms.
COPY README.md               /README.md
COPY LICENSE                 /LICENSE
COPY THIRD-PARTY-NOTICES.md  /THIRD-PARTY-NOTICES.md
COPY docs                    /docs

ENV PERIDOT_BIN=/usr/local/bin/peridot \
    OVERSEER_BIN=/usr/local/bin/peridot-overseer \
    BATCHER_BIN=/usr/local/bin/tensor_batcher \
    GUESTS=/artifact/guests \
    CONFIGS=/artifact/configs \
    RESULTS=/artifact/results \
    PYTHONDONTWRITEBYTECODE=1

# Experiments write into /artifact/results; mount a volume there to keep the
# CSVs and figures after the container exits.
VOLUME ["/artifact/results", "/artifact/figures"]

CMD ["/artifact/experiments/smoke.sh"]
