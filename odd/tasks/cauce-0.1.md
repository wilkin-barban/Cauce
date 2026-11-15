# Cauce 0.1 — MVP conversor multimedia nativo

**Feature:** Cauce v0.1 — conversor multimedia para Ubuntu 26.04 (Rust + GTK4/Libadwaita, daemon propio, FFmpeg del sistema, `.deb`).
**Autoridad normativa:** `docs/architecture.md`, `docs/contracts.md`, `docs/presets.md`, `docs/data-model.md`, `docs/ui-mvp.md`, `docs/parity.md`, `docs/backlog.md`, `docs/adr/`. Ninguna tarea puede contradecirlos; si una contradicción aparece, se registra en `## Log` y se frena hasta resolverla.
**Estado:** plan aprobado por el usuario el 2026-11-14. Fases 1-5 sin implementar. `C-001` ya está completo (licencia y `NOTICE` en `main`).

---

## Specs

Citas literales del pedido del usuario. No se reformulan ni se amplían.

| ID | Requisito (literal) | Tareas |
|---|---|---|
| S1 | "Una app de escritorio en Rust + GTK4/Libadwaita, con daemon propio y GUI nativa." | C-010, C-040 |
| S2 | "FFmpeg del sistema como dependencia; nada de binarios embebidos en v0.1." | C-020, C-050 |
| S3 | "SQLite WAL, IPC JSON-RPC por socket Unix, cola persistente y un .deb instalable." | C-030, C-032, C-050 |
| S4 | "Una interfaz minimalista, simple, rápida y agradable. No quiero una app complicada ni una copia de la UI o de la lógica de otro proyecto. Cauce debe tener su propia marca y personalidad." | C-040, C-041, C-042, C-043 |
| S5 | "Convertir archivos y carpetas, inspeccionar con FFprobe y elegir entre los 13 presets del sistema." | C-012, C-013, C-041 |
| S6 | "Añadir a la cola y luego iniciar; pausar, reanudar, cancelar y reintentar." | C-031, C-042 |
| S7 | "Progreso real, validación de salida, política de duplicados, diagnóstico, logs básicos y ajustes." | C-016, C-021, C-043 |
| S8 | "i18n ES/EN/PT-BR desde el primer release." | C-044 |
| S9 | "Las conversiones deben seguir cuando se cierre la GUI." | C-033, C-061 |
| S10 | "Watchers, historial, presets personalizados y aceleración por hardware quedan documentados para fases posteriores; no los implementes en v0.1." | Todas (no-alcance) |
| S11 | "Las especificaciones de `docs/` son la autoridad" […] "No inventes comportamiento que contradiga esos documentos." | Todas |
| S12 | "primero el test que falla y su evidencia RED, después la implementación mínima, GREEN y refactor" […] "Cada unidad cerrada con commit atómico en `main`, Conventional Commits, sin ramas, sin worktrees y sin atribución de IA ni Co-Authored-By." | Todas |
| S13 | "Seguí el orden del backlog: empezá por C-010 y el núcleo, no por la GUI." | Orden de fases |
| S14 | "usá la PC Ubuntu para construir y probar el .deb, hacer clean install, ejecutar la GUI y tomar capturas reales. Pulí la estética con criterio minimalista." | C-050, C-051, C-061 |
| S15 | "No publiques release ni muevas tags hasta tener QA y mi aprobación." | C-062 |
| S16 | "Reportá hitos reales, no cierres el panel y quedate idle al terminar cada fase." […] "No tomes la UI ni la lógica de otro proyecto como referencia: Cauce es un proyecto propio." | Todas |

---

## Reglas de ejecución

1. **TDD estricto (S12).** Cada tarea abre con un test que falla. La evidencia RED (comando + salida recortada) se anota en `## Log` antes de escribir la implementación. Sin RED registrado, no hay GREEN.
2. **Un commit por tarea, en `main`.** Convención `tipo(scope): descripción` en inglés. Prohibido: ramas de feature, worktrees, `Co-Authored-By`, trailers de IA.
3. **Puertas del backlog** (`docs/backlog.md:97-154`): G1 núcleo, G2 daemon, G3 GUI, G4 paquete, G5 release. Una fase no se cierra sin su puerta.
4. **Sin atajos de alcance (S10).** `watchers`, historial visual, presets personalizados y VAAPI/NVENC/QSV quedan documentados y fuera del binario.
5. **Sin automatismos de publicación (S15).** C-062 prepara el candidato; tag, release y push de artefactos requieren aprobación explícita.
6. **Idle al cerrar fase (S16).** Al terminar cada fase: reporte de hito real, suite completa en verde, commit hecho, y espera.

---

## Fases

| Fase | Alcance | Puerta | Items |
|---|---|---|---|
| 1 | Workspace y núcleo puro | G1 | C-010 → C-016, C-017 |
| 2 | FFmpeg, persistencia, daemon | G2 | C-020 → C-033 |
| 3 | GUI GTK4/Libadwaita + i18n | G3 | C-040 → C-044 |
| 4 | Empaquetado `.deb` y verificación | G4 | C-050 → C-052 |
| 5 | Integración real y smoke | G5 | C-060 → C-062 |

Orden obligatorio (S13): núcleo antes que GUI; la GUI nunca es la primera pieza.

---

## Tareas

Una línea por tarea. `Route` indica quién ejecuta: `inline` (sesión principal) o `worker` (writer acotado, cuando la unidad supera varios archivos o ~100 líneas).

| ID | Backlog | Fase | Specs | Route | Commit previsto |
|---|---|---|---|---|---|
| T1 | C-010 | 1 | S1, S11, S12 | inline | `chore(workspace): add rust workspace and crate skeletons` |
| T2 | C-011 | 1 | S11, S12 | inline | `feat(core): add domain types and serialization contract` |
| T3 | C-017 | 1 | S11, S12 | inline | `test(fixtures): add reproducible ffmpeg fixture generator` |
| T4 | C-012 | 1 | S5, S12 | inline | `feat(core): parse ffprobe output into media snapshots` |
| T5 | C-013 | 1 | S5, S11 | inline | `feat(core): add canonical thirteen-preset catalog` |
| T6 | C-014 | 1 | S5, S7, S11 | inline | `feat(core): resolve copy and transcode per stream` |
| T7 | C-015 | 1 | S2, S11, S12 | inline | `feat(core): build safe ffmpeg argv from conversion plans` |
| T8 | C-016 | 1 | S7, S11 | inline | `feat(core): resolve output paths and validate published media` |
| T9 | C-020 | 2 | S2, S7 | inline | `feat(ffmpeg): locate system ffmpeg and report capabilities` |
| T10 | C-021 | 2 | S6, S7 | worker | `feat(ffmpeg): run conversions with progress and process control` |
| T11 | C-030 | 2 | S3, S11 | worker | `feat(jobs): add sqlite schema with wal and migrations` |
| T12 | C-031 | 2 | S6, S11 | worker | `feat(jobs): add job state machine and scheduler` |
| T13 | C-032 | 2 | S3, S11 | worker | `feat(daemon): serve json-rpc over unix socket` |
| T14 | C-033 | 2 | S9, S11 | inline | `feat(daemon): add systemd user socket and service` |
| T15 | C-040 | 3 | S1, S4 | worker | `feat(gui): add libadwaita shell with daemon client` |
| T16 | C-041 | 3 | S4, S5 | worker | `feat(gui): add convert page with inspect and queueing` |
| T17 | C-042 | 3 | S4, S6 | worker | `feat(gui): add queue page with progress and controls` |
| T18 | C-043 | 3 | S4, S7 | worker | `feat(gui): add presets diagnostics and settings pages` |
| T19 | C-044 | 3 | S8 | worker | `feat(i18n): add es en and pt-br catalogs with live switching` |
| T20 | C-050 | 4 | S2, S3, S14 | worker | `build(deb): package binaries units and desktop assets` |
| T21 | C-051 | 4 | S14 | inline | `test(package): add check-package gate for the deb` |
| T22 | C-052 | 4 | S14 | inline | `ci: verify fmt clippy tests gui smoke and clean install` |
| T23 | C-060 | 5 | S5, S7 | inline | `test(integration): convert real fixtures across formats` |
| T24 | C-061 | 5 | S9, S14 | inline | `test(smoke): run gui and daemon under xvfb` |
| T25 | C-062 | 5 | S14, S15 | inline | `chore(release): prepare 0.1.0 release candidate` |

> **T2 (C-011) son 6 commits**, no uno: la primera versión tenía 1.054 líneas y violaba la
> regla de ≤400 por unidad. Se dividió por dominio sin cambiar comportamiento (L8).

