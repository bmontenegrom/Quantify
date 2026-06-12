# Resetea la base de desarrollo: borra la base SQLite (+ archivos -wal/-shm) y los uploads.
# El proximo `cargo run` recrea el esquema y vuelve a sembrar los datos de prueba.
# Respeta DATABASE_URL y UPLOAD_DIR si estan definidas (mismos defaults que el server).
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

$dbUrl = if ($env:DATABASE_URL) { $env:DATABASE_URL } else { 'sqlite:data/quantify.db' }
if ($dbUrl -notlike 'sqlite:*') {
    throw "DATABASE_URL no apunta a SQLite ($dbUrl); reseteala a mano."
}
$dbPath = $dbUrl.Substring('sqlite:'.Length)
$uploadDir = if ($env:UPLOAD_DIR) { $env:UPLOAD_DIR } else { 'data/uploads' }

foreach ($f in @($dbPath, "$dbPath-wal", "$dbPath-shm")) {
    if (Test-Path $f) {
        try {
            Remove-Item $f -Force
            Write-Host "Borrado: $f"
        } catch {
            throw "No se pudo borrar $f (¿el server sigue corriendo?). Detenelo y volve a intentar."
        }
    }
}

if (Test-Path $uploadDir) {
    Remove-Item $uploadDir -Recurse -Force
    Write-Host "Borrado: $uploadDir"
}

Write-Host "Listo. El proximo 'cargo run' migra y vuelve a sembrar los datos de prueba."
