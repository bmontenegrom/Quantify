#!/usr/bin/env bash
# Resetea la base de desarrollo: borra la base SQLite (+ archivos -wal/-shm) y los uploads.
# El proximo `cargo run` recrea el esquema y vuelve a sembrar los datos de prueba.
# Respeta DATABASE_URL y UPLOAD_DIR si estan definidas (mismos defaults que el server).
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

echo "Listo. El proximo 'cargo run' migra y vuelve a sembrar los datos de prueba."