---

## Detalle de tareas

### Fase 1 — Workspace y núcleo

**T1 · C-010 · Workspace Rust y fronteras de dependencia**
- RED: `cargo test -p cauce-workspace-check` (harness de workspace) falla porque no existe `Cargo.toml`; el test lee `cargo metadata` y exige los 7 paquetes (`cauce-core`, `cauce-ffmpeg`, `cauce-jobs`, `cauce-daemon`, `cauce-gui`, `cauce-cli`, `cauce-i18n`) y cero aristas invertidas (core no depende de tokio/gtk/sqlite; jobs no depende de gtk; gui no depende de jobs ni ffmpeg).
- GREEN: `Cargo.toml` de workspace con `resolver = "3"`, crates vacías con `lib.rs`/`main.rs` mínimos, `rust-version` y `edition` fijados.
- Refactor/aceptación: `cargo metadata --format-version 1` lista los 7 paquetes; `cargo fmt --check` y `cargo clippy --workspace --all-targets -- -D warnings` limpios. Gate G1 (parte 1).

**T2 · C-011 · Tipos de dominio y contrato de serialización**
- RED: tests de contrato fallan por tipos inexistentes: serde round-trip de `MediaSnapshot`, `ConversionPlan`, `Preset`, `Job`, `Progress`, `Settings`; un JSON dorado por tipo; rechazo de `Progress.percent = -1` y de `Job.state` desconocido.
- GREEN: structs/enums de `docs/contracts.md` §2 con `serde`, `#[serde(rename_all = "snake_case")]` donde la spec lo fija, `Option` para los campos opcionales, validación en `TryFrom`/`Deserialize`.
- Refactor/aceptación: todo tipo serializa y deserializa estable; `reasoning[]` y `metadata` presentes; ningún tipo de dominio importa `tokio`, `rusqlite` ni `gtk`.

**T3 · C-017 · Fixtures reproducibles**
- RED: test que invoca el generador y falla porque no existe; luego exige 4 fixtures (MP4/H.264+AAC, MKV/H.265, WebM/VP9+Opus, WAV/FLAC) con duración < 3 s y tamaño < 1 MiB.
- GREEN: binario/herramienta de test `tests/fixtures/generate.sh` (o `xtask`) que usa `ffmpeg -f lavfi testsrc2` y `sine`, con hash esperado en un manifiesto; los fixtures no se versionan.
- Refactor/aceptación: generación idempotente y reproducible; si `ffmpeg` falta, el test se salta con mensaje explícito, nunca en silencio.

**T4 · C-012 · Parser de FFprobe**
- RED: tests fallan por parser inexistente: JSON real de `ffprobe` de los 4 fixtures; JSON con `duration` ausente → `duration_ms = 0`; audio sin bitrate → `bitrate_bps = null`; subtítulos `text` vs `bitmap`; entrada basura → `PROBE_FAILED`.
- GREEN: parser puro de `ffprobe -show_streams -show_format -print_format json` a `MediaSnapshot`, con normalización de contenedor y detección HDR/bit depth por `pix_fmt`/`bits_per_raw_sample`.
- Refactor/aceptación: el parser no lanza panics ante JSON hostil; cubre video, audio y subtítulos; G1 (parte 2).

**T5 · C-013 · Catálogo de 13 presets**
- RED: tests fallan porque el catálogo no existe: exactamente 13 IDs de `docs/presets.md`, campos completos por preset, nombres ES/EN/PT-BR presentes, audio → `video_codec: none`, `faststart` en MP4/M4A, `hardware_preference` conservado y resuelto a CPU.
- GREEN: catálogo como datos versionados en `cauce-core` (const o `include_str!` JSON embebido), inmutable, sin I/O.
- Refactor/aceptación: un test de mutabilidad comprueba que resolver un preset no altera otro ni el catálogo; los 13 IDs coinciden literalmente con `presets.md`.

**T6 · C-014 · Motor de compatibilidad**
- RED: tests fallan por motor inexistente: para cada uno de los 13 presets y un snapshot de video/audio, el plan declara `copy`/`transcode`/`remove` por pista con `reasoning[]` no vacío; contenedor incompatible fuerza recodificación; preset de audio produce `video: remove`; `copy` nunca declara encoder.
- GREEN: `plan(snapshot, preset) -> ConversionPlan` en `cauce-core`, puro, con tablas de compatibilidad codec/contenedor/subtítulos.
- Refactor/aceptación: combinaciones inválidas se rechazan con `PLAN_INVALID` antes de FFmpeg; `hdr_action` explícito; invariantes de `contracts.md` §3 verificados por test.

**T7 · C-015 · Command builder seguro**
- RED: tests de argv dorado fallan: para cada plan representativo se compara el `Vec<String>` exacto, sin shell; se rechaza cualquier flag fuera de la allowlist; el destino entregado es siempre el temporal, nunca la ruta final.
- GREEN: builder puro que emite `-progress pipe:1 -nostats -i <input> -map ... -c:v ... -c:a ... <filtros permitidos> -movflags +faststart <temp>` según `architecture.md` §10, con allowlist de filtros.
- Refactor/aceptación: ningún argumento se construye por concatenación de string; `-i` aparece una sola vez; entrada = ruta autorizada.

**T8 · C-016 · Resolución de salida y validador**
- RED: tests fallan: política `suffix` genera `nombre (1).mp4` sin sobrescribir; `skip` devuelve `OUTPUT_COLLISION`; `overwrite` no toca el destino hasta publicar; `output_path != input_path`; validador rechaza temporal vacío, duración fuera de tolerancia (2 s o 3 %), streams faltantes, códec distinto y resolución mayor al límite.
- GREEN: funciones puras de resolución de nombre y de validación sobre un `MediaSnapshot` del temporal; rename atómico en `cauce-ffmpeg`/`cauce-jobs` cuando corresponda.
- Refactor/aceptación: un fallo de validación nunca borra el destino previo; `keep_temp_on_failure` respetado. Cierra G1.

### Fase 2 — FFmpeg, persistencia y daemon

**T9 · C-020 · Localizador de FFmpeg y capacidades**
- RED: falla por módulo inexistente: con `PATH` vacío → `FFMPEG_MISSING`; con binario falso sin `libx265` → `FFMPEG_FEATURE_MISSING` con la lista de faltantes; con FFmpeg real → `ffmpeg_version`, `ffprobe_version`, encoders y filtros del MVP presentes.
- GREEN: detección vía `Command` sin shell, parseo de `-version` y `-encoders`/`-filters`, resultado `Capabilities` de `contracts.md` §6.
- Refactor/aceptación: nunca ejecuta FFmpeg para diagnosticar más de lo necesario; mensajes accionables ("instalá ffmpeg con `sudo apt install ffmpeg`").

**T10 · C-021 · Runner de procesos y progreso real**
- RED: falla por runner inexistente: un proceso falso que emite `-progress` produce `Progress` con `percent`, `processed_ms`, `fps`, `speed`, `bitrate_bps`, `eta_seconds`; pausa envía `SIGSTOP` y resume `SIGCONT`; cancelar envía `SIGTERM` y escalado a `SIGKILL`; un hijo que ignora `SIGTERM` se mata igual; no quedan huérfanos.
- GREEN: runner en `cauce-ffmpeg` sobre grupo de procesos propio, parseo de `-progress pipe:1`, emisión a 2 Hz máximo y persistencia a 1 Hz máximo.
- Refactor/aceptación: ETA por muestras, no por reloj; `percent = null` si no hay duración; cero procesos huérfanos al salir.

**T11 · C-030 · Esquema SQLite y migraciones**
- RED: fallan por store inexistente: migración desde base vacía crea `schema_version=1` y las 6 tablas; migración idempotente; migración fallida hace rollback; base con `user_version` futura se rechaza con `DATABASE_NEWER_THAN_APP`; alta concurrente de dos trabajos con la misma salida deja una sola reserva; transición inválida no escribe evento.
- GREEN: `cauce-jobs` con `rusqlite`, WAL, `foreign_keys=ON`, `busy_timeout=5000`, migraciones transaccionales, backup previo en `backups/`.
- Refactor/aceptación: ninguna ruta de la base fuera de `$XDG_DATA_HOME/cauce/cauce.db` en tests (directorio temporal); invariantes de `data-model.md` §7 cubiertos.

