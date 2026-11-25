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
| `audio_tracks` | `AudioTrackPlan[]` | Opcional. Vacío en un plan escrito por 0.1.x; se omite del JSON cuando está vacío |
| `subtitle_tracks` | `SubtitleTrackPlan[]` | Opcional. Una pista incrustada vive aquí sin `output_index` |

`VideoPlan`: `action` (`copy`/`transcode`/`remove`), `encoder?`, `quality?`, `max_width?`, `max_height?`, `fps?`, `pixel_format?`, `profile?`, `level?`, `hdr_action` (`preserve`/`tone_map_sdr`), `stream_index?`, `tone_mapping?`.

`tone_mapping?` elige la curva (`hable`/`mobius`/`reinhard`) cuando `hdr_action` mapea. Ausente significa `hable`, y se omite del JSON cuando falta, así que un plan de 0.1.x serializa igual que antes.

`AudioPlan`: `action` (`copy`/`transcode`/`remove`), `encoder?`, `bitrate?`, `channels?`, `stream_index?`, `normalize?`, `filter?`.

`filter?` es un enumerado cerrado, nunca texto libre: `stereo_dialogue_boost` pide la matriz de downmix con realce del canal central que corrige el «audio mudo» de v0.1 ([`architecture-phases-0-and-1.md`](architecture-phases-0-and-1.md) §2 BUG-001), más el limitador `alimiter=limit=0.98` que evita el clipping de fase 1 (§3.2). Se omite del JSON cuando no hay filtro, así que un plan persistido por v0.1 se sigue leyendo byte a byte igual.

`SubtitlePlan`: `action` (`copy`/`transcode`/`remove`/`burn_in`), `encoder?`, `stream_index?`, `source_index?`.

`AudioTrackPlan`: `source_index`, `output_index`, `action`, `encoder?`, `bitrate?`, `channels`, `filter?`, `language?`, `title?`, `is_default`. `SubtitleTrackPlan`: igual pero `output_index?` (
`null` para una pista incrustada) y `action` con los valores de `SubtitlePlan`.

Un plan que no declara pistas se lee con `primary_audio()` / `primary_subtitles()`, que derivan la vista de un solo stream a partir de los campos `audio` y `subtitles`. Esa vista es la que mantiene funcionando los planes persistidos por 0.1.x sin migración de datos.

#### Selección de pistas y opciones personalizadas (v0.2)

Ambos son **opcionales** en `media.plan` y `jobs.add`: si el campo entero falta, el daemon aplica las reglas del preset, que es el comportamiento de v0.1 ([`architecture-phases-0-and-1.md`](architecture-phases-0-and-1.md) §3.5).

`TrackSelection`: `audio_track_indices` (u32[]), `subtitle_track_index?`, `subtitle_mode`, `audio_mix_mode`.

| Regla | Detalle |
|---|---|
| Orden | `audio_track_indices` es orden de inclusión en la salida, no orden de pista de origen |
| Lista vacía | Solo legal con `audio_mix_mode: passthrough`; con cualquier otro modo es un downmix sin fuente |
| Duplicados | Prohibidos: dos entradas iguales producirían dos `-map` de la misma pista |
| Tope | Cuatro pistas; el modo dual emite dos y el benchmark necesita una |

`subtitle_mode`: `soft_embed`, `burn_in`, `strip`. `audio_mix_mode`: `passthrough`, `stereo_dialogue_boost`, `dual_track_universal`. No existe `external_file` ni un booleano `dual_track_audio`: el enum es la única fuente de verdad (decisión registrada en [`odd/tasks/cauce-v0.2.0-fase1-motor-multimedia.md`](../odd/tasks/cauce-v0.2.0-fase1-motor-multimedia.md) L3).

`CustomConversionOptions`: `resolution`, `crf?`, `video_bitrate_kbps?`, `video_codec?`, `audio_codec?`, `audio_bitrate_kbps?`, `tone_mapping`. Todos los campos tienen valor por defecto, así que `{"resolution": "1080p", "crf": 20}` es un payload válido.

| Campo | Valores | Regla |
|---|---|---|
| `resolution` | `"original"`, `"1080p"` o la forma etiquetada `{"fixed": {"width", "height"}}` / `{"max_height"}` | El atajo `"<alto>p"` es azúcar de `max_height`; una dimensión `0` se rechaza |
| `crf` | entero `0-51` | Fuera de rango es error de contrato, no un valor a recortar |
| `video_bitrate_kbps` / `audio_bitrate_kbps` | entero `> 0` | `0` es un bitrate imposible |
| `tone_mapping` | `hable`, `mobius`, `reinhard` | Elige la curva; **si** se mapea sigue siendo decisión de `HdrAction` |

### 2.3 `Preset`

Un preset contiene identidad, presentación, restricciones y preferencias. El catálogo canónico está en [`presets.md`](presets.md).

