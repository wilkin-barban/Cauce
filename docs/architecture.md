# Cauce — Especificación de arquitectura

**Estado:** borrador normativo, sin implementación  
**Versión:** 0.1  
**Fecha:** 2026-11-14  
**Alcance:** Ubuntu 26.04 LTS, arquitectura amd64  
**Nombre del producto:** Cauce  
**Licencia:** GPL-3.0-or-later

## 1. Resultado esperado

Cauce será un conversor multimedia nativo para Ubuntu, construido en Rust y GTK4/Libadwaita, que use el **FFmpeg del sistema** y mantenga la cola de conversión viva aunque se cierre la interfaz.

El usuario podrá añadir archivos o carpetas, inspeccionarlos, elegir un preset, enviarlos a una cola persistente y seguir el progreso real desde una GUI nativa. Un daemon de usuario será el dueño de los trabajos y del estado.

La primera entrega será un **MVP enfocado en convertir, encolar y aplicar presets**. Watchers, historial, presets personalizados y aceleración por hardware quedarán documentados en la matriz de paridad y se implementarán en fases posteriores.

## 2. Decisiones cerradas

| Decisión | Elección | Motivo |
|---|---|---|
| Nombre | **Cauce** | Identidad propia, corta y ligada a un flujo de conversión |
| Plataforma inicial | Ubuntu 26.04 amd64 | Evita deuda Windows/macOS y permite integrarse con GNOME |
| FFmpeg | Dependencia del sistema (`ffmpeg`/`ffprobe`) | Paquete pequeño, actualizaciones de seguridad y menor superficie de mantenimiento |
| Interfaz | GTK4 + Libadwaita | Nativa, coherente con GNOME y con la experiencia de Segmenta |
| Proceso | Daemon + GUI | Las conversiones y la cola sobreviven al cierre de la ventana |
| Persistencia | SQLite WAL | Recuperación, historial mínimo y estado duradero |
| IPC | JSON-RPC 2.0 sobre socket Unix | Contrato explícito, comprobable y sin shell |
| Presets MVP | Los 13 del sistema | Paridad inicial; personalizados en v0.2 |
| Flujo de GUI | Añadir a cola y luego iniciar | Revisión segura antes de ejecutar |
| CLI v0.1 | Solo interna | Diagnóstico y pruebas sin contrato público |
| Licencia | GPL-3.0-or-later (ratificada) | Coherente con Segmenta y con un ecosistema abierto |

## 3. Alcance del MVP v0.1

### Incluido

- Añadir archivos y carpetas mediante selector del portal o drag & drop.
- Inspeccionar medios con `ffprobe` y mostrar pistas, duración, resolución y códec.
- Seleccionar carpeta de salida y preset.
- Crear trabajos en una cola persistente.
- Iniciar, pausar, reanudar, cancelar, reintentar y eliminar trabajos.
- Limitar la concurrencia y pausar/reanudar la cola completa.
- Progreso real: porcentaje, tiempo procesado, FPS, velocidad, bitrate y ETA.
- Validación de salida antes de publicar el archivo final.
- Política de duplicados: `suffix`, `skip` y `overwrite`, sin sobrescribir nunca el archivo de entrada.
- Diagnóstico de FFmpeg, CPU, espacio en disco y estado del daemon.
- Logs básicos consultables y persistentes.
- Ajustes: carpeta de salida, concurrencia, tema, idioma y notificaciones.
- GUI con Convertir, Cola, Presets, Diagnóstico y Ajustes.
- `.deb` instalable con `ffmpeg` y dependencias GTK4 declaradas.
- CLI mínima para diagnóstico y pruebas de integración.

### Fuera del MVP

- Carpetas vigiladas.
- Historial visual completo.
- Presets personalizados y editor avanzado.
- Aceleración VAAPI/NVENC/QSV.
- Subtítulos incrustados y flujos HDR avanzados.
- Flags FFmpeg personalizados.
- Actualización automática.
- Windows y macOS.

La paridad completa y sus criterios de aceptación están en [`parity.md`](parity.md).

## 4. Modelo del sistema

