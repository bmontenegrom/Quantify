# Deploy de Quantify

Guía para correr Quantify en producción/laboratorio, especialmente cuando se accede
desde varias PCs en una LAN.

## Variables de entorno

| Variable | Default | Descripción |
|----------|---------|-------------|
| `APP_BIND_ADDR` | `127.0.0.1:8080` | Dirección y puerto donde escucha el server. Para LAN: `0.0.0.0:8080`. |
| `DATABASE_URL` | `sqlite:data/quantify.db` | Ruta de la base SQLite. |
| `UPLOAD_DIR` | `data/uploads` | Carpeta de archivos subidos. |
| `APP_SECRET_KEY` | _(efímera)_ | Clave para derivar los tokens CSRF (HMAC-SHA256). **Obligatoria en deploy real.** |
| `APP_SECURE_COOKIES` | `false` | Si es `true`/`1`, agrega el flag `Secure` a la cookie de sesión (requiere HTTPS/TLS). |

## `APP_SECRET_KEY`: por qué es obligatoria en producción

Los tokens CSRF se derivan con `HMAC-SHA256(APP_SECRET_KEY, session_token)`. Las **sesiones
persisten en SQLite** y sobreviven a un reinicio del server, pero la clave efímera **no**.

Si no configurás `APP_SECRET_KEY`:

1. Al arrancar, el server genera una clave aleatoria y emite un `WARN`.
2. Tras un reinicio, los usuarios siguen logueados (su cookie de sesión sigue siendo válida),
   pero el token CSRF que tienen en memoria ya no coincide con el que deriva la clave nueva.
3. **La primera mutación (guardar, eliminar, revisar) devuelve `403`** hasta que recargan la
   página, momento en que `GET /api/auth/me` les entrega un token CSRF fresco.

Para evitar ese fallo silencioso, configurá una clave **estable y secreta**:

```bash
# Generá una vez y guardala en el entorno (no la commitees):
export APP_SECRET_KEY="$(openssl rand -hex 32)"   # o: uuidgen
```

> Como salvaguarda, el server **se niega a arrancar** si `APP_SECURE_COOKIES=true` (señal de
> deploy con TLS) y `APP_SECRET_KEY` no está configurada.

## Ejemplo: LAN de laboratorio (sin TLS)

```bash
export APP_BIND_ADDR="0.0.0.0:8080"
export DATABASE_URL="sqlite:/var/lib/quantify/quantify.db"
export UPLOAD_DIR="/var/lib/quantify/uploads"
export APP_SECRET_KEY="$(openssl rand -hex 32)"   # generala una vez y dejala fija
export APP_SECURE_COOKIES="false"                 # HTTP plano dentro de la LAN
./quantify
```

Las PCs cliente acceden a `http://<ip-del-server>:8080`.

## Ejemplo: detrás de un reverse proxy con TLS

Si terminás TLS en nginx/Caddy y servís por HTTPS:

```bash
export APP_BIND_ADDR="127.0.0.1:8080"   # solo accesible por el proxy
export APP_SECRET_KEY="$(openssl rand -hex 32)"
export APP_SECURE_COOKIES="true"        # la cookie solo viaja por HTTPS
./quantify
```

## Capas de seguridad activas

- **CSRF**: token HMAC-SHA256 derivado de la sesión, validado en todo `POST/PUT/PATCH/DELETE`
  (excepto `login` y `logout`). El frontend lo manda en el header `X-CSRF-Token`.
- **Rate-limiting de login**: 5 intentos fallidos consecutivos por email bloquean ese email
  durante 15 minutos. El contador se resetea con un login exitoso o al expirar el bloqueo.
- **Cookies**: `HttpOnly` + `SameSite=Lax` siempre; `Secure` cuando `APP_SECURE_COOKIES=true`.
- **Contraseñas**: hash Argon2.
