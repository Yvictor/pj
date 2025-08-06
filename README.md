# pj

A TCP reverse proxy built with pingora

## Usage

```bash
# Single proxy
pj --proxy 0.0.0.0:8787:127.0.0.1:22

# Multiple proxies
pj \
  --proxy 0.0.0.0:8787:127.0.0.1:22 \
  --proxy 0.0.0.0:8788:127.0.0.1:80 \
  --proxy 0.0.0.0:8789:127.0.0.1:443

# Show help
pj --help
```

## Options

```
Options:
  -p, --proxy <PROXY>    Proxy mapping in format "listen_ip:listen_port:proxy_ip:proxy_port"
                        Can be specified multiple times for multiple mappings
  -h, --help           Print help
  -V, --version        Print version
```

## Examples

1. SSH proxy:
   ```bash
   pj --proxy 0.0.0.0:8787:127.0.0.1:22
   ```

2. Multiple service proxy:
   ```bash
   pj --proxy 0.0.0.0:8787:127.0.0.1:22 --proxy 0.0.0.0:8080:127.0.0.1:80
   ```

3. Docker container proxy:
   ```bash
   pj --proxy 0.0.0.0:8080:172.17.0.2:80
   ```

## Building from Source

### Prerequisites
- Rust 1.82.0 or later
- CMake (required for building dependencies)

### Build Commands
```bash
# Clone the repository
git clone https://github.com/yvictor/pj.git
cd pj

# Build debug version
cargo build

# Build release version
cargo build --release

# Run directly with cargo
cargo run -- --proxy 0.0.0.0:8787:127.0.0.1:22
```

## Testing

The project includes comprehensive unit tests and integration tests to ensure reliability.

### Running Tests

```bash
# Run all tests
cargo test

# Run unit tests only
cargo test --lib

# Run integration tests only
cargo test --test integration_test

# Run tests with single thread (useful for integration tests)
cargo test -- --test-threads=1

# Run tests with output displayed
cargo test -- --nocapture

# Run a specific test
cargo test test_basic_proxy_functionality
```

### Test Coverage

#### Unit Tests (11 tests)
- **Parser Tests**: Validate proxy mapping format parsing
  - Valid IPv4 format parsing
  - Localhost format support
  - Invalid format error handling
  - Various edge cases

- **ProxyApp Tests**: Core proxy functionality
  - ProxyApp instance creation
  - Service creation and configuration
  - ProxyMapping clone and debug traits

- **DuplexEvent Tests**: Data transfer event handling
  - Downstream read events
  - Upstream read events
  - Event structure validation

#### Integration Tests (5 tests)
- **Basic Proxy Functionality**: End-to-end proxy data transfer
- **Multiple Concurrent Connections**: Simultaneous connection handling
- **Multiple Proxy Mappings**: Multiple port mappings in single instance
- **Large Data Transfer**: Bulk data transmission (10KB+)
- **Error Handling**: Unreachable upstream server scenarios

### Test Examples

```bash
# Run tests with verbose output
cargo test -- --nocapture

# Run tests in release mode for performance testing
cargo test --release

# Check test coverage (requires cargo-tarpaulin)
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

## Docker Support

### Building Docker Image
```bash
# Build the Docker image
make image

# Run the container
make run
```

### Docker Compose Example
```yaml
version: '3'
services:
  tcp-proxy:
    image: pj:latest
    command: --proxy 0.0.0.0:8787:backend:22 --proxy 0.0.0.0:8080:backend:80
    ports:
      - "8787:8787"
      - "8080:8080"
```

## Architecture

The proxy is built using Cloudflare's Pingora framework and follows these design principles:

- **Async I/O**: Uses Tokio for high-performance async operations
- **Zero-copy**: Efficient data transfer between client and upstream
- **Memory Safety**: Written in Rust with compile-time guarantees
- **Resource Isolation**: Each proxy mapping runs in its own service
- **1:1 Mapping**: Each listening port maps to exactly one backend (no load balancing)

## Performance

The proxy is optimized for low latency and high throughput:
- Uses jemalloc for efficient memory management
- Implements bidirectional data copying with minimal overhead
- Supports thousands of concurrent connections
- Buffer size: 1024 bytes (configurable in source)

## Contributing

Contributions are welcome! Please ensure:
1. All tests pass (`cargo test`)
2. Code follows Rust conventions (`cargo fmt` and `cargo clippy`)
3. New features include appropriate tests

## TODO

### Phase 1: Production Readiness
- [ ] **Error Handling**: Remove all `unwrap()` calls and implement proper error handling
  - Replace with proper `Result` types and error propagation
  - Add graceful error recovery
  - Implement comprehensive error logging
  - Ensure no panics in production

### Phase 2: Observability
- [ ] **Connection Logging**: Display connection information
  - Log client IP addresses
  - Log destination addresses
  - Connection timestamps
  - Connection duration
  - Bytes transferred
  - Connection status (success/failure)
  - Format: `[timestamp] client_ip:port -> proxy:port -> backend:port [status]`

### Phase 3: Load Balancing
- [ ] **Load Balancing**: Support multiple backends for a single listening port
  - Round-robin algorithm
  - Least connections algorithm
  - Health checks for backend servers
  - Automatic failover
  - Configuration format: `--proxy "0.0.0.0:8080:backend1:80,backend2:80,backend3:80"`

### Future Enhancements
- [ ] **Metrics & Monitoring**: Add Prometheus metrics endpoint
- [ ] **Configuration File**: Support YAML/TOML configuration files
- [ ] **Hot Reload**: Reload configuration without downtime
- [ ] **TLS/SSL Support**: Add support for encrypted connections
- [ ] **Connection Pooling**: Reuse upstream connections for better performance
- [ ] **Rate Limiting**: Add per-client rate limiting capabilities
- [ ] **Access Control**: IP-based access control lists