```text
┌──────────────────────────────┐
│        cauce-gui             │
│  GTK4 + Libadwaita           │
│  Convertir · Cola · Presets  │
│  Diagnóstico · Ajustes       │
└──────────────┬───────────────┘
               │ JSON-RPC 2.0 / NDJSON
               │ $XDG_RUNTIME_DIR/cauce/daemon.sock (0600)
┌──────────────▼───────────────┐
│        cauce-daemon          │
│  scheduler · workers · IPC   │
└───────┬──────────────┬───────┘
        │              │
┌───────▼──────┐  ┌────▼────────────┐
│ cauce-jobs   │  │ cauce-ffmpeg    │
│ SQLite WAL   │  │ ffmpeg/ffprobe  │
│ cola/estados │  │ progreso/procesos│
└───────┬──────┘  └────┬────────────┘
        │              │
        └──────┬───────┘
               ▼
        ┌──────────────┐
        │ cauce-core   │
        │ presets      │
        │ compatibilidad│
        │ argumentos   │
        │ validación   │
        └──────────────┘
```

La GUI no abre SQLite ni ejecuta FFmpeg. El daemon no dibuja widgets. `cauce-core` no conoce GTK, sockets ni procesos.

## 5. Workspace Rust

```text
cauce/
├── Cargo.toml                 # workspace
├── crates/
│   ├── cauce-core/            # dominio puro
│   ├── cauce-ffmpeg/          # FFmpeg, FFprobe y progreso
│   ├── cauce-jobs/            # cola, SQLite y estados
│   ├── cauce-daemon/          # proceso headless + IPC
│   ├── cauce-gui/             # GTK4/Libadwaita
│   ├── cauce-cli/             # diagnóstico y automatización
│   └── cauce-i18n/            # catálogos es/en/pt-BR
├── packaging/
│   ├── cauce.desktop
│   ├── io.github.wilkin.Cauce.metainfo.xml
│   ├── cauce.socket
│   ├── cauce.service
│   ├── check-package.sh
│   └── icons/
├── docs/
├── tests/
│   └── fixtures/
└── README.md
```

Reglas de dependencia:

- `cauce-core` no depende de GTK, Tokio, SQLite ni procesos.
- `cauce-ffmpeg` depende de `cauce-core` y encapsula `std::process`/Tokio.
- `cauce-jobs` depende de `cauce-core` y SQLite.
- `cauce-daemon` compone `cauce-jobs`, `cauce-ffmpeg` y `cauce-core`.
- `cauce-gui` solo habla con el daemon y usa `cauce-core` para tipos compartidos.
- `cauce-cli` es un cliente del daemon y, en pruebas, puede usar la librería de jobs.

## 6. Flujo de un trabajo

1. La GUI obtiene una ruta autorizada mediante el selector del portal o drag & drop.
2. Envía `media.inspect` al daemon.
3. El daemon ejecuta `ffprobe` y devuelve un `MediaSnapshot`.
4. La GUI solicita `media.plan` con `snapshot` y `preset_id`.
5. `cauce-core` decide copiar/transcodificar y devuelve un `ConversionPlan`.
6. `jobs.add` persiste el trabajo y reserva la ruta de salida.
7. El scheduler toma trabajos `queued` según concurrencia y prioridad.
8. `cauce-ffmpeg` ejecuta FFmpeg con argumentos seguros y `-progress pipe:1`.
9. El progreso se persiste y se emite por el socket a 2 Hz como máximo.
10. Al terminar, `output.validate` inspecciona el temporal; si es válido, se renombra atómicamente.
11. El trabajo pasa a `completed` o `failed`; el evento queda en `job_events`.

## 7. Estados y cola

Estados del MVP:

```text
queued -> probing -> running -> validating -> completed
  |          |          |            |
  |          |          +-> paused -> running
  |          |          +-> cancelling -> cancelled
  |          +-> failed
  +-> cancelling -> cancelled

running -> interrupted  (reinicio/caída)
interrupted -> queued    (recuperación)
failed -> queued         (reintento)
```

Reglas:

- `completed`, `cancelled` y `failed` son terminales salvo un reintento explícito para `failed`.
- Una pausa envía `SIGSTOP` al grupo de procesos; reanudar envía `SIGCONT`.
- Cancelar envía `SIGTERM` al grupo y, tras un plazo, `SIGKILL`.
- El daemon nunca inicia más trabajos que `max_concurrent_jobs`.
- La cola completa puede pausarse sin perder el estado de cada trabajo.
- Al arrancar, los trabajos que estaban `running` pasan a `interrupted`; la GUI ofrece reencolarlos.
- Cada transición genera un registro en `job_events`.

