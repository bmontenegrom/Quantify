#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

db_url="${DATABASE_URL:-sqlite:data/quantify.db}"
case "$db_url" in
  sqlite:*) db_path="${db_url#sqlite:}" ;;
  *) echo "DATABASE_URL no apunta a SQLite ($db_url); reseteala a mano." >&2; exit 1 ;;
esac
upload_dir="${UPLOAD_DIR:-data/uploads}"

for f in "$db_path" "$db_path-wal" "$db_path-shm"; do
  if [ -e "$f" ]; then
    rm -f "$f"
    echo "Borrado: $f"
  fi
done

if [ -d "$upload_dir" ]; then
  rm -rf "$upload_dir"
  echo "Borrado: $upload_dir"
fi

echo "Listo. El proximo 'cargo run' migra y crea el admin; con SEED_DEMO_DATA=true siembra tambien los datos de prueba."
