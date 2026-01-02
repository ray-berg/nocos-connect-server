# NOCOS-Connect Server User Manual

## Table of Contents

1. [Overview](#overview)
2. [System Requirements](#system-requirements)
3. [Installation](#installation)
4. [Configuration](#configuration)
5. [Network Setup](#network-setup)
6. [Key Management](#key-management)
7. [Running as a Service](#running-as-a-service)
8. [Administration](#administration)
9. [Troubleshooting](#troubleshooting)
10. [Security Considerations](#security-considerations)

---

## Overview

NOCOS-Connect Server provides the backend infrastructure for remote desktop connectivity. It consists of two main components:

| Component | Binary | Default Port | Purpose |
|-----------|--------|--------------|---------|
| **ID/Rendezvous Server** | `hbbs` | 21116 | Peer registration, discovery, and NAT traversal coordination |
| **Relay Server** | `hbbr` | 21117 | Data forwarding when direct peer-to-peer connections cannot be established |

### How It Works

1. Clients register with the Rendezvous Server (hbbs)
2. When a connection is requested, hbbs coordinates NAT punch-through
3. If direct connection fails, traffic is routed through the Relay Server (hbbr)

---

## System Requirements

### Minimum Hardware
- **CPU**: 1 core (2+ recommended for production)
- **RAM**: 512 MB (1 GB+ recommended)
- **Storage**: 100 MB for binaries + space for logs and database
- **Network**: Static IP address or domain name

### Supported Platforms
- Linux (x86_64, ARM64)
- Windows (x86_64)
- macOS (x86_64, ARM64)

### Build Requirements (if compiling from source)
- Rust 1.70 or later
- C compiler (gcc/clang)
- OpenSSL development libraries (Linux)

---

## Installation

### Option 1: Building from Source

```bash
# Clone the repository
git clone https://github.com/your-org/nocos-connect-server.git
cd nocos-connect-server

# Initialize submodules
git submodule update --init --recursive

# Build release binaries
cargo build --release

# Binaries are located in target/release/
ls target/release/hbbs target/release/hbbr target/release/nocos-connect-utils
```

### Option 2: Pre-built Binaries

Download the latest release for your platform and extract:

```bash
# Linux example
tar -xzf nocos-connect-server-linux-x64.tar.gz
cd nocos-connect-server

# Make binaries executable
chmod +x hbbs hbbr nocos-connect-utils
```

### Option 3: Docker

```bash
# Using Docker Compose
docker-compose up -d

# Or run containers individually
docker run -d --name hbbs \
  -p 21115:21115 -p 21116:21116 -p 21116:21116/udp -p 21118:21118 \
  -v ./data:/root \
  nocos/connect-server hbbs

docker run -d --name hbbr \
  -p 21117:21117 -p 21119:21119 \
  -v ./data:/root \
  nocos/connect-server hbbr
```

### Verify Installation

```bash
# Check versions
./hbbs --version
./hbbr --version

# Test server connectivity
./nocos-connect-utils doctor your-server-address
```

---

## Configuration

### Rendezvous Server (hbbs)

```bash
hbbs [OPTIONS]
```

| Option | Environment Variable | Default | Description |
|--------|---------------------|---------|-------------|
| `-p, --port` | `PORT` | 21116 | Main listening port (UDP + TCP) |
| `-k, --key` | `KEY` | (generated) | License key for client authentication |
| `-r, --relay-servers` | `RELAY_SERVERS` | (empty) | Comma-separated list of relay servers |
| `-R, --rendezvous-servers` | `RENDEZVOUS_SERVERS` | (empty) | Comma-separated list of rendezvous servers |
| `-s, --serial` | `SERIAL` | 0 | Configuration serial number |
| `-u, --software-url` | `SOFTWARE_URL` | (empty) | URL for client software updates |
| `-c, --config` | - | `.env` | Path to configuration file |
| `--mask` | `MASK` | (empty) | LAN subnet mask (e.g., `192.168.0.0/16`) |
| `-M, --rmem` | `RMEM` | 0 | UDP receive buffer size |

**Port Allocation:**
- Main port (default 21116): UDP + TCP for peer registration
- NAT test port (main - 1 = 21115): TCP for NAT type detection
- WebSocket port (main + 2 = 21118): WebSocket connections

### Relay Server (hbbr)

```bash
hbbr [OPTIONS]
```

| Option | Environment Variable | Default | Description |
|--------|---------------------|---------|-------------|
| `-p, --port` | `PORT` | 21117 | Main listening port |
| `-k, --key` | `KEY` | (empty) | License key (must match hbbs) |

**Port Allocation:**
- Main port (default 21117): TCP relay connections
- WebSocket port (main + 2 = 21119): WebSocket relay connections

**Bandwidth Control (Environment Variables):**

| Variable | Default | Description |
|----------|---------|-------------|
| `TOTAL_BANDWIDTH` | 1024 | Total bandwidth limit (Mb/s) |
| `SINGLE_BANDWIDTH` | 128 | Per-connection bandwidth limit (Mb/s) |
| `LIMIT_SPEED` | 32 | Speed limit for blacklisted IPs (Mb/s) |
| `DOWNGRADE_THRESHOLD` | 0.66 | Threshold for automatic quality downgrade |
| `DOWNGRADE_START_CHECK` | 1800 | Seconds before checking for downgrade |

### Configuration File (.env)

Create a `.env` file in the working directory:

```ini
# Logging level: trace, debug, info, warn, error
RUST_LOG=info

# Server key (base64-encoded)
KEY=your-key-here

# Relay servers (comma-separated)
RELAY_SERVERS=relay1.example.com,relay2.example.com:21117

# Database connection
DB_URL=./db_v2.sqlite3
MAX_DATABASE_CONNECTIONS=4

# Bandwidth limits (Mb/s)
TOTAL_BANDWIDTH=1024
SINGLE_BANDWIDTH=128

# Force all connections through relay
ALWAYS_USE_RELAY=N
```

### Example Deployment

**Single Server Setup:**
```bash
# Generate key pair
./nocos-connect-utils genkeypair

# Start rendezvous server with key
./hbbs -k YOUR_PUBLIC_KEY

# Start relay server (same key)
./hbbr -k YOUR_PUBLIC_KEY
```

**Multi-Server Setup:**
```bash
# On rendezvous server (server1.example.com)
./hbbs -k YOUR_KEY -r relay1.example.com,relay2.example.com

# On relay server 1 (relay1.example.com)
./hbbr -k YOUR_KEY

# On relay server 2 (relay2.example.com)
./hbbr -k YOUR_KEY
```

---

## Network Setup

### Required Ports

| Port | Protocol | Service | Direction |
|------|----------|---------|-----------|
| 21115 | TCP | NAT test | Inbound |
| 21116 | TCP + UDP | Rendezvous | Inbound |
| 21117 | TCP | Relay | Inbound |
| 21118 | TCP | Rendezvous WebSocket | Inbound |
| 21119 | TCP | Relay WebSocket | Inbound |

### Firewall Configuration

**Linux (iptables):**
```bash
# Allow NOCOS-Connect ports
iptables -A INPUT -p tcp --dport 21115:21119 -j ACCEPT
iptables -A INPUT -p udp --dport 21116 -j ACCEPT
```

**Linux (firewalld):**
```bash
firewall-cmd --permanent --add-port=21115-21119/tcp
firewall-cmd --permanent --add-port=21116/udp
firewall-cmd --reload
```

**Linux (ufw):**
```bash
ufw allow 21115:21119/tcp
ufw allow 21116/udp
```

**Windows Firewall:**
```powershell
New-NetFirewallRule -DisplayName "NOCOS-Connect" -Direction Inbound -Protocol TCP -LocalPort 21115-21119 -Action Allow
New-NetFirewallRule -DisplayName "NOCOS-Connect UDP" -Direction Inbound -Protocol UDP -LocalPort 21116 -Action Allow
```

### Reverse Proxy (Optional)

For WebSocket connections behind nginx:

```nginx
# /etc/nginx/conf.d/nocos-connect.conf
upstream hbbs_ws {
    server 127.0.0.1:21118;
}

upstream hbbr_ws {
    server 127.0.0.1:21119;
}

server {
    listen 443 ssl;
    server_name connect.example.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location /hbbs {
        proxy_pass http://hbbs_ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }

    location /hbbr {
        proxy_pass http://hbbr_ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

---

## Key Management

### Generating Keys

```bash
# Generate a new keypair
./nocos-connect-utils genkeypair

# Output:
# Public Key:  abc123...
# Secret Key:  xyz789...
```

### Key Files

When hbbs starts, it creates or reads key files:

| File | Contents |
|------|----------|
| `id_ed25519` | Base64-encoded secret key (64 bytes) |
| `id_ed25519.pub` | Base64-encoded public key (32 bytes) |

### Using Keys

**Option 1: Auto-generated (default)**
```bash
# Keys are generated on first run and saved to id_ed25519
./hbbs
```

**Option 2: Command-line**
```bash
# Use specific key (secret key or public key)
./hbbs -k YOUR_SECRET_KEY_BASE64
```

**Option 3: Key file**
```bash
# Place secret key in id_ed25519 file
echo "YOUR_SECRET_KEY_BASE64" > id_ed25519
./hbbs
```

### Validating Keys

```bash
./nocos-connect-utils validatekeypair "PUBLIC_KEY" "SECRET_KEY"
# Output: Key pair is VALID
```

### Client Configuration

Clients must be configured with:
1. **Server address**: Your hbbs server hostname/IP
2. **Public key**: The public key from `id_ed25519.pub`

---

## Running as a Service

### Linux (systemd)

**Create service file for hbbs:**
```ini
# /etc/systemd/system/nocos-hbbs.service
[Unit]
Description=NOCOS-Connect ID/Rendezvous Server
After=network.target

[Service]
Type=simple
User=nocos
Group=nocos
WorkingDirectory=/opt/nocos-connect
ExecStart=/opt/nocos-connect/hbbs
Restart=always
RestartSec=5
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
```

**Create service file for hbbr:**
```ini
# /etc/systemd/system/nocos-hbbr.service
[Unit]
Description=NOCOS-Connect Relay Server
After=network.target

[Service]
Type=simple
User=nocos
Group=nocos
WorkingDirectory=/opt/nocos-connect
ExecStart=/opt/nocos-connect/hbbr
Restart=always
RestartSec=5
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
```

**Enable and start services:**
```bash
# Create service user
sudo useradd -r -s /bin/false nocos

# Set permissions
sudo chown -R nocos:nocos /opt/nocos-connect

# Enable services
sudo systemctl daemon-reload
sudo systemctl enable nocos-hbbs nocos-hbbr
sudo systemctl start nocos-hbbs nocos-hbbr

# Check status
sudo systemctl status nocos-hbbs nocos-hbbr
```

### Windows (NSSM)

```powershell
# Install NSSM (Non-Sucking Service Manager)
# Download from https://nssm.cc/

# Install hbbs as service
nssm install nocos-hbbs "C:\nocos-connect\hbbs.exe"
nssm set nocos-hbbs AppDirectory "C:\nocos-connect"
nssm set nocos-hbbs Description "NOCOS-Connect ID/Rendezvous Server"

# Install hbbr as service
nssm install nocos-hbbr "C:\nocos-connect\hbbr.exe"
nssm set nocos-hbbr AppDirectory "C:\nocos-connect"
nssm set nocos-hbbr Description "NOCOS-Connect Relay Server"

# Start services
nssm start nocos-hbbs
nssm start nocos-hbbr
```

### Docker Compose

```yaml
# docker-compose.yml
version: '3'

services:
  hbbs:
    image: nocos/connect-server:latest
    command: hbbs -r hbbr:21117
    ports:
      - "21115:21115"
      - "21116:21116"
      - "21116:21116/udp"
      - "21118:21118"
    volumes:
      - ./data:/root
    restart: unless-stopped

  hbbr:
    image: nocos/connect-server:latest
    command: hbbr
    ports:
      - "21117:21117"
      - "21119:21119"
    volumes:
      - ./data:/root
    restart: unless-stopped
```

---

## Administration

### Runtime Commands

Connect to the admin interface via TCP on loopback:

```bash
# Connect to hbbs admin (port 21115)
echo "h" | nc localhost 21115

# Connect to hbbr admin (port 21117)
echo "h" | nc localhost 21117
```

**hbbs Commands:**

| Command | Description |
|---------|-------------|
| `h` | Show help |
| `rs <servers>` | Set relay servers (comma-separated) |
| `ib [ip]` | Show/query IP blocker status |
| `ib <ip> -` | Remove IP from blocker |
| `ic [id]` | Show/query IP changes |
| `aur [Y/N]` | Get/set always-use-relay mode |
| `tg <ip1> <ip2>` | Test geo-routing between IPs |

**hbbr Commands:**

| Command | Description |
|---------|-------------|
| `h` | Show help |
| `ba <ip>` | Add IP to blacklist (throttled) |
| `br <ip>` | Remove IP from blacklist |
| `b [ip]` | Show blacklist / check IP |
| `Ba <ip>` | Add IP to blocklist (blocked) |
| `Br <ip>` | Remove IP from blocklist |
| `B [ip]` | Show blocklist / check IP |
| `dt [value]` | Get/set downgrade threshold |
| `t [seconds]` | Get/set downgrade start check time |
| `ls [Mb/s]` | Get/set limit speed for blacklisted IPs |
| `tb [Mb/s]` | Get/set total bandwidth limit |
| `sb [Mb/s]` | Get/set single connection bandwidth |
| `u` | Show usage statistics |

### IP Blocking

**Blacklist** (throttles connections):
```bash
# Add to blacklist
echo "ba 192.168.1.100" | nc localhost 21117

# Remove from blacklist
echo "br 192.168.1.100" | nc localhost 21117
```

**Blocklist** (blocks connections):
```bash
# Add to blocklist
echo "Ba 192.168.1.100" | nc localhost 21117

# Remove from blocklist
echo "Br 192.168.1.100" | nc localhost 21117
```

**Persistent blocking** (survives restart):

Create `blacklist.txt` or `blocklist.txt` in the working directory:
```
# One IP per line
192.168.1.100
10.0.0.50
```

### Database

The SQLite database (`db_v2.sqlite3`) stores:
- Peer registrations (ID, UUID, public key)
- Connection metadata

**Backup:**
```bash
sqlite3 db_v2.sqlite3 ".backup 'backup.sqlite3'"
```

**Query peers:**
```bash
sqlite3 db_v2.sqlite3 "SELECT id, info FROM peer LIMIT 10;"
```

---

## Troubleshooting

### Diagnostic Tool

```bash
./nocos-connect-utils doctor your-server-address
```

This checks:
- DNS resolution
- TCP connectivity on all ports
- Reverse DNS lookup

### Common Issues

#### Server won't start

**Symptom:** "Address already in use"
```bash
# Check what's using the port
sudo lsof -i :21116
sudo netstat -tlnp | grep 21116

# Kill conflicting process or change port
./hbbs -p 21200
```

**Symptom:** "Permission denied"
```bash
# Use port above 1024 or run as root
# Or grant capability
sudo setcap 'cap_net_bind_service=+ep' ./hbbs
```

#### Clients can't connect

1. **Check firewall:** Ensure ports 21115-21119 are open
2. **Check key:** Client must have correct public key
3. **Check DNS:** Server address must resolve correctly
4. **Check logs:** Set `RUST_LOG=debug` for verbose output

#### NAT traversal fails

**Symptom:** Direct connections always fail, relay used

1. **Check NAT test port:** Port 21115 must be accessible
2. **Check UDP:** Port 21116/UDP must be open
3. **Symmetric NAT:** Some NAT types require relay; this is expected

#### High memory usage

```bash
# Limit database connections
export MAX_DATABASE_CONNECTIONS=2

# Check peer count
sqlite3 db_v2.sqlite3 "SELECT COUNT(*) FROM peer;"
```

#### Relay performance issues

```bash
# Check current usage
echo "u" | nc localhost 21117

# Adjust bandwidth limits
echo "tb 2048" | nc localhost 21117  # Total: 2 Gb/s
echo "sb 256" | nc localhost 21117   # Per-connection: 256 Mb/s
```

### Logs

**Enable debug logging:**
```bash
export RUST_LOG=debug
./hbbs
```

**Log levels:**
- `error`: Critical errors only
- `warn`: Warnings and errors
- `info`: General information (default)
- `debug`: Detailed debugging
- `trace`: Very verbose tracing

---

## Security Considerations

### Deployment Best Practices

1. **Use a dedicated user:** Don't run as root
   ```bash
   sudo useradd -r -s /bin/false nocos
   sudo -u nocos ./hbbs
   ```

2. **Protect key files:**
   ```bash
   chmod 600 id_ed25519
   chown nocos:nocos id_ed25519
   ```

3. **Use a license key:** Prevents unauthorized relay usage
   ```bash
   ./hbbs -k YOUR_SECRET_KEY
   ./hbbr -k YOUR_SECRET_KEY
   ```

4. **Enable TLS:** Use a reverse proxy (nginx/caddy) for WebSocket TLS

5. **Monitor logs:** Watch for unusual patterns
   ```bash
   journalctl -u nocos-hbbs -f
   ```

6. **Rate limiting:** The server has built-in rate limiting:
   - 30 registrations per IP per minute
   - 300 unique peers per IP per day

### Network Security

1. **Firewall:** Only expose required ports
2. **VPN:** Consider placing servers behind VPN for internal use
3. **Reverse proxy:** Add authentication layer if needed

### Updates

Regularly update to get security fixes:
```bash
git pull
cargo build --release
sudo systemctl restart nocos-hbbs nocos-hbbr
```

---

## Appendix

### Environment Variables Reference

| Variable | Component | Description |
|----------|-----------|-------------|
| `RUST_LOG` | Both | Log level (trace/debug/info/warn/error) |
| `PORT` | Both | Override default port |
| `KEY` | Both | License/authentication key |
| `DB_URL` | hbbs | Database file path |
| `MAX_DATABASE_CONNECTIONS` | hbbs | Connection pool size |
| `RELAY_SERVERS` | hbbs | Comma-separated relay servers |
| `RENDEZVOUS_SERVERS` | hbbs | Comma-separated rendezvous servers |
| `ALWAYS_USE_RELAY` | hbbs | Force relay mode (Y/N) |
| `TOTAL_BANDWIDTH` | hbbr | Total bandwidth limit (Mb/s) |
| `SINGLE_BANDWIDTH` | hbbr | Per-connection limit (Mb/s) |
| `LIMIT_SPEED` | hbbr | Blacklist speed limit (Mb/s) |
| `DOWNGRADE_THRESHOLD` | hbbr | Quality downgrade threshold |
| `DOWNGRADE_START_CHECK` | hbbr | Seconds before downgrade check |

### File Locations

| File | Purpose |
|------|---------|
| `id_ed25519` | Server secret key |
| `id_ed25519.pub` | Server public key |
| `db_v2.sqlite3` | Peer database |
| `.env` | Configuration file |
| `blacklist.txt` | Persistent IP blacklist |
| `blocklist.txt` | Persistent IP blocklist |

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error / invalid arguments |

---

## Support

For issues and feature requests, please use the project issue tracker.