Concurrencia del MVP:

- Valor inicial recomendado: **1** para CPU, configurable entre 1 y 8.
- No habrá concurrencia automática hasta la fase de hardware/optimización.
- Los trabajos de extracción o remux podrán compartir más adelante una política distinta.

## 8. Contratos de datos

### 8.1 `MediaSnapshot`

Campos mínimos: `path`, `container`, `duration_ms`, `size_bytes`, `bitrate_bps`, `video_streams[]`, `audio_streams[]`, `subtitle_streams[]`, `probed_at`.

Cada pista de video incluye índice, códec, perfil, nivel, ancho, alto, FPS, pixel format, bit depth y flags HDR. Audio incluye índice, códec, canales, sample rate, bitrate, idioma y default. Subtítulos incluyen índice, códec, idioma, tipo y flags default/forced.

### 8.2 `ConversionPlan`

Debe contener:

- Contenedor y política de metadatos.
- Acción de video: `copy`, `transcode` o `remove`; encoder, calidad, límites, pixel format, perfil, nivel, HDR.
- Acción de audio: `copy`, `transcode` o `remove`; encoder, bitrate, canales, normalización.
- Acción de subtítulos: `copy`, `transcode`, `remove` o `burn-in`.
- `faststart`, `reasoning[]` y argumentos adicionales solo si están en la allowlist.

El plan debe ser serializable y quedar guardado en el trabajo para que la ejecución sea reproducible.

### 8.3 `Job`

Campos: `id`, `input_path`, `output_path`, `temp_path`, `preset_id`, `state`, `priority`, `progress`, `probe_json`, `plan_json`, `error_code`, `error_message`, `created_at`, `started_at`, `completed_at`.

### 8.4 Esquema SQLite

| Tabla | Uso |
|---|---|
| `schema_version` | Versión de migración |
| `jobs` | Trabajo, plan, snapshot, estado y fechas |
| `job_events` | Transiciones auditables |
| `presets` | Presets del sistema y, después, personalizados |
| `settings` | Clave/valor JSON validado |
| `output_reservations` | Evita colisiones entre trabajos activos |
| `watchers` | Se añade en la fase de paridad |
| `watcher_processed` | Se añade en la fase de paridad |

Configuración: WAL, `foreign_keys=ON`, `busy_timeout`, migraciones transaccionales y `user_version`.

## 9. Contrato IPC

Socket: `$XDG_RUNTIME_DIR/cauce/daemon.sock`, permisos `0600`, propietario del usuario. JSON-RPC 2.0 en NDJSON, máximo 1 MiB por mensaje, verificación `SO_PEERCRED` y cierre de conexiones no autorizadas.

Métodos MVP:

| Método | Resultado |
|---|---|
| `media.inspect` | Lista de `MediaSnapshot` |
| `media.plan` | `ConversionPlan` |
| `jobs.add` | Trabajos creados |
| `jobs.list` | Trabajos y estado |
| `jobs.pause` / `jobs.resume` | Transición aplicada |
| `jobs.cancel` | Cancelación solicitada |
| `jobs.retry` | Trabajo reencolado |
| `jobs.remove` | Trabajo eliminado |
| `queue.pause` / `queue.resume` | Estado de cola |
| `presets.list` | Presets disponibles |
| `settings.get` / `settings.set` | Configuración validada |
| `system.capabilities` | FFmpeg, CPU, disco y daemon |
| `logs.list` | Entradas recientes |

Eventos: `job.updated`, `job.progress`, `queue.updated`, `log.entry`, `daemon.status`.

La GUI no envía rutas que no haya seleccionado el usuario. El daemon valida existencia, tipo, permisos, canonicalización y pertenencia a una raíz autorizada.

## 10. FFmpeg del sistema

