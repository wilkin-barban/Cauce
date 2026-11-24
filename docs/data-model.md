# Cauce — Modelo de datos y persistencia

**Estado:** normativo para v0.1  
**Motor:** SQLite 3 con WAL  
**Ubicación:** `$XDG_DATA_HOME/cauce/cauce.db`  
**Relacionado:** [`contracts.md`](contracts.md), [`architecture.md`](architecture.md)

## 1. Reglas de persistencia

- El daemon es el único escritor de la base.
- La GUI y la CLI consultan por IPC, nunca abren el archivo SQLite.
- Toda migración es transaccional e idempotente.
- Antes de una migración se crea un backup recuperable.
- `foreign_keys=ON`, `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=5000`.
- Los secretos no se guardan en la base.
- Las rutas se guardan canonicalizadas; los nombres visibles se derivan de ellas.
- Los JSON internos (`probe`, `plan`, `settings`) se validan al escribir y al leer.

## 2. Esquema lógico

### 2.1 `schema_version`

| Columna | Tipo | Regla |
|---|---|---|
| `version` | INTEGER PK | Versión monotónica |
| `applied_at` | TEXT | RFC 3339 UTC |
| `app_version` | TEXT | Versión que aplicó la migración |

Solo existe una fila con la versión actual. La migración debe fallar completa si no puede escribir el esquema y la versión.

### 2.2 `jobs`