**T12 · C-031 · Máquina de estados y scheduler**
- RED: fallan: matriz completa de transiciones de `contracts.md` §4 (válidas aceptan, inválidas rechazan sin evento); `pause` solo desde `running`; `resume` solo desde `paused`; `cancel` desde `queued|probing|running|paused`; reinicio marca `interrupted` con `reason = recovery` y no reencola solo; concurrencia respeta `max_concurrent_jobs`; reintento `failed → queued`.
- GREEN: máquina de estados pura en `cauce-core`, scheduler en `cauce-jobs`/`cauce-daemon` que toma `queued` por prioridad y antigüedad.
- Refactor/aceptación: cada transición escribe `job_events` en la misma transacción; la cola pausada no inicia nada pero permite cancelar.

**T13 · C-032 · Daemon y JSON-RPC**
- RED: fallan: socket en `$XDG_RUNTIME_DIR/cauce/daemon.sock` con modo `0600`; `initialize` responde `protocol_version: 1`; los 17 métodos de `contracts.md` §6 responden su forma; mensaje > 1 MiB cierra la conexión; `SO_PEERCRED` de otra UID se rechaza; NDJSON inválido cierra; error `PATH_DENIED` para ruta no autorizada; eventos `job.updated`/`job.progress`/`queue.updated`/`log.entry`/`daemon.status`.
- GREEN: daemon headless con Tokio + `tokio::net::UnixListener`, despacho tipado, broadcaster de eventos, sesión persistente.
- Refactor/aceptación: la GUI no necesita SQLite ni FFmpeg; el daemon reconstruye estado tras reconexión. Cierra G2 (parte IPC).

**T14 · C-033 · systemd de usuario**
- RED: falla la validación de unidades: `cauce.socket` en `$XDG_RUNTIME_DIR/cauce/daemon.sock` con `SocketMode=0600`, `cauce.service` con `WantedBy=default.target` y sin privilegios elevados; activación por socket arranca el daemon.
- GREEN: `packaging/cauce.socket` y `packaging/cauce.service`, más ayuda de activación desde la GUI/CLI.
- Refactor/aceptación: cerrar la GUI no detiene el daemon ni cancela trabajos (S9). Cierra G2.

### Fase 3 — GUI nativa

**T15 · C-040 · Shell Libadwaita y cliente IPC**
- RED: falla el smoke: `xvfb-run -a cargo run -p cauce-gui` abre ventana, conecta al daemon, muestra estado y reconecta si el socket cae; un daemon ausente muestra banner con acción "Reconectar", no un crash.
- GREEN: `AdwApplicationWindow` con `AdwNavigationSplitView` y las 5 páginas de `ui-mvp.md`, cliente JSON-RPC sin bloqueo del hilo principal.
- Refactor/aceptación: la GUI no abre SQLite ni lanza FFmpeg (verificado por test de dependencias y grep de imports); estado vacío distinto de error.

**T16 · C-041 · Página Convertir**
- RED: fallan: añadir archivos y carpeta dispara `media.inspect`; resumen de plan con `reasoning[]`; "Añadir a la cola" nunca inicia conversión; estados Inicial/Inspeccionando/Listo/Parcial/Error; FFmpeg ausente bloquea con acción concreta.
- GREEN: página con drop zone, selector de carpeta de salida, selector de preset y lista de rechazados.
- Refactor/aceptación: sin bloqueo del hilo de UI; accesibilidad por Tab y etiquetas en botones de icono.

**T17 · C-042 · Página Cola**
- RED: fallan: progreso real por trabajo (porcentaje, FPS, velocidad, ETA); pausar/reanudar/cancelar/reintentar/eliminar invocan los métodos correctos; diálogo de confirmación al cancelar y al eliminar activo; estado vacío "No hay trabajos en la cola".
- GREEN: lista de trabajos con controles y acciones de abrir carpeta/archivo al completar.
- Refactor/aceptación: la GUI reconstruye la cola desde `jobs.list` + eventos; no mantiene estado propio divergente.

**T18 · C-043 · Presets, Diagnóstico y Ajustes**
- RED: fallan: catálogo legible con los 13 presets y explicación de copy/transcode; diagnóstico muestra versión de FFmpeg, encoders, CPU, disco y estado del daemon con acción si falta algo; ajustes persisten carpeta, concurrencia (1-8), tema e idioma.
- GREEN: tres páginas Libadwaita con `AdwPreferencesPage`/`AdwStatusPage` donde corresponda.
- Refactor/aceptación: cambios de ajustes pasan por `settings.set` validado en el daemon, nunca escritura directa.

**T19 · C-044 · i18n ES/EN/PT-BR**
- RED: fallan: catálogos ES/EN/PT-BR completos y sin claves faltantes; selector de idioma cambia en vivo; clave ausente cae a inglés con warning; plurales y formatos de fecha correctos.
- GREEN: `cauce-i18n` con catálogos embebidos y API de traducción usada por GUI y CLI.
- Refactor/aceptación: test que compara el conjunto de claves entre los tres idiomas y falla ante diferencias; los nombres de preset salen de `presets.md` en los tres idiomas. Cierra G3.

### Fase 4 — Paquete

**T20 · C-050 · `.deb` y activos**
- RED: falla `cargo deb`: el paquete no existe; el test exige `/usr/bin/cauce-{daemon,gui,cli}`, unidades en `/usr/lib/systemd/user/`, `.desktop`, AppStream `io.github.wilkin.Cauce.metainfo.xml`, icono y `Depends: ffmpeg, libgtk-4-1, libadwaita-1-0, libnotify4, xdg-desktop-portal`.
- GREEN: metadatos `[package.metadata.deb]`, activos en `packaging/`, build release en la PC Ubuntu.
- Refactor/aceptación: el `.deb` no embebe binarios FFmpeg (S2); instalación en `ssh ubuntu` con `apt` resolviendo dependencias.

**T21 · C-051 · Puerta `check-package`**
- RED: falla `packaging/check-package.sh` contra un `.deb` incompleto o con permisos erróneos.
- GREEN: script que valida binarios ejecutables, unidades, metadatos AppStream (`appstreamcli validate`), iconos, permisos y dependencias declaradas.
- Refactor/aceptación: exit code distinto de cero con mensaje concreto por cada defecto; pasa sobre el `.deb` real.

**T22 · C-052 · CI Ubuntu 26.04**
- RED: falla el workflow: sin CI no hay verificación.
- GREEN: workflow con `cargo fmt --check`, `cargo clippy -D warnings`, tests, smoke GUI con `xvfb-run` y clean install del `.deb` en contenedor limpio.
- Refactor/aceptación: el job de clean install instala el paquete y arranca el daemon. Cierra G4.

### Fase 5 — Integración y cierre

**T23 · C-060 · Conversión real**
- RED: fallan conversiones reales de los 4 fixtures (MP4/H.264, MKV/H.265, WebM/VP9, audio MP3 y FLAC): salida existe, valida, duración y códecs correctos.
- GREEN: ajustes mínimos descubiertos por la integración real.
- Refactor/aceptación: la entrada nunca se sobrescribe; una salida inválida no reemplaza el destino; `keep_temp_on_failure` respetado.

**T24 · C-061 · Smoke GUI + daemon**
- RED: falla el smoke end-to-end: Xvfb abre la ventana, encola un fixture, inicia, progresa y completa; cerrar la GUI no detiene la conversión.
- GREEN: script `tests/smoke-gui.sh` con capturas reales tomadas en la PC Ubuntu.
- Refactor/aceptación: sin sleeps arbitrarios como criterio; el test espera condiciones observables.

**T25 · C-062 · Candidato de release**
- RED: no aplica test nuevo; se ejecuta la matriz completa de aceptación de `architecture.md` §19 y `parity.md`.
- GREEN: notas, checksums y `.deb` candidato preparados localmente.
- Refactor/aceptación: **no** se crea tag, **no** se publica release, **no** se sube artefacto. Cierra G5 cuando el usuario apruebe (S15).

---

## Criterios de aceptación de v0.1

Copiados de `architecture.md` §19 y `ui-mvp.md` §170-179; se marcan solo con evidencia observada.

- [ ] Instalar el `.deb` en Ubuntu 26.04 limpio y abrir Cauce desde el menú.
- [ ] El daemon arranca por socket y sobrevive al cierre de la GUI.
- [ ] Añadir varios archivos y elegir un preset.
- [ ] La cola convierte al menos MP4/H.264, MKV/H.265, WebM/VP9 y audio MP3/FLAC.
- [ ] El progreso refleja el archivo real, no un temporizador simulado.
- [ ] Pausar, reanudar, cancelar y reintentar funcionan.
- [ ] Una salida inválida no reemplaza el destino.
- [ ] El archivo de entrada nunca se sobrescribe.
- [ ] La GUI funciona bajo Xvfb y el paquete pasa clean install.
- [ ] ES/EN/PT-BR completos y conmutables en vivo.
- [ ] Diagnóstico muestra FFmpeg, encoders, CPU, disco y daemon con acciones concretas.
- [ ] `watchers`, historial, presets personalizados y hardware **no** están implementados.