- Dependencia del paquete: `ffmpeg`.
- Binarios: `ffmpeg` y `ffprobe` desde `PATH`.
- Línea base soportada: Ubuntu 26.04 con FFmpeg 8.0.x.
- Comprobación al arrancar: versión, `ffprobe`, encoders requeridos y filtros usados por el MVP.
- Si falta FFmpeg o una capacidad, la GUI muestra el diagnóstico y una acción concreta; no se inicia el trabajo.
- No hay binarios FFmpeg dentro del `.deb` en el MVP.
- Todo proceso se ejecuta con argv, nunca con shell.

Argumentos base del MVP:

```text
-progress pipe:1 -nostats -i <input>
-map <streams> -c:v <encoder|copy> -c:a <encoder|copy>
<filtros permitidos> -movflags +faststart <temp-output>
```

El archivo final nunca se pasa directamente a FFmpeg. Se escribe en un temporal único de la carpeta de salida y solo se renombra tras validar.

## 11. Presets y compatibilidad

- Se portan los 13 presets del sistema de FF-Converter en la primera fase.
- El catálogo del sistema es inmutable y vive en el binario como datos versionados.
- Un preset personalizado se almacena como JSON validado; el editor queda fuera del MVP.
- `cauce-core` decide `copy` o `transcode` por separado para video, audio y subtítulos.
- El agente debe poder explicar por qué recodifica; `reasoning[]` se muestra en la UI.
- Las combinaciones codec/contenedor/subtítulos inválidas se rechazan antes de ejecutar FFmpeg.
- En el MVP solo se permiten encoders CPU: `libx264`, `libx265`, `libsvtav1`, `libvpx-vp9`, `aac`, `libmp3lame`, `flac`, `libopus`.
- Los presets con preferencia GPU se resuelven a CPU y se informa que la aceleración es una fase posterior.

## 12. Validación de salida

Antes de publicar:

1. El temporal existe y no está vacío.
2. FFprobe lo abre correctamente.
3. La duración está dentro de una tolerancia (mínimo 2 s o 3 %).
4. Están los streams que el plan declaró.
5. El códec de video/audio coincide con el esperado.
6. La resolución no supera los límites del preset.
7. El destino no cambió durante la conversión.
8. El rename es atómico y respeta la política de duplicados.
9. Un fallo conserva el temporal si `keep_temp_on_failure` está activo y nunca borra el destino previo.

## 13. GUI MVP

Ventana principal con `AdwNavigationSplitView`:

| Página | MVP |
|---|---|
| Convertir | Drop zone, archivos, carpeta de salida, preset y "Añadir a la cola" |
| Cola | Trabajos, progreso, FPS, velocidad, ETA, pausa/cancelar/reintentar |
| Presets | Catálogo del sistema y explicación del plan |
| Diagnóstico | FFmpeg, versión, encoders, CPU, disco y daemon |
| Ajustes | Carpeta, concurrencia, tema, idioma y notificaciones |

La GUI usa el portal para archivos y carpetas, acepta drag & drop y muestra un estado vacío distinto de un error. No incluye páginas de Watchers ni Historial en v0.1; la documentación de paridad las define.

## 14. Logs y observabilidad

- `tracing` como fachada de logs.
- Archivos en `$XDG_STATE_HOME/cauce/logs/`, rotación por tamaño y retención.
- Eventos de sistema, FFmpeg, cola y validación.
- No se registran cabeceras, tokens, credenciales ni rutas completas sin redacción.
- La UI puede filtrar por nivel y origen; exportar logs queda para una fase posterior.
- Cada transición de trabajo es auditable en SQLite.

## 15. Seguridad

- Sin shell; argumentos en array y allowlist estricta.
- Rutas seleccionadas por el usuario y canonicalizadas; rechazo de rutas relativas, nulas y escapes.
- Comprobación de que la salida no sea la entrada.
- Socket `0600`; `SO_PEERCRED`; límite de payload.
- SQLite sin secretos.
- Temporales con permisos del usuario y limpieza controlada.
- No seguir enlaces simbólicos fuera de las raíces autorizadas.
- No aceptar flags personalizados arbitrarios en el MVP.

## 16. Empaquetado y despliegue

El `.deb` incluirá:

- `/usr/bin/cauce-daemon`
- `/usr/bin/cauce-gui`
- `/usr/bin/cauce-cli`
- `/usr/lib/systemd/user/cauce.socket`
- `/usr/lib/systemd/user/cauce.service`
- `.desktop`, AppStream, icono y documentación de licencias.

