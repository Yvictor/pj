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

3. Load balancer setup:
   ```bash
   pj \
     --proxy 0.0.0.0:8080:10.0.0.1:80 \
     --proxy 0.0.0.0:8080:10.0.0.2:80 \
     --proxy 0.0.0.0:8080:10.0.0.3:80
   ```
```

The changes include:

1. New argument parsing that supports multiple proxy mappings
2. A custom ProxyMapping struct to hold the configuration
3. A parser function to validate and parse the proxy mapping format
4. Updated server setup to create multiple services based on mappings
5. Better logging to show all active mappings
6. Updated documentation with examples

Now you can run multiple proxies with a single command like:

```bash
pj \
  --proxy 0.0.0.0:8787:127.0.0.1:22 \
  --proxy 0.0.0.0:8080:127.0.0.1:80 \
  --proxy 0.0.0.0:8443:127.0.0.1:443
```

Each proxy mapping will run independently in its own service within the Pingora server, with proper resource isolation and error handling.