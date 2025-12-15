#!/bin/sh
set -e

# Start Caddy in background
# Default admin API is localhost:2019
echo "Starting Caddy..."
caddy run --config /etc/caddy/Caddyfile --adapter caddyfile 2>&1 &

# Wait for Caddy to be ready
echo "Waiting for Caddy..."
# Simple wait loop
until curl -s http://localhost:2019/config/ > /dev/null 2>&1; do
    sleep 1
done
echo "Caddy is up!"

# Start Coordinator
echo "Starting Coordinator..."
exec /usr/local/bin/coordinator "$@"
