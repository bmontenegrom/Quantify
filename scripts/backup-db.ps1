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

$backupDir = "data/backups"
New-Item -ItemType Directory -Force -Path $backupDir | Out-Null
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$dest = Join-Path $backupDir "quantify-$timestamp.db"

Copy-Item $dbPath $dest
Write-Host "Backup creado: $dest"