---

## Riesgos

| Riesgo | Impacto | Mitigación |
|---|---|---|
| Build de FFmpeg sin un encoder/filtro del MVP | Bloquea presets | `system.capabilities` valida al arrancar; `FFMPEG_FEATURE_MISSING` con acción; verificado ya en `ssh ubuntu`: 8.0.1 con `libx264`, `libx265`, `libsvtav1`, `libvpx-vp9`, `libmp3lame`, `libopus` |
| Filtros de tone-mapping HDR→SDR ausentes | Presets Smart TV/YouTube fallan | Verificar `zscale`/`tonemap` en la puerta de capacidades antes de implementar T7; si faltan, preguntar antes de improvisar |
| Escalado de procesos y huérfanos | FFmpeg zombi, recursos | Grupo de procesos propio, `SIGSTOP`/`SIGCONT`, `SIGTERM`→`SIGKILL`, test de no-huérfanos (T10) |
| Colisiones de salida entre trabajos | Sobrescritura de datos | `output_reservations` en la misma transacción + temporal único + rename atómico (T8, T11) |
| Spin de GTK en el hilo principal | GUI congelada | Cliente IPC asíncrono; smoke con Xvfb; prohibido I/O bloqueante en handlers |
| i18n incompleta al release | Incumple S8 | Test de paridad de claves ES/EN/PT-BR en CI |
| `.deb` con dependencias mal declaradas | Clean install roto | `check-package` + CI en contenedor limpio (T21, T22) |
| Deriva hacia una copia de otro producto | Incumple S4/S16 | Identidad propia: nombres, iconografía y copy de Cauce; nada de assets ni código de terceros |
| Contexto agotado en fases largas | Pérdida de continuidad | ODD en repo + espejo en Engram tras cada tarea material |

---

## Commits previstos

30 commits atómicos en `main`, en el orden de la tabla de tareas (T2 se dividió en 6 commits, ver L8). Cada uno: suite verde, `fmt` y `clippy` limpios, sin `Co-Authored-By` ni trailers de IA. Las fases 1-4 cierran con su puerta; la fase 5 no publica nada.

---

## Log

**L1 (pedido original del usuario, literal):**

> Soy Wilkin. Quiero que empieces Cauce, un conversor multimedia nativo para Ubuntu 26.04, independiente y con identidad propia.
>
> Qué quiero construir:
> - Una app de escritorio en Rust + GTK4/Libadwaita, con daemon propio y GUI nativa.
> - FFmpeg del sistema como dependencia; nada de binarios embebidos en v0.1.
> - SQLite WAL, IPC JSON-RPC por socket Unix, cola persistente y un .deb instalable.
> - Una interfaz minimalista, simple, rápida y agradable. No quiero una app complicada ni una copia de la UI o de la lógica de otro proyecto. Cauce debe tener su propia marca y personalidad.
>
> Alcance del MVP v0.1:
> - Convertir archivos y carpetas, inspeccionar con FFprobe y elegir entre los 13 presets del sistema.
> - Añadir a la cola y luego iniciar; pausar, reanudar, cancelar y reintentar.
> - Progreso real, validación de salida, política de duplicados, diagnóstico, logs básicos y ajustes.
> - i18n ES/EN/PT-BR desde el primer release.
> - Las conversiones deben seguir cuando se cierre la GUI.
> - Watchers, historial, presets personalizados y aceleración por hardware quedan documentados para fases posteriores; no los implementes en v0.1.
>
> Las especificaciones de `docs/` son la autoridad: `architecture.md`, `contracts.md`, `presets.md`, `data-model.md`, `ui-mvp.md`, `parity.md`, `backlog.md` y `docs/adr/`. No inventes comportamiento que contradiga esos documentos.
>
> Primero necesito que explores el repositorio y el entorno, sin empezar a codificar. Revisá el estado de git, la documentación, las herramientas disponibles y FFmpeg en la PC Ubuntu (`ssh ubuntu`).
>
> Después quiero que crees el artefacto ODD en `odd/tasks/cauce-0.1.md`, con fases, tareas, tests RED/GREEN, criterios de aceptación, riesgos y commits previstos. No implementes nada antes de que el ODD esté escrito.
>
> Luego desarrollá bajo TDD estricto: primero el test que falla y su evidencia RED, después la implementación mínima, GREEN y refactor. Los tests deben cubrir el núcleo, command builder, estados, SQLite/IPC, validación, smoke de GUI y paquete. Cada unidad cerrada con commit atómico en `main`, Conventional Commits, sin ramas, sin worktrees y sin atribución de IA ni Co-Authored-By.
>
> Seguí el orden del backlog: empezá por C-010 y el núcleo, no por la GUI. Cuando llegues a la interfaz, usá la PC Ubuntu para construir y probar el .deb, hacer clean install, ejecutar la GUI y tomar capturas reales. Pulí la estética con criterio minimalista. No publiques release ni muevas tags hasta tener QA y mi aprobación.
>
> Reportá hitos reales, no cierres el panel y quedate idle al terminar cada fase. Si una decisión contradice las specs o el ODD, preguntá antes de improvisar. No tomes la UI ni la lógica de otro proyecto como referencia: Cauce es un proyecto propio.

**L2 (exploración, hechos observados 2026-11-14):**
- Repo: `main` limpio y sincronizado, 5 commits de solo documentación. Sin `Cargo.toml`, `crates/`, `packaging/`, `tests/`, `README.md` ni `odd/`.
- Host local: Ubuntu 26.04.1 LTS x86_64; `cargo`/`rustc` 1.99.1; `cargo-deb` 3.8.0; `pkg-config` ve `gtk4` 4.22.4 y `libadwaita-1` 1.9.1.
- `ssh ubuntu`: Ubuntu 26.04.1 LTS x86_64, kernel 7.0.0-43, usuario `wilkin`; `ffmpeg`/`ffprobe` 8.0.1-3ubuntu2+esm5 en `/usr/bin`, con `libx264`, `libx265`, `libsvtav1`, `libvpx-vp9`, `libmp3lame`, `libopus` habilitados.
- Orden del backlog confirmado en `docs/backlog.md:18,41,56`: `C-001` completo; se arranca por `C-010`.

**L3 (decisiones del plan):**
- 25 tareas = 25 commits atómicos, una por ítem del backlog MVP, en el orden de dependencias del backlog (S13).
- `T3` (C-017, fixtures) se adelanta a `T4` porque el parser de FFprobe necesita fixtures reales.
- Ninguna tarea implementa `C-070`..`C-076` (S10).

**L4 (T1 · C-010 · 86f1ece) — RED observado:**
- `cargo test -p cauce-core --test workspace_boundaries` → `error: could not find `Cargo.toml` in `/home/wilkin/proyectos/Cauce` or any parent directory`, exit 101.
- GREEN: workspace con los 7 crates y `serde_json` como dev-dependency de `cauce-core`; 3/3 tests pasan.
- Sonda negativa (el guard muerde): agregar `cauce-jobs = { path = "../cauce-jobs" }` a `cauce-core` produce `cauce-core must not depend on cauce-jobs (docs/architecture.md §5)`, 1 failed, 2 passed. Revertido.
- `cargo fmt --check` y `cargo clippy -D warnings` limpios.

**L5 (T2 · C-011 · 6 commits, ver L8) — RED observado:**
- `cargo test -p cauce-core --test domain_contracts` → `E0432`: 27 imports sin resolver (`MediaSnapshot`, `ConversionPlan`, `Job`, `Preset`, …) y `E0433`: `serde` no encontrado, exit 101.
- GREEN dividido por dominio, cada commit compilando y testeando solo:
  - `0f1e10b` `feat(core): add utc timestamp contract` — 158 líneas.
  - `2fc9619` `feat(core): add media snapshot contract` — 193.
  - `32effa7` `feat(core): add conversion plan contract` — 183.
  - `c0c9d5b` `feat(core): add stable job error codes` — 63.
  - `5eeaff7` `feat(core): add job state progress and priority contract` — 238.
  - `ab304e6` `feat(core): add preset and settings contracts` — 258.
- Seis JSON dorados (snapshot, plan, progress, job, settings, preset) y rechazo de `percent < 0`, `state` desconocido, `output_path == input_path`, concurrencia fuera de 1..=8 y timestamp no UTC.
- Decisión de interpretación (no contradicción): `Preset` lleva `name_pt_br` y `description_pt_br` porque `presets.md` ratifica nombres PT-BR y S8 exige PT-BR; `contracts.md` §2.3 enumera campos comunes sin prohibir añadidos y §10 permite añadir campos opcionales sin romper el protocolo. Pendiente de veto del usuario.

