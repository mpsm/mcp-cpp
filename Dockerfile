# Multi-stage build for mcp-cpp-server
# Stage 1: Build the Rust binary
FROM rust:1.89 AS builder

WORKDIR /build

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build release binary
RUN cargo build --release

# Stage 2: Runtime image with clangd-20
FROM ubuntu:24.04

# Install dependencies and clangd-20
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        gnupg \
        lsb-release && \
    # Get Ubuntu codename
    UBUNTU_CODENAME=$(lsb_release -cs) && \
    # Add LLVM repository for clangd-20
    curl -fsSL https://apt.llvm.org/llvm-snapshot.gpg.key | gpg --dearmor -o /usr/share/keyrings/llvm-archive-keyring.gpg && \
    echo "deb [signed-by=/usr/share/keyrings/llvm-archive-keyring.gpg] http://apt.llvm.org/${UBUNTU_CODENAME}/ llvm-toolchain-${UBUNTU_CODENAME}-20 main" > /etc/apt/sources.list.d/llvm.list && \
    # Install clangd-20
    apt-get update && \
    apt-get install -y --no-install-recommends clangd-20 && \
    # Cleanup to reduce image size
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

# Copy binary from builder
COPY --from=builder /build/target/release/mcp-cpp-server /usr/local/bin/mcp-cpp-server

# Copy entrypoint script
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Set default working directory for C++ projects
WORKDIR /workspace

# Environment variables
ENV CLANGD_PATH=/usr/bin/clangd-20
ENV RUST_LOG=info

# Use entrypoint script
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

# Default arguments (can be overridden)
CMD ["--root", "/workspace"]
