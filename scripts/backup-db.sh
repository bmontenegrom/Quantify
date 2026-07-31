#!/usr/bin/env bash
# Copia la base SQLite a data/backups/ con timestamp. Respeta DATABASE_URL si esta definida
# (mismo default que el server). Pensado para correr a mano o desde cron en un deploy LAN.
set -euo pipefail
cd "$(dirname "$0")/.."

db_url="${DATABASE_URL:-sqlite:data/quantify.db}"
case "$db_url" in
  sqlite:*) db_path="${db_url#sqlite:}" ;;
  *) echo "DATABASE_URL no apunta a SQLite ($db_url); hace el backup a mano." >&2; exit 1 ;;
esac

if [ ! -e "$db_path" ]; then
  echo "No existe $db_path; nada para respaldar." >&2
  exit 1
fi

backup_dir="data/backups"
mkdir -p "$backup_dir"
timestamp="$(date +%Y%m%d-%H%M%S)"
dest="$backup_dir/quantify-$timestamp.db"

cp "$db_path" "$dest"
echo "Backup creado: $dest"