**L6 (T3 · C-017 · db1d72f) — RED observado:**
- `cargo test -p cauce-ffmpeg --test fixtures` → 3/3 fallan: `the fixture generator must live at …/tests/fixtures/generate.sh`.
- GREEN: 5 fixtures sintéticos (`h264+aac.mp4`, `hevc.mkv`, `vp9+opus.webm`, `flac`, `mp3`) de 24–89 KB, duración 2 s, y **byte-idénticos** entre dos corridas (verificado también para x265 y el muxer MP4 con `-fflags +bitexact -flags +bitexact -map_metadata -1`).
- Los fixtures viven en `target/cauce-fixtures/` y no se versionan; `tests/fixtures/manifest.json` declara nombres, contenedor y cotas.
- Árbol idéntico a la versión previa a la división: comparado antes de reescribir la historia, sin diferencias en `tests/fixtures` ni `crates/cauce-ffmpeg`.

**L7 (proceso) — desvío registrado:** la evidencia RED se capturó en la sesión al momento de correr cada test, pero se transcribió a este Log después de cada commit y no antes de implementar, como pedía la regla 1. Corregido: desde T4 la entrada del Log se escribe antes de la implementación.

**L8 (división de T2, pedido del usuario):**
> "el commit T2 `6e6fbd4` tiene 1.054 líneas y supera la regla de commits atómicos ≤400" […] "Dividí T2 en commits coherentes de ≤400 líneas, sin perder ningún tipo ni test y sin cambiar el comportamiento."

- Mecánica: `git reset --soft 86f1ece` sobre trabajo no pusheado, `git restore --staged .`, y staging por dominio; T1 (`86f1ece`) se preservó intacto.
- Cambio estructural permitido por el usuario: `JobError`/`ErrorCode` se extrajeron de `src/job.rs` a `src/error.rs` (dominio propio, `contracts.md` §8) y `tests/domain_contracts.rs` se partió en `tests/{time,media,plan,error,job,preset,settings}_contracts.rs` más `tests/common/mod.rs`.
- Prueba de no-regresión: los mismos 12 tests de contrato conservan nombre y aserciones; suite total 18/18; T3 byte-idéntico.
- Efecto en el plan: 25 tareas → 30 commits.

**L9 (T4 · C-012 · parser de FFprobe) — entrada escrita antes de implementar:**
- RED previsto: `cargo test -p cauce-core --test probe_contracts` falla por `parse_ffprobe` inexistente (E0432/E0425).
- Casos: capturas reales de los 5 fixtures; `duration` ausente → `duration_ms = 0`; audio sin `bit_rate` → `None`; subtítulos `text` vs `bitmap`; FPS fraccionario `30000/1001`; JSON basura → `PROBE_FAILED`.
- Diseño: parser puro en `cauce-core::probe` sin procesos; las capturas reales de `ffprobe` se guardan como JSON en `tests/fixtures/ffprobe/` para no ejecutar binarios desde los tests de `cauce-core`.

**L9 (T4) — RED observado, GREEN y decisiones:**
- RED real: `error[E0432]: unresolved import `cauce_core::probe`` (módulo inexistente) con los 8 tests ya escritos.
- GREEN: 8/8 en `probe_contracts` + `probe_edges`; suite total 26/26; `fmt --check` y `clippy -D warnings` limpios.
- Firma: `parse_ffprobe(path: &str, probed_at: Timestamp, source: &str) -> Result<MediaSnapshot, JobError>`; malformado → `PROBE_FAILED` no reintentable.
- Reglas fijadas: contenedor por extensión con fallback al primer token de `format_name`; `duration_ms` redondeado, ausente → 0; `fps` desde `r_frame_rate` (o `avg_frame_rate`) como fracción; `bit_depth` desde `bits_per_raw_sample` o sufijo de `pix_fmt`; `is_hdr` = `color_transfer` en {`smpte2084`,`arib-std-b67`}; subtítulos bitmap {`dvd_subtitle`,`hdmv_pgs_subtitle`,`dvb_subtitle`,`xsub`} y el resto texto; `language` `und` → `None`; streams `data`/`attachment` descartados.
- Las capturas reales de `ffprobe` se commitean minificadas en `tests/fixtures/ffprobe/` (11 KB total) para que los tests de `cauce-core` no ejecuten binarios.
- División por la regla ≤400: `24d4e1e` capturas · `6620342` parser + captura MP4 (320) · `fd5987d` multiformato y bordes (247). Los 8 tests se escribieron antes de la implementación y se reparten entre los dos últimos commits; ambos comparten el mismo RED observado.
- Hallazgo del fixture real: el AAC de `generate.sh` es mono (`channels: 1`), no estéreo. El test afirma el byte real, no el valor esperado a mano.
- T4 se ejecutó delegado a un writer acotado; el parent verificó suite, fmt y clippy por su cuenta antes de commitear.

**L10 (T5 · C-013 · d4e4ba8 + d3be22a + 204cab3) — RED observado, conflicto resuelto y decisiones:**
- RED real: `error[E0432]: unresolved import `cauce_core::system_presets``. Segundo RED tras partir datos: `couldn't read .../catalog-video.json` y `catalog must contain `system-audio-mp3``.
- GREEN: 4 tests de video, 3 de audio, 4 de catálogo completo; suite total 37/37; fmt y clippy limpios.
- **Conflicto detectado y resuelto a favor de `docs/` (S11)**: mi handoff pedía `max_fps = 60` para `system-smarttv-hevc`, pero `presets.md:25` no declara frame rate para ese preset (solo 3840×2160). Se dejó `max_fps: null` y el test pasó a exigir 60 fps solo para Samsung, LG y Android TV. No se inventó una restricción.
- **Desviación detectada en verificación propia y corregida**: 3 nombres no eran literales de `presets.md`. Quedaron como el documento: `system-smarttv-compat` EN `Maximum Compatibility`, `system-smarttv-hevc` EN `Efficient Quality`, `system-android-tv` ES `Android TV` / EN `Google TV` (PT-BR conserva `Android TV / Google TV`).
  - Observación para el usuario: la columna "Nombre ES / EN" de `presets.md` es asimétrica en esos tres casos (el prefijo "Smart TV —" no aparece del lado EN). Si preferís nombres simétricos, hay que editar `presets.md`; no lo hice por mi cuenta.
- Huecos de spec resueltos sin contradecir nada: `category` no está enumerado en `presets.md` → se usa {`tv`, `social`, `archive`, `audio`}; los 5 presets de audio declaran `subtitle_policy: remove` porque sus contenedores no llevan subtítulos; `system-audio-wav` deja `audio_bitrate` en `null` porque "PCM 16-bit" es formato de muestra, no tasa.
- Datos partidos por dominio (`catalog-video.json` 8 presets, `catalog-audio.json` 5) para respetar la regla de ≤400 líneas: 378 + 246 + 212.
- T5 se ejecutó delegado a un writer; el parent verificó los 13 IDs, los 39 nombres contra `presets.md` y el mapeo de campos con un script propio antes de commitear.

**L11 (T6 · C-014 · e6bd200 + 400a2fa + 57aa64b) — RED observado, excepción aprobada y límites de contrato:**
- RED real: `error[E0432]: unresolved import `cauce_core::compat`` en ambos test targets.
- GREEN: 7 tests de video/audio + 8 de subtítulos; suite total 53/53; fmt y clippy limpios.
- **Excepción de tamaño aprobada por el usuario (A)**: motor `compat.rs` en un commit de 491 líneas (+22% sobre 400) porque es una función pura indivisible; los tests van en dos commits inmediatos de 352 y 280, sin stubs ni código interino.
- **Bug propio corregido**: la regla 9 de mi handoff pedía `AudioPlan.encoder` igual al códec del preset (`mp3`, `opus`), pero `presets.md` §"Resolución en v0.1" y `architecture.md` §11 exigen los encoders reales de FFmpeg. Ahora se mapea `aac→aac`, `mp3→libmp3lame`, `flac→flac`, `alac→alac`, `pcm_s16le→pcm_s16le`, `opus→libopus`, con `audio_transcodes_use_the_real_ffmpeg_encoder` como test de regresión. Sonda negativa verificada: al revertir el mapeo, el test falla con `left: Some("mp3")`.
- **Límite de contrato documentado (no se agregó `bit_depth`)**: `presets.md` decía "10-bit"; `contracts.md` §2.2 no declara bit depth en `VideoPlan`. Por decisión del usuario se reescribió a "hasta 10-bit (preserva si la fuente ya es 10-bit)" en los cuatro presets UHD y en la prueba obligatoria, sin tocar el contrato. Consecuencia: una fuente 8-bit sigue en 8-bit bajo Samsung/LG/Android TV/H.265.
- **Downmix**: mi test original nombraba `system-samsung-tv` (6 canales declarados), donde 6→2 es imposible. Queda `system-smarttv-compat` para el caso 2 canales y Samsung verificado en `Some(6)`. La regla de no hacer upmix silencioso se mantiene y se explica en `reasoning`.
- Cualquier preset con `normalize_audio: true` y códec de audio no soportado se rechaza con `PLAN_INVALID`; `BurnIn` nunca se produce en v0.1.