| Columna | Tipo | Regla |
|---|---|---|
| `id` | TEXT PK | UUIDv7 |
| `input_path` | TEXT NOT NULL | Ruta absoluta canonicalizada |
| `output_path` | TEXT NOT NULL | Ruta absoluta final |
| `temp_path` | TEXT | Único mientras corre |
| `preset_id` | TEXT NOT NULL | FK lógica al catálogo/presets |
| `state` | TEXT NOT NULL | Estado de [`contracts.md`](contracts.md#4-máquina-de-estados) |
| `priority` | INTEGER NOT NULL DEFAULT 0 | `-1000..1000` |
| `progress_json` | TEXT | `Progress` serializado |
| `probe_json` | TEXT | `MediaSnapshot` serializado |
| `plan_json` | TEXT | `ConversionPlan` congelado |
| `error_code` | TEXT | Código estable |
| `error_message` | TEXT | Mensaje humano, sin trazas sensibles |
| `created_at` | TEXT NOT NULL | UTC |
| `updated_at` | TEXT NOT NULL | UTC |
| `started_at` | TEXT | UTC |
| `finished_at` | TEXT | UTC |

Restricciones:

- `output_path != input_path`.
- `state` pertenece al enum permitido.
- Un trabajo `completed` debe tener `finished_at` y `output_path`.
- Un trabajo `failed` debe tener `error_code`.

Índices: `(state, priority, created_at)`, `(created_at)`, `(input_path)`.

### 2.3 `job_events`

| Columna | Tipo | Regla |
|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | Orden monotónico |
| `job_id` | TEXT NOT NULL | FK a `jobs.id` |
| `from_state` | TEXT | Null en creación |
| `to_state` | TEXT NOT NULL | Estado nuevo |
| `reason` | TEXT | `user`, `scheduler`, `error`, `recovery`, etc. |
| `created_at` | TEXT NOT NULL | UTC |

Cada cambio de `jobs.state` se escribe en la misma transacción que el evento.

### 2.4 `presets`

| Columna | Tipo | Regla |
|---|---|---|
| `id` | TEXT PK | ID estable |
| `kind` | TEXT NOT NULL | `system` o `custom` |
| `name_es` | TEXT NOT NULL | Nombre localizado |
| `name_en` | TEXT NOT NULL | Nombre localizado |
| `definition_json` | TEXT NOT NULL | Definición validada |
| `created_at` | TEXT NOT NULL | UTC |
| `updated_at` | TEXT NOT NULL | UTC |

Los presets del sistema se siembran desde el catálogo canónico y no se editan. El único preset personalizado de v0.1 vive en `settings` bajo la clave `custom_preset` (§2.5): una sola fila que se reemplaza al guardar, con `id` fijo `custom-1` y `kind` lógico `custom` según [`contracts.md`](contracts.md) §2.3. La tabla `presets` sigue aceptando `kind = 'custom'` para cuando los personalizados múltiples lleguen en una fase posterior.

### 2.5 `settings`

| Columna | Tipo | Regla |
|---|---|---|
| `key` | TEXT PK | Clave estable |
| `value_json` | TEXT NOT NULL | Valor validado |
| `updated_at` | TEXT NOT NULL | UTC |

Claves iniciales: `output_dir`, `max_concurrent_jobs`, `theme`, `language`, `notifications`, `duplicate_policy`, `keep_temp_on_failure`.

Clave `custom_preset`: el `CustomPreset` serializado ([`contracts.md`](contracts.md) §2.3). Se valida al escribir —una combinación inválida se rechaza con `INVALID_INPUT` y no deja fila— y al leer: un JSON roto o una combinación inválida se ignora con una advertencia en el log, y `settings_get` sigue funcionando porque la clave no forma parte de `Settings`. Guardar reemplaza el valor; `presets.clear_custom` borra la fila.

### 2.6 `output_reservations`

| Columna | Tipo | Regla |
|---|---|---|
| `path` | TEXT PK | Ruta final canonicalizada |
| `job_id` | TEXT NOT NULL | FK a `jobs.id` |
| `created_at` | TEXT NOT NULL | UTC |

Se crea dentro de la misma transacción que el trabajo. Se elimina cuando el trabajo se publica, falla de forma terminal o se elimina. Impide que dos trabajos activos elijan el mismo destino.

### 2.7 Tablas reservadas para v0.2

No se implementan en v0.1, pero el diseño deja espacio para:

- `watchers`: reglas de carpeta, preset, destino y post-acción.
- `watcher_processed`: deduplicación por ruta/inodo/hash.
- `watcher_events`: auditoría de detecciones.

## 3. Transacciones

### Alta de trabajo

1. Validar rutas, preset y plan.
2. Insertar `jobs` en `queued`.
3. Reservar `output_path` en `output_reservations`.
4. Insertar `job_events` con `from_state = null`, `to_state = queued`.
5. Commit.

Si cualquier paso falla, no queda trabajo ni reserva.

### Transición de estado

1. Leer estado actual.
2. Validar la transición con la máquina de estados.
3. Actualizar `jobs`.
4. Insertar `job_events`.
5. Commit.

No se permite una transición sin evento.

### Progreso

- Actualización de `progress_json` como máximo una vez por segundo.
- No se crea un `job_event` por cada muestra de progreso.
- Los eventos de progreso se emiten a la GUI a un máximo de 2 Hz.

### Publicación de salida

1. Validar el temporal.
2. Confirmar que no cambió el destino.
3. Renombrar de forma atómica.
4. Marcar `completed`, liberar reserva y registrar evento.
5. Si falla, conservar el temporal según ajuste y marcar `failed`.

## 4. Recuperación

Al arrancar el daemon:

1. Leer `schema_version`; migrar si hace falta.
2. Buscar trabajos en `probing`, `running`, `paused`, `validating` o `cancelling`.
   `cancelling` entra en la barrida porque cuenta como activo para el
   planificador y `jobs.remove` lo rechaza: si el daemon muere a mitad de una
   cancelación, sin esta regla la fila queda inmortal y la cola no vuelve a
   arrancar nunca.
3. Marcarlos como `interrupted` y registrar `job_events` con `reason = recovery`.
4. Liberar las reservas de trabajos que ya no están activos.
5. No borrar automáticamente temporales; dejarlos para inspección o limpieza manual.
6. Mantener `queued` para que el scheduler los tome.

La UI recibe los `interrupted` y ofrece “Reencolar”. No se reanuda automáticamente un trabajo interrumpido.

## 5. Retención y mantenimiento

- Los trabajos completados/cancelados permanecen hasta que el usuario los elimine.
- Los eventos se conservan con el trabajo.
- Los logs rotan fuera de SQLite y no bloquean la base.
- Una limpieza de trabajos elimina reservas asociadas y eventos en una transacción.
- La base se puede exportar/backup con el daemon detenido; no se copia un WAL activo a mano.

## 6. Migraciones

- `schema_version` empieza en `1`.
- Cada migración tiene un identificador y una transacción.
- Una migración fallida deja la base en la versión anterior.
- Antes de migrar se escribe un backup con timestamp en `$XDG_DATA_HOME/cauce/backups/`.
- Las migraciones no dependen de la red ni de FFmpeg.
- Si la versión de la base es mayor que la soportada, el daemon no abre y reporta `DATABASE_NEWER_THAN_APP`.

## 7. Invariantes de integridad

- No hay dos reservas activas para la misma ruta final.
- No hay trabajos activos sin una entrada existente.
- No hay trabajos `completed` sin salida validada.
- No hay eventos huérfanos.
- No hay presets personalizados con IDs de sistema.
- No se guardan tokens, contraseñas ni contenido de medios.

## 8. Pruebas obligatorias

- Migración desde base vacía y desde versión anterior.
- Fallo de migración con rollback.
- Alta concurrente de dos trabajos con la misma salida: solo uno reserva.
- Transición inválida rechazada sin escribir evento.
- Reinicio con trabajos activos: todos pasan a `interrupted`.
- Publicación de salida con destino existente y política `skip`/`suffix`/`overwrite`.
- Backup y restauración de una base.
- Base con `schema_version` futura se rechaza de forma segura.
