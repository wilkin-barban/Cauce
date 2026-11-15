# Cauce — Contratos de dominio e IPC

**Estado:** normativo, sin implementación  
**Versión:** 0.1  
**Ámbito:** `cauce-core`, `cauce-daemon`, `cauce-gui` y `cauce-cli`

## 1. Principios

- El daemon es la única fuente de verdad del estado.
- La GUI no ejecuta FFmpeg ni accede a SQLite.
- Ningún contrato depende de una ruta de Windows ni de Electron.
- Los datos serializados son versionados y validados antes de persistirse.
- Un error debe tener código estable, mensaje humano y posibilidad de reintento explícita.

## 2. Tipos de dominio

### 2.1 `MediaSnapshot`

Representa el resultado de `ffprobe` para un archivo:

| Campo | Tipo | Regla |
|---|---|---|
| `path` | string | Ruta absoluta y canonicalizada |
| `container` | string | Contenedor normalizado (`mp4`, `mkv`, `webm`, etc.) |
| `duration_ms` | integer | `>= 0`; 0 si FFprobe no puede determinarla |
| `size_bytes` | integer | `>= 0` |
| `bitrate_bps` | integer | `>= 0`; 0 si se desconoce |
| `video_streams` | `VideoStream[]` | Puede estar vacío |
| `audio_streams` | `AudioStream[]` | Puede estar vacío |
| `subtitle_streams` | `SubtitleStream[]` | Puede estar vacío |
| `probed_at` | timestamp RFC 3339 | UTC |

`VideoStream`: `index`, `codec`, `profile?`, `level?`, `width`, `height`, `fps`, `pixel_format`, `bit_depth`, `is_hdr`, `bitrate_bps?`, `language?`.

`AudioStream`: `index`, `codec`, `channels`, `sample_rate`, `bitrate_bps?`, `language?`, `is_default`.

`SubtitleStream`: `index`, `codec`, `language?`, `kind` (`text`/`bitmap`), `is_default`, `is_forced`.

### 2.2 `ConversionPlan`

| Campo | Tipo | Regla |
|---|---|---|
| `container` | string | Igual al destino del preset |
| `video` | `VideoPlan` | Acción obligatoria |
| `audio` | `AudioPlan` | Acción obligatoria |
| `subtitles` | `SubtitlePlan` | Acción obligatoria |
| `faststart` | boolean | Solo aplica a MP4/M4A |
| `metadata` | `preserve`/`strip` | Política explícita |
| `reasoning` | string[] | Explica copy/transcode para la UI |

`VideoPlan`: `action` (`copy`/`transcode`/`remove`), `encoder?`, `quality?`, `max_width?`, `max_height?`, `fps?`, `pixel_format?`, `profile?`, `level?`, `hdr_action` (`preserve`/`tone_map_sdr`), `stream_index?`.

`AudioPlan`: `action` (`copy`/`transcode`/`remove`), `encoder?`, `bitrate?`, `channels?`, `stream_index?`, `normalize?`.

`SubtitlePlan`: `action` (`copy`/`transcode`/`remove`/`burn_in`), `encoder?`, `stream_index?`, `source_index?`.

### 2.3 `Preset`

Un preset contiene identidad, presentación, restricciones y preferencias. El catálogo canónico está en [`presets.md`](presets.md).

Campos comunes: `id`, `name_es`, `name_en`, `description_es`, `description_en`, `category`, `container`, `video_codec`, `audio_codec`, `video_quality`, `audio_bitrate`, `max_width`, `max_height`, `max_fps`, `pixel_format`, `profile`, `level`, `max_bit_depth`, `hdr_policy`, `subtitle_policy`, `faststart`, `preserve_metadata`, `hardware_preference`, `audio_channels`, `normalize_audio`, `preferred_audio_language`.

### 2.4 `Job`