Dependencias declaradas:

```text
ffmpeg
libgtk-4-1
libadwaita-1-0
libnotify4
xdg-desktop-portal
```

`ffmpeg` instala FFprobe y las bibliotecas de códecs necesarias. El usuario habilita `cauce.socket`; la GUI puede activarlo automáticamente mediante systemd.

El paquete se construye con `cargo-deb` y pasa una puerta de empaquetado que valida binarios, unidades, metadatos, iconos, permisos y dependencias. CI instala el `.deb` en un contenedor limpio de Ubuntu 26.04.

## 17. Pruebas y CI

### Unitarias

- Parser de FFprobe.
- Compatibilidad video/audio/subtítulos.
- Resolución de presets.
- Command builder dorado (argv exacto).
- Máquina de estados y validación de transiciones.
- Política de duplicados y colisiones.
- Validación de salida.
- Cálculo de ETA con muestras.

### Integración

- Daemon + SQLite en directorio temporal.
- IPC con socket temporal y `SO_PEERCRED`.
- Conversión real de fixtures generados con `ffmpeg` (testsrc/sine).
- Pausa, reanudación, cancelación y recuperación.
- Watchers cuando llegue su fase.

### GUI y paquete

- Smoke de la GUI con `xvfb-run`.
- Instalación del `.deb` en Ubuntu 26.04 limpio.
- Comprobación de FFmpeg del sistema y arranque del daemon.
- `cargo fmt --check`, `cargo clippy -D warnings`, tests y build release.

## 18. Riesgos y mitigaciones

| Riesgo | Mitigación |
|---|---|
| Diferencias entre builds de FFmpeg | Validar versión/encoders al arrancar y probar en Ubuntu 26.04 |
| VAAPI no funciona solo con `-hwaccel vaapi` | Posponer hardware; diseñar después con dispositivo, hwupload y pruebas reales |
| Procesos huérfanos | Grupo de procesos, SIGSTOP/SIGCONT y SIGTERM/SIGKILL controlados |
| Colisiones de salida | Reserva en SQLite + temporal único + rename atómico |
| Watchers duplican archivos | Fase posterior con deduplicación persistente por ruta/inodo/hash |
| Logs con datos sensibles | Redacción y exclusión de credenciales |
| GUI y daemon desincronizados | El daemon es la única fuente de verdad; la GUI reconstruye desde eventos y `jobs.list` |

## 19. Criterios de aceptación del MVP

- [ ] Un usuario instala el `.deb` en Ubuntu 26.04 y abre Cauce desde el menú.
- [ ] El daemon arranca por socket y sobrevive al cierre de la GUI.
- [ ] Se pueden añadir varios archivos y elegir un preset.
- [ ] La cola convierte al menos MP4/H.264, MKV/H.265, WebM/VP9 y audio MP3/FLAC.
- [ ] El progreso refleja el archivo real, no un temporizador simulado.
- [ ] Pausar, reanudar, cancelar y reintentar funcionan.
- [ ] Una salida inválida no reemplaza el destino.
- [ ] El archivo de entrada nunca se sobrescribe.
- [ ] La GUI funciona bajo Xvfb y el paquete pasa clean-install.
- [ ] La documentación de paridad está publicada y enlazada desde este documento.

## 20. Decisiones abiertas

1. Crear `LICENSE` y `NOTICE` con la licencia ratificada.
2. Definir la política exacta de notificaciones del escritorio.
3. Ratificar los nombres localizados del catálogo de 13 presets antes de traducir PT-BR.

## 21. ADRs y backlog

Las decisiones estructurales están registradas en [`adr/`](adr/). Los contratos operativos están en:

- [`contracts.md`](contracts.md) — tipos, estados, IPC y errores.
- [`presets.md`](presets.md) — catálogo canónico de los 13 presets.
- [`data-model.md`](data-model.md) — SQLite, transacciones y recuperación.
- [`ui-mvp.md`](ui-mvp.md) — flujo y estados de la GUI MVP.

El trabajo se desglosa en [`backlog.md`](backlog.md) y la paridad está en [`parity.md`](parity.md). Los ADRs aceptados son normativos; cualquier cambio de decisión requiere uno nuevo que lo reemplace.