Campos comunes: `id`, `name_es`, `name_en`, `description_es`, `description_en`, `category`, `container`, `video_codec`, `audio_codec`, `video_quality`, `audio_bitrate`, `max_width`, `max_height`, `max_fps`, `pixel_format`, `profile`, `level`, `max_bit_depth`, `hdr_policy`, `subtitle_policy`, `faststart`, `preserve_metadata`, `hardware_preference`, `audio_channels`, `normalize_audio`, `preferred_audio_language`.

#### `CustomPreset`

El único preset propio de v0.1. Se guarda como JSON bajo la clave `custom_preset` de `settings` ([`data-model.md`](data-model.md) §2.5) y se convierte en `Preset` con `id` fijo `custom-1`, `category` `custom` y nombres «Personalizado» / «Custom» / «Personalizado», de modo que `media.plan`, `jobs.add` y el ejecutor no necesitan ningún caso especial. **Guardar reemplaza**: nunca hay más de uno, y no existe un editor genérico.

| Campo | Valores | Regla |
|---|---|---|
| `container` | `mp4`, `mkv`, `webm` | Debe transportar el video y el audio elegidos |
| `video` | `h264`, `h265`, `vp9`, `av1`, `copy`, `none` | `none` elimina la pista de video |
| `quality` | CRF entero `0-51` o `high`, `medium`, `low` | Un nivel se resuelve a CRF por codificador (H.264 18/23/28, H.265 20/26/30, VP9 24/32/38, AV1 24/30/36); `copy` y `none` no declaran CRF |
| `resolution` | `original`, `1080p`, `720p`, `480p` | Límite máximo; `original` no limita |
| `audio` | `aac`, `mp3`, `opus`, `flac`, `copy`, `none` | `none` elimina la pista de audio |
| `bitrate` | `128`, `192`, `256`, `320` (kbps) o `null` | Prohibido con `audio: copy` y `audio: none`; ausente en un recodificado significa `192` |
| `subtitles` | `keep`, `remove` | `keep` exige pista de video y de audio |

Combinaciones rechazadas con `INVALID_INPUT` (no reintentable), todas probadas: `mp4` con `vp9`; `webm` con `h264` o `h265`; `webm` con `aac`, `mp3` o `flac`; `bitrate` informado con `audio: copy` o `audio: none`; `subtitles: keep` con `video: none` (sin imagen no hay a dónde asociarlos) o con `audio: none` (el resultado sería solo texto). Un valor de enum desconocido y un CRF fuera de `0-51` se rechazan al deserializar. Un valor guardado que ya no se pueda usar se ignora al leer, con una advertencia en el log: `presets.list` sigue respondiendo los trece del sistema.

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

`output_dir`, `max_concurrent_jobs` (1–8), `theme` (`system`/`light`/`dark`), `language` (`es`/`en`/`pt-BR`), `notifications` (default `false`), `duplicate_policy` (`suffix`/`skip`/`overwrite`), `keep_temp_on_failure`.

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
| `cancelling` | `cancelled`, `failed`, `interrupted` |
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
- `interrupted` solo aparece tras un reinicio del daemon; `retry` nunca lo produce.
- `recover` (método `jobs.recover`, §6) solo es válido desde `interrupted` y lo devuelve a `queued` con un evento de motivo `recovery`. No reanuda nada por sí solo: el trabajo vuelve a la cola y el planificador lo toma cuando la cola está activa.
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
| `jobs.recover` | `id` | `Job` |
| `jobs.remove` | `id` | `{removed: bool}` |
| `jobs.clear_finished` | `{}` | `{removed: N}` |
| `queue.pause` / `queue.resume` / `queue.start` | `{}` | `{paused: bool}` |
| `presets.list` | `{}` | `Preset[]` (los trece del sistema y, si existe, `custom-1` al final) |
| `presets.set_custom` | `CustomPreset` (§2.3) | `Preset` (el `custom-1` resultante) |
| `presets.clear_custom` | `{}` | `{cleared: bool}` |
| `settings.get` | `{}` | `Settings` |
| `settings.set` | `Partial<Settings>` | `Settings` |
| `system.capabilities` | `{}` | `Capabilities` |
| `logs.list` | `limit?` | `LogEntry[]` |

`queue.start` es un **alias explícito de `queue.resume`**: activa la cola y deja que el planificador tome el próximo trabajo `queued` en su siguiente tick. Garantías:

- Nunca duplica un trabajo.
- No cambia el estado de los trabajos `probing`, `running`, `paused`, `validating` ni `cancelling`.
- Sobre una cola que ya está activa es una operación nula: responde `{"paused": false}`.
- Emite un solo `queue.updated` por llamada.

`jobs.recover` es la acción de usuario que reencola un trabajo `interrupted`: aplica `JobAction::Recover`, lo lleva a `queued` y emite un `job.updated` con el `Job` completo. Es inválido desde cualquier otro estado y responde `INVALID_INPUT` (no reintentable); no reanuda nada automáticamente, y `interrupted` no es alcanzable por `jobs.retry`.

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