| Campo | Tipo | Regla |
|---|---|---|
| `id` | UUID | UUIDv7 generado por el daemon |
| `input_path` | string | Absoluta, existente y autorizada |
| `output_path` | string | Absoluta, calculada por el daemon |
| `temp_path` | string? | Único mientras corre |
| `preset_id` | string | Preset existente |
| `state` | enum | Ver §4 |
| `priority` | integer | `-1000..1000`; por defecto 0 |
| `progress` | `Progress` | Ver §2.5 |
| `probe` | `MediaSnapshot?` | Snapshot de entrada |
| `plan` | `ConversionPlan?` | Plan congelado al iniciar |
| `error` | `JobError?` | Código y mensaje |
| `created_at`, `updated_at` | timestamp | UTC |
| `started_at`, `finished_at` | timestamp? | UTC |

### 2.5 `Progress`

`percent` (0–100), `processed_ms`, `total_ms`, `fps`, `speed`, `bitrate_bps`, `eta_seconds`, `updated_at`. `percent` puede ser null cuando no hay duración conocida; la UI muestra modo indeterminado.

### 2.6 `Settings`

`output_dir`, `max_concurrent_jobs` (1–8), `theme` (`system`/`light`/`dark`), `language` (`es`/`en`), `notifications`, `duplicate_policy` (`suffix`/`skip`/`overwrite`), `keep_temp_on_failure`.

## 3. Invariantes

- Un trabajo no puede apuntar a una entrada no autorizada.
- `output_path` nunca puede ser igual a `input_path`.
- Un plan `copy` no puede declarar un encoder.
- Un plan `remove` no puede declarar un stream.
- Un trabajo `completed` debe tener salida validada y publicada.
- Un trabajo `failed` conserva el error y no publica un archivo inválido.
- `job_events` registra cada cambio de estado; no se actualiza un estado sin evento.
- Un plan persistido no se recalcula durante la ejecución.
- El daemon no acepta flags FFmpeg arbitrarios en v0.1.

## 4. Máquina de estados

| Estado | Permitidos |
|---|---|
| `queued` | `probing`, `cancelling` |
| `probing` | `running`, `failed`, `cancelling` |
| `running` | `paused`, `validating`, `cancelling`, `failed`, `interrupted` |
| `paused` | `running`, `cancelling` |
| `cancelling` | `cancelled`, `failed` |
| `validating` | `completed`, `failed`, `interrupted` |
| `completed` | terminal |
| `cancelled` | terminal |
| `failed` | `queued` por reintento explícito |
| `interrupted` | `queued` por recuperación |

Reglas:

- `pause` solo es válido desde `running`.
- `resume` solo es válido desde `paused`.
- `cancel` es válido desde `queued`, `probing`, `running` o `paused`.
- Un reinicio convierte `probing`/`running`/`validating` en `interrupted`.
- Un trabajo `interrupted` no se reencola automáticamente: la UI debe ofrecerlo.
- La cola pausada no inicia nuevos trabajos, pero permite cancelar los activos.

## 5. Transporte IPC

- Socket: `$XDG_RUNTIME_DIR/cauce/daemon.sock`.
- Permisos: `0600`; propietario: usuario actual.
- Protocolo: JSON-RPC 2.0 sobre NDJSON.
- Límite: 1 MiB por mensaje.
- Autenticación local: `SO_PEERCRED`; misma UID.
- Los mensajes inválidos cierran la conexión.
- Cada conexión declara `client_name` y `client_version` en `initialize`.

Envolvente de petición:

```json
{"jsonrpc":"2.0","id":"req-1","method":"jobs.list","params":{}}
```

Envolvente de error:

```json
{"jsonrpc":"2.0","id":"req-1","error":{"code":"PATH_DENIED","message":"La ruta no está autorizada.","data":{"retryable":false}}}
```

## 6. Métodos MVP