**L12 (T7 · C-015 · 4e0b200 + dfb0d91) — RED observado, bug propio corregido y capacidad de filtros:**
- RED real: `error[E0432]: unresolved import `cauce_core::command``.
- GREEN: 12 tests de argv dorado; suite 65/65 al cerrar la unidad; fmt y clippy limpios.
- **Bug propio corregido antes de commitear**: mi argv vinculante omitía `-crf`, así que la calidad del preset (CRF 21/24/18…) se habría ignorado y FFmpeg habría usado su default. Ahora `-c:v <encoder> -crf <quality>` se emite cuando el plan declara calidad, con el vector dorado actualizado. Sonda negativa verificada: al quitar la emisión, `builds_the_exact_argv_for_a_video_transcode` falla.
- **Capacidad de filtros (pedido del usuario)**: `ssh ubuntu` tiene `zscale`, `tonemap`, `scale`, `format`, `fps` y los 10 encoders del MVP. El builder expone `REQUIRED_FILTERS` y `tonemap_filter_chain()` para que T9 verifique exactamente la misma cadena que emite.
- Smoke real de la cadena HDR: `zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv` parsea y **sale 0 sobre una fuente con primarios BT.2020/SMPTE2084**; sobre `testsrc2` SDR sin etiquetar sale 187 ("no path between colorspaces"), que es correcto: la cadena solo se emite cuando el plan declara `ToneMapSdr` sobre una fuente HDR.
- Reglas del builder: sin shell, sin binarios embebidos, allowlist de encoders y de filtros, rutas absolutas sin NUL, `-i` exactamente una vez, el último argumento es siempre el temporal, `normalize_audio` rechazado con `PLAN_INVALID` porque v0.1 no tiene filtro de normalización.
- Corte: 263 (builder) + 389 (tests).

**L13 (T8 · C-016 · fef5cc3 + 822703c + f1d0aea) — RED observado y alcance del validador:**
- RED real: `error[E0432]: unresolved imports `cauce_core::resolve_output_path`, `cauce_core::temp_path`` y ``cauce_core::validate_output``.
- GREEN: 8 tests de rutas + 9 de validación; suite 82/82; fmt y clippy limpios.
- Política de duplicados: `suffix` incrementa ` (1)`, ` (2)`… mientras la ruta esté reservada; `skip` devuelve `OUTPUT_COLLISION` reintentable; `overwrite` devuelve la misma ruta y el archivo previo no se borra hasta que la salida valide; ninguna política puede devolver la ruta de entrada.
- Temporales: `<dir>/.cauce-<job_id>.part.<ext>`, en la misma carpeta que el destino para permitir rename atómico; `job_id` restringido a `[A-Za-z0-9._-]`.
- `validate_output` es puro y cubre `architecture.md` §12.1/3/4/5/6 (no vacío, duración con tolerancia `max(2000 ms, 3%)`, streams declarados, códec esperado por encoder, resolución dentro del límite). §12.2/§12.7 (ffprobe abre la salida, destino sin cambios) y §15 (carpeta escribible, symlinks) requieren I/O y quedan para `cauce-jobs`/`cauce-ffmpeg`.
- Sondas negativas del worker: tolerancia ampliada al total → falla `rejects_a_duration_outside_tolerance`; guard de entrada desactivado → falla `never_returns_the_input_path`. Ambas revertidas.
- Corte: 364 (módulo) + 149 (rutas) + 277 (validación).

**L14 (G1 · Puerta del núcleo — cerrada)**
- `cargo fmt --all --check`: limpio. `cargo clippy --workspace --all-targets -- -D warnings`: sin issues. `cargo test --workspace`: **82 tests, 0 fallos, 30 suites**. `cargo build --release --workspace`: OK en 14 s.
- Sin GPU ni red en tests: las únicas apariciones de `nvenc`/`vaapi`/`qsv` son aserciones *negativas* (el catálogo no puede declarar encoders GPU y el builder debe rechazarlos). Cero dependencias de red.
- Fronteras verificadas por `cargo metadata`: `cauce-core` solo depende de `serde`/`serde_json`; `cauce-i18n`, `cauce-jobs`, `cauce-daemon`, `cauce-gui` y `cauce-cli` siguen sin dependencias cruzadas.
- Fase 1 completa: T1-T8 cerradas. La GUI no se tocó.

**L15 (T9 · C-020) — RED observado, inventario real y dos correcciones de spec:**
- RED real: `E0432` por `REQUIRED_ENCODERS` y `Capabilities` inexistentes, y `E0433: cannot find module or crate cauce_core` en `cauce-ffmpeg`.
- GREEN: 9 tests de toolchain + 2 de contrato de capacidades; suite total **97/97**; fmt y clippy limpios.
- **Faltaba una arista de dependencia**: `cauce-ffmpeg` no declaraba `cauce-core`, que `architecture.md` §5 exige. Autorizada y agregada como única línea de `[dependencies]`; el guard `workspace_boundaries` ya la permitía.
- **Inventario real medido** (FFmpeg 8.0.1-3ubuntu2 local): 226 encoders y 558 filtros tras descartar las líneas de leyenda (`=`). Mi primer conteo de 228/559 las incluía por error; verifiqué que el parser del código las descarta. Los 10 encoders del MVP y los 5 filtros requeridos están presentes. En `ssh ubuntu` (8.0.1-3ubuntu2+esm5) el inventario coincide.
- **Contradicción de docs detectada**: `architecture.md` §11 enumera 8 encoders CPU y omite `alac` y `pcm_s16le`, que `presets.md` exige para `system-audio-alac` y `system-audio-wav`. Se siguió `presets.md` (`REQUIRED_ENCODERS` = 10). Recomendación: actualizar §11; no lo edité por mi cuenta.
- **Línea RED del ODD corregida**: T9 no emite `FFMPEG_FEATURE_MISSING`; `inspect` devuelve `Ok(Capabilities)` con `ffmpeg_missing`/`warnings` porque `architecture.md` §10 pide diagnóstico primero. El rechazo con `FFMPEG_FEATURE_MISSING` corresponde al arranque de un trabajo (T12/T13).
- Decisión de implementación: `disk_free_bytes` vía `df -B1 --output=avail` con argv (sin shell); timeouts de 10 s por proceso con `spawn` + espera acotada, sin dependencias nuevas.

**L16 (T10 · C-021) — RED, GREEN y un test flaky corregido de raíz:**
- RED real: `E0432` por `cauce_ffmpeg::progress` y `cauce_ffmpeg::runner` inexistentes.
- GREEN: 9 tests de parser + 10 de control de procesos; suite 120/120 al cerrar la unidad.
- Implementación: grupo de procesos propio (`process_group(0)`), señales a `-pid` con `libc` (única dependencia nueva), `SIGSTOP`/`SIGCONT` para pausa, `SIGTERM`→`SIGKILL` con plazo, `Drop` mata y reap el grupo. Progreso desde `-progress pipe:1` con throttle de 2 Hz y bloque final siempre entregado. `out_time_ms` se interpreta como microsegundos (rareza documentada de FFmpeg).
- **Test flaky detectado por el worker y corregido**: `pause_suspends_the_process_group` fallaba ~1 de 4 corridas con `ETXTBSY` ("Text file busy"). Causa real: los fakes se `exec`aban recién escritos y además compartían carpeta por PID entre tests paralelos. Corrección: carpeta única por test, publicación atómica del script (`rename`) y ejecución vía `/bin/sh <script>` (el `exec` va contra un binario estable). Verificado con **20 corridas seguidas, 0 fallos**, sin sleeps ni reintentos.
- Corte: 285 (parser) + 171 (tests parser) + 333 (runner) + 397 (tests runner).

