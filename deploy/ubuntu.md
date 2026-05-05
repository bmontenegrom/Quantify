# Deploy en Ubuntu local

## Requisitos

- Ubuntu Server en la maquina de facultad
- Docker Engine
- Docker Compose plugin
- IP fija o nombre DNS local

## Instalacion

```bash
git clone <repo> /opt/quantify
cd /opt/quantify
cp .env.example .env
docker compose up -d --build
```

Abrir desde otra maquina de la red:

```text
http://IP_DEL_SERVIDOR:8080
```

## Backups

Para el MVP, SQLite y los CSV subidos viven en `./data`.

Backup manual:

```bash
tar -czf quantify-backup-$(date +%F).tar.gz data
```

Restauracion:

```bash
docker compose down
tar -xzf quantify-backup-YYYY-MM-DD.tar.gz
docker compose up -d
```

## Actualizacion

```bash
git pull
docker compose up -d --build
```