| Método | Params | Resultado |
|---|---|---|
| `initialize` | `client_name`, `client_version` | `server_version`, `protocol_version` |
| `media.inspect` | `paths[]` | `MediaSnapshot[]` |
| `media.plan` | `snapshot`, `preset_id` | `ConversionPlan` |
| `jobs.add` | `inputs[]`, `output_dir`, `preset_id`, `duplicate_policy?`, `priority?` | `Job[]` |
| `jobs.list` | `states?[]` | `Job[]` |
| `jobs.get` | `id` | `Job` |
| `jobs.pause` | `id` | `Job` |
| `jobs.resume` | `id` | `Job` |
| `jobs.cancel` | `id` | `Job` |
| `jobs.retry` | `id` | `Job` |
| `jobs.remove` | `id` | `{removed: bool}` |
| `queue.pause` / `queue.resume` | `{}` | `{paused: bool}` |
| `presets.list` | `{}` | `Preset[]` |
| `settings.get` | `{}` | `Settings` |
| `settings.set` | `Partial<Settings>` | `Settings` |
| `system.capabilities` | `{}` | `Capabilities` |
| `logs.list` | `limit?` | `LogEntry[]` |

`Capabilities`: `ffmpeg_version`, `ffprobe_version`, `encoders[]`, `filters[]`, `cpu_cores`, `disk_free_bytes`, `daemon_version`, `ffmpeg_missing[]`, `warnings[]`.

## 7. Eventos

| Evento | Payload |
|---|---|
| `job.updated` | `Job` completo |
| `job.progress` | `id`, `Progress` |
| `queue.updated` | `paused`, `running`, `queued` |
| `log.entry` | `LogEntry` |
| `daemon.status` | `state`, `version` |

Los eventos de progreso se emiten como máximo a **2 Hz**. El estado se persiste como máximo una vez por segundo, salvo una transición.

## 8. Errores estables

| Código | Significado | Reintentable |
|---|---|---|
| `INVALID_INPUT` | Ruta o argumento inválido | No |
| `PATH_DENIED` | Ruta fuera de una raíz autorizada | No |
| `FFMPEG_MISSING` | No se encontró ffmpeg/ffprobe | No |
| `FFMPEG_FEATURE_MISSING` | Falta encoder/filtro requerido | No |
| `PROBE_FAILED` | FFprobe no pudo analizar la entrada | No |
| `PLAN_INVALID` | Preset/plan incompatible | No |
| `OUTPUT_COLLISION` | Destino reservado o no escribible | Sí, con otra política |
| `OUTPUT_VALIDATION_FAILED` | La salida no cumple el contrato | Sí |
| `PROCESS_FAILED` | FFmpeg terminó con error | Sí |
| `CANCELLED` | Cancelación solicitada por el usuario | Sí |
| `DAEMON_UNAVAILABLE` | El socket/daemon no responde | Sí |
| `DATABASE_BUSY` | SQLite no pudo escribir | Sí, con backoff |
| `INTERNAL` | Error no clasificado | Depende |

La UI muestra el mensaje humano y, si aplica, una acción: reintentar, abrir diagnóstico o instalar FFmpeg.

## 9. Seguridad de archivos

- Canonicalizar rutas antes de validar.
- Rechazar rutas relativas, bytes nulos y escapes de raíz.
- La carpeta de salida debe existir, ser escribible y no ser la entrada.
- Los temporales viven en la carpeta de salida y llevan el ID del trabajo.
- No seguir symlinks que salgan de la raíz autorizada.
- No borrar el destino anterior hasta que la salida temporal pase validación.
- No aceptar `-i` adicional, URLs ni flags de shell desde presets.

## 10. Versionado

- `protocol_version` empieza en `1`.
- Añadir un método o un campo opcional no rompe compatibilidad.
- Eliminar o cambiar un campo requiere incrementar la versión y documentarlo.
- La GUI y la CLI deben mostrar un error explícito si el daemon habla una versión mayor.

## 11. Referencias

- [`architecture.md`](architecture.md) — arquitectura y límites de proceso.
- [`presets.md`](presets.md) — catálogo canónico.
- [`data-model.md`](data-model.md) — persistencia.
- [`ui-mvp.md`](ui-mvp.md) — flujo de usuario.
- [`backlog.md`](backlog.md) — items y puertas.