**L17 (T11 · C-030) — RED, GREEN y decisiones de persistencia:**
- RED real: `E0432` por `cauce_jobs::Store`, `MIGRATIONS`, `Migration` y `run_migrations` inexistentes.
- GREEN: 5 tests de migraciones + 6 de persistencia; suite 131/131; fmt y clippy limpios.
- **SQLite embebido** (`rusqlite` con `bundled`): el host local no tiene `libsqlite3-dev` y no quise exigir un `sudo apt install` ni atar el build a un paquete del sistema; el futuro `.deb` no dependerá de `libsqlite3-0`. Si preferís enlace contra el SQLite del sistema, es un cambio de una línea de Cargo más `libsqlite3-dev` en `Build-Depends`.
- PRAGMAs observados: `journal_mode=wal`, `foreign_keys=1`, `busy_timeout=5000`, `synchronous=1`, `user_version=1`.
- Backup previo a migrar con `VACUUM INTO` (no `cp` de un WAL vivo, como exige `data-model.md` §5); nombre `backups/cauce-v<old>-<UTC>.db`.
- `insert_job` es una sola transacción (trabajo + reserva + evento); una colisión de destino no deja ni fila ni evento. Recuperación marca `probing|running|paused|validating` como `interrupted` con evento `recovery`, sin tocar `queued`.
- Corte en 5 commits ≤400 autoradas: 394 (esquema + migraciones, con `Cargo.lock` de 242 líneas generadas) · 181 (store y ajustes) · 342 (codec + operaciones de trabajos) · 255 y 362 (tests).
- **Pendiente de T12**: la prueba de "transición inválida rechazada sin escribir evento" y el rechazo con `FFMPEG_FEATURE_MISSING` al arrancar un trabajo (T9 solo diagnostica).

**L18 (T12 · C-031) — RED, GREEN y una discrepancia entre documentos resuelta:**
- RED real: `E0432` por `transition_for`, `JobAction`, `on_restart` y `next_startable`; 18 errores `E0599` por `Store::transition`, `apply_action` y `active_count`.
- GREEN: 9 tests de máquina de estados + 6 de scheduler + 9 de transiciones; suite 155/155; fmt y clippy limpios.
- Tabla de transiciones copiada literal de `contracts.md` §4 y verificada con la matriz completa 10×10: exactamente los pares documentados son válidos.
- **Discrepancia entre documentos**: `contracts.md` §4 dice que un reinicio convierte `probing|running|validating` en `interrupted`; `data-model.md` §4 agrega `paused`. Se alineó `on_restart` a `data-model.md` (incluye `paused`) porque el proceso de un trabajo pausado muere con el daemon y la recuperación de T11 ya lo interrumpía. Test `restart_interrupts_a_paused_job_too` con sonda negativa verificada. Recomendación: agregar `paused` a la lista de `contracts.md` §4.
- `next_startable` es la única política de v0.1: prioridad DESC, `created_at` ASC, `id` ASC; devuelve `None` con la cola pausada, en el límite de concurrencia o sin trabajos `queued`.
- Ocupan slot: `probing`, `running`, `paused`, `validating`, `cancelling`.
- Limitación conocida: transicionar a `failed` sin `error_code` persistido viola el CHECK del esquema; el daemon deberá guardar el error antes de la transición (T13).
- Corte: 146 (state) + 262 (tests) + 156 (scheduler con tests) + 128 (transitions) + 356 (tests).

**L19 (T13a · C-032, primera mitad) — RED, GREEN y política de socket:**
- RED real: `E0433/E0432` por `cauce_daemon` inexistente y por `cauce_jobs`/`cauce_ffmpeg`/`tokio` no declarados en los tests (22 errores).
- GREEN: 7 tests de seguridad de transporte + 13 de métodos (20 en `cauce-daemon`); suite 175/175; fmt y clippy limpios.
- Socket en `$XDG_RUNTIME_DIR/cauce/daemon.sock` (dir 0700, socket 0600), `SO_PEERCRED` con la misma UID, NDJSON con tope de 1 MiB, cierre de la conexión ante mensaje inválido o sobredimensionado, y `unlink` del socket viejo solo si nadie contesta.
- Respuestas reales verificadas: `initialize` → `{"protocol_version":1,"server_version":"0.1.0"}`; `system.capabilities` con FFmpeg 8.0.1, 6 núcleos y disco libre.
- `cauce_ffmpeg::probe::probe_file` invoca `ffprobe` y parsea con `cauce_core::probe::parse_ffprobe` (sin duplicar el parser).
- Raíz autorizada en v0.1: `$HOME` (el portal no tiene campo de concesión en el protocolo todavía); una ruta fuera → `PATH_DENIED` con test y sonda negativa.
- **Segundo test flaky corregido**: `toolchain.rs` sufría el mismo `ETXTBSY` que T10 (fakes compartidos por PID). Se aplicó carpeta única + publicación atómica ejecutable (`chmod` sobre el archivo staged y después `rename`). Verificado con **10 corridas del workspace completo, 0 fallos**.
- Gaps documentados para T13b: `jobs.remove` devuelve `INTERNAL "no disponible todavía"` (falta `Store::remove_job`), no hay bucle de conversión (sin `job.progress`), `log.entry` no se emite (solo ring en memoria) y la pausa de cola vive solo en memoria.
- Corte: 130 (probe) · 397 (protocolo + eventos) · 245 (socket) · 62 (ids) · 390 (dispatch) · 255 (serve + main) · 260 y 400 (tests).

**L20 (T13b · C-032, orquestador) — el daemon convierte y sobrevive a la GUI:**
- RED real: `E0599` por `set_probe_and_plan`, `set_temp_path`, `set_error`, `save_progress` y `remove_job`; `E0432` por `cauce_daemon::orchestrator`.
- GREEN: 7 tests de store + 7 de orquestador; suite 189/189.
- `tick` es síncrono y testeable: reapa hijos terminados, publica o falla, y arranca como mucho un trabajo por tick según `next_startable`. El bucle asíncrono de 250 ms vive en `lib.rs` y `main.rs` lo arranca tras la recuperación.
- Conversión real observada en test: `sample-h264-aac.mp4` → salida de 53 286 B, recorrido `queued→probing→running→validating→completed`, temporal limpiado, 1 muestra de progreso (96 %).
- Reglas: `set_error` antes de `failed` (el CHECK lo exige); publicación atómica con `fs::rename`; `suffix`/`skip` nunca sobrescriben; cancelar mata el grupo, deja `cancelled` y el temporal queda si `keep_temp_on_failure`.
- Gaps documentados: el `tick` usa el `output_path` persistido en vez de re-resolverlo; el fin de proceso se detecta por el marcador de stderr (un hijo que cierre stderr antes de tiempo bloquearía un tick); `pause`/`resume` de un hijo vivo está cableado pero solo `cancel` tiene test.
- Corte: 135 · 382 · 191 · 349 · 60 · 9 · 398 (el mensaje del commit `edf0f53` menciona el arranque del scheduler, que en realidad entra en `4681c21`; el contenido es correcto).

**L21 (T14 · C-033) — unidades de usuario:**
- RED real: las 5 pruebas fallan con `No such file or directory` al leer `packaging/cauce.{socket,service}`.
- GREEN: 5 tests; suite 194/194.
- `cauce.socket`: `ListenStream=%t/cauce/daemon.sock`, `SocketMode=0600`, `DirectoryMode=0700`, `RemoveOnStop=yes`, `WantedBy=sockets.target`. `cauce.service`: `Type=exec`, `ExecStart=/usr/bin/cauce-daemon`, `Restart=on-failure`, `RestartSec=2`, `NoNewPrivileges=yes`, `Requires/After=cauce.socket`, `WantedBy=default.target`, sin `User=`.
- `systemd-analyze verify` corre de verdad (systemd 259) y devuelve una única línea: `Command /usr/bin/cauce-daemon is not executable`, esperada hasta que el `.deb` instale el binario. El test acepta **exactamente** ese diagnóstico y cualquier otro lo hace fallar; se endurecerá en C-050.

**L22 (G2 · Puerta del daemon — cerrada con el binario real):**
- `cargo fmt --check` limpio, `clippy -D warnings` sin issues, **194 tests / 46 suites** en verde.
- Smoke real (no in-process): daemon arrancado desde `target/debug/cauce-daemon` con `XDG_RUNTIME_DIR` temporal → socket `0600` en directorio `0700`; `initialize` → `protocol_version: 1`; `system.capabilities` → FFmpeg 8.0.1, `ffmpeg_missing: []`, 6 núcleos; `jobs.add` con un fixture real → `queued → running → validating → completed`, salida de 53 286 B publicada; `SIGTERM` → **exit 0** y socket eliminado.
- Recuperación tras reinicio y `SO_PEERCRED` cubiertas por tests (`marks_active_jobs_interrupted_on_recovery`, `peer_uid_matches_the_test_process`).
- La GUI no se tocó: sigue siendo `fn main() {}`.

