#!/bin/bash

set -e

RESTART_DELAY=2

log() {
    echo "[$(date '+%H:%M:%S')] $*"
}

log "Building images..."
sudo docker compose build

log "Starting cluster in background..."
sudo docker compose up &
COMPOSE_PID=$!

log "Compose running in background (PID $COMPOSE_PID). Waiting ${RESTART_DELAY}s before restarting s4..."
sleep "$RESTART_DELAY"

log "Restarting s4..."
sudo docker compose restart s4
log "s4 restarted."

log "Bringing compose back to foreground (Ctrl+C to stop)..."
wait "$COMPOSE_PID"