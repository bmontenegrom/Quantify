# Copia la base SQLite a data/backups/ con timestamp. Respeta DATABASE_URL si esta definida
# (mismo default que el server). Pensado para correr a mano o desde el Task Scheduler en un deploy LAN.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

$dbUrl = if ($env:DATABASE_URL) { $env:DATABASE_URL } else { 'sqlite:data/quantify.db' }
if ($dbUrl -notlike 'sqlite:*') {
    throw "DATABASE_URL no apunta a SQLite ($dbUrl); hace el backup a mano."
}
$dbPath = $dbUrl.Substring('sqlite:'.Length)

if (-not (Test-Path $dbPath)) {
    throw "No existe $dbPath; nada para respaldar."
}

if (-not (Get-Command sqlite3 -ErrorAction SilentlyContinue)) {
    throw "Falta el binario sqlite3 en PATH; instalalo (necesario para un backup consistente, un copy crudo puede quedar incompleto si hay -wal pendiente)."
}

$backupDir = "data/backups"
New-Item -ItemType Directory -Force -Path $backupDir | Out-Null
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$dest = (Join-Path (Resolve-Path $backupDir) "quantify-$timestamp.db") -replace '\\', '/'

sqlite3 $dbPath ".backup '$dest'"
Write-Host "Backup creado: $dest"