**L23 (endurecimiento del runner, pedido del usuario) — dos huecos cerrados:**
- **Pausa/reanudación sobre un hijo vivo**: `pause_freezes_and_resume_continues_a_live_child`. El fake escribe una línea cada 20 ms; el test verifica con polling acotado que el contador deja de crecer tras `pause` y vuelve a crecer tras `resume`. Observado: 5 líneas al pausar, 5 tras 250 ms en pausa, 6 al reanudar. Sonda negativa: quitando `SIGSTOP` de `pause()`, el test falla con `the counter kept growing while paused: 5 -> 14 lines`.
- **Watchdog de salida**: `ConversionChild::try_exit()` no bloqueante (`try_wait` + caché + liberación de hilos, sin zombis). El orquestador ahora completa o falla el trabajo desde el estado de salida en cada tick, aunque el marcador `StreamEnd` nunca llegue; ambos caminos son idempotentes (3 ticks extra tras el fallo dejan un solo `failed`). Tests: `reports_exit_even_when_stderr_closes_early`, `try_exit_is_non_blocking_for_a_running_child`, `a_converter_that_closes_stderr_does_not_hang_the_queue` (el segundo trabajo de la cola arranca igual).
- Nota del worker: el script literal `exec 2>&-; exit 0` ya disparaba el marcador (EOF de stderr); el caso realmente bloqueante es un hijo que mantiene stderr abierto y muere, y ese es el que cubre el test del daemon.
- Deuda aceptada: `RunningJob.exited` queda escrito pero ya no decide el completado (el marcador es solo camino rápido); ningún test lo lee.
- Corte: 60 · 42 · 103 · 109. Suite **198/198**.

**L24 (decisión documentada) — pausa de cola al reiniciar el daemon:**
- La pausa de cola vive en memoria (`AtomicBool` del daemon). **Al reiniciar el daemon la cola vuelve a estar activa**: no se persiste en `settings` y no se reanuda ningún trabajo por sí solo.
- Los trabajos que estaban `probing|running|paused|validating` pasan a `interrupted` con evento `recovery` (`data-model.md` §4) y **no se reencolan automáticamente**: la UI debe ofrecer "Reencolar", que es la acción `Recover` de la máquina de estados.
- Los `queued` siguen `queued` y el scheduler los toma en el primer tick si la cola está activa.
- Consecuencia de UX: si el usuario pausó la cola y reinicia el daemon, la conversión de los pendientes vuelve a arrancar. Persistir el flag en `settings` queda para una fase posterior; no contradice ninguna spec y está anotado en `## Log`.

**L25 (T15 · C-040 y primera review nativa real) — evidencia:**
- RED: `couldn't find file .../src/client.rs` con los 6 tests escritos; el shell no podía imprimir `window-ready` porque el binario era un stub.
- GREEN: 6 tests de GUI (4 de transporte + 2 de smoke bajo `xvfb-run`); suite **204/204** al cerrar la unidad. Smoke real: `window-ready`, exit 0.
- Versiones resueltas contra las librerías del sistema: `gtk4 0.11.5` + `gtk4-sys 0.11.5`, `libadwaita 0.9.2` con `v1_9`, sobre GTK 4.22.4 / libadwaita 1.9.1, sin features vendorizadas.
- Se añadió `src/lib.rs` real (el worker había incluido `client.rs` por `#[path]` en los tests); `main.rs` consume la librería.
- Corte: 373 (cliente + manifiesto), 259 (tests de cliente), 373 (shell), 134 (binario + smoke). `Cargo.lock` (+616) es generado.

**L26 (review nativa del candidato T15) — escalated, con dos hallazgos corregidos:**
- `assess` marcó el rango como **high risk** por `process_boundary` en el binario de la GUI, así que la review nativa sí correspondía. Lentes: risk, resilience, reliability. Lineage `review-116bd8c960d6de90`, 10 archivos, 1759 líneas.
- **Hallazgo CRITICAL R3-001** (determinista, causado por el cambio): ante un timeout el cliente no cerraba la conexión ni cancelaba el `read_loop`, así que un daemon colgado filtraba socket, tarea y fd. Corrección aplicada en `117` líneas de diff (presupuesto 200): el plazo aborta el lector y envía una señal de cierre que `supervise` consume, con test que comprueba EOF en el servidor (`a_timed_out_request_closes_the_connection`).
- **El validador dirigido rechazó esa corrección** y encontró una regresión real (R3-REG-001): la señal de cierre no identificaba la conexión, así que un cierre viejo podía tirar una conexión nueva. Estado final de la lineage: **escalated**. Se corrigió en trabajo ordinario etiquetando la señal con la generación de la conexión, y se agregó `a_timeout_does_not_poison_the_next_request`.
- Hallazgos no bloqueantes registrados por el revisor y **no** corregidos en esta lineage (quedan como follow-ups): R1-001 (fallback de `XDG_RUNTIME_DIR` a un temporal del proceso permite suplantar el daemon), R1-002 (lectura sin tope de línea y canal de eventos sin límite), R3-002/R4-003 (no se emite evento de desconexión al cerrar el daemon, el banner queda oculto hasta el próximo sondeo), R3-003 (los smoke tests pasan si falta Xvfb), R4-001 (`call` no tiene plazo mientras se abre el socket o se escribe), R4-002 (el lector de una conexión descartada sobrevive).
- Los dos últimos R4 son consecuencia del mismo diseño y se atacan en T16 cuando el cliente se conecte a un daemon real durante la UI.

**L27 (T16 · C-041) — página Convertir y seis hallazgos cerrados:**
- **Hallazgos de la review de T15 cerrados** (commits `2b27c65`, `6b3719b` + tests `b0b8b26`, `8bd6d9e`):
  - **R1-001**: el cliente ya no cae a un temporal si falta `XDG_RUNTIME_DIR`; falla con `DAEMON_UNAVAILABLE` nombrando la variable. Test `refuses_to_guess_the_socket_path`.
  - **R1-002**: línea acotada a 1 MiB y canal de eventos acotado a 256 (descarta lo viejo); `EVENT_BUFFER` público. Tests `closes_the_connection_on_an_oversized_line`, `event_buffer_is_bounded`.
  - **R3-002/R4-003**: el lector emite `daemon.disconnected` y la ventana muestra el banner al instante, sin sondeo. Test `emits_a_disconnect_notification_when_the_daemon_closes`.
  - **R3-003**: los smoke fallan si falta `xvfb-run`/`Xvfb`, con el comando de instalación en el mensaje.
  - **R4-001**: un solo plazo cubre abrir socket, escribir y esperar; test `a_stalled_open_times_out`.
  - **R4-002**: `Drop for Connection` aborta el lector. El test no tuvo RED previo porque el camino de descarte de R3-001 ya lo cubría; queda como invariante fijado.
- **Página Convertir**: estado puro (`ConvertState`/`ConvertPhase`), expansión de carpetas de un nivel sin symlinks, resumen por archivo, `media.plan` con `reasoning[]` literal, presets desde `presets.list`, carpeta de salida desde `settings.get`, bloqueo con `sudo apt install ffmpeg` cuando faltan capacidades, spamnel de inspección, estados vacío/parcial/listo/error, accesibilidad por teclado y DnD.
- Regla dura: añadir **nunca** inicia: solo `jobs.add`; `queue.resume`/`jobs.resume` quedan para T17. Test `queue_request_is_taken_once` y `queue_request_contains_every_accepted_input`.
- RED: `unresolved import cauce_gui::pages::convert`. GREEN: 7 tests de conversión + 12 de cliente + 2 de smoke; suite **219/219**. Smoke real: `window-ready`, exit 0.
- Se habilitó `gtk4` con `features = ["v4_10"]` (única forma de tener `GtkFileDialog` y `gdk::FileList` con GTK 4.22.4; sin features posteriores).
- **Excepción de tamaño documentada**: `view.rs` (303) y `flows.rs` (358) son mutuamente dependientes (la vista llama a `flows::wire` y los flujos usan los widgets), así que entran en un mismo commit de **664** líneas. Los demás: 1, 345, 254 y 6.
- Deuda anotada: el desplegable de presets es plano (agrupar por categoría es de la página Presets, T18) y la inspección muestra un spinner de página, no uno por fila, porque `media.inspect` responde el lote completo.
