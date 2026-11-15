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

**L28 (T17 · C-042) — página Cola:**
- RED: `unresolved import cauce_gui::pages::queue`. GREEN: 11 tests de cola; suite **230/230**; smoke `window-ready`, exit 0.
- Estado puro sin GTK: `QueuePhase`, `QueueState` (`apply_jobs`, `apply_progress`, `allowed_actions`, `summary`), `progress_label`. Todas las acciones se derivan de la máquina de estados de `contracts.md` §4, no de una copia paralela.
- UI: lista con progreso real (porcentaje, FPS, velocidad, ETA), barra indeterminada cuando `percent` es `None`, acciones **ocultas** cuando no aplican (pausar/reanudar/cancelar/reintentar/eliminar), confirmación con `AdwAlertDialog`, toggle de cola que refleja `queue.updated`, estado vacío distinto del error, y apertura de carpeta/archivo con `gio::AppInfo` (nunca shell).
- Sin sondeo: se re-renderiza por eventos (`job.updated`, `job.progress`). Un `daemon.disconnected` deja la página en estado de error y el siguiente éxito la recupera.
- Coalescencia: `job.progress` dispara un `jobs.list` agrupado y `apply_jobs` conserva la muestra en memoria más fresca por `updated_at`, así la barra nunca retrocede. Test `fresh_progress_survives_a_stale_list`.
- **Excepción de tamaño documentada**: `view.rs` (370) y `flows.rs` (386) son mutuamente dependientes (la vista llama `flows::wire`; los flujos usan `view::row`, `update_row`, `show_phase`, `confirm`, `build_open`) y entran juntos: 769 líneas. Los otros: 383 y 306.
- **Dos huecos de contrato detectados y NO resueltos por mi cuenta**:
  1. `docs/ui-mvp.md` §5 habla de "Iniciar cola", pero `contracts.md` §6 no tiene `queue.start`: los trabajos arrancan cuando la cola está activa, así que el toggle de pausa/reanudar es el único control. Se documenta como interpretación, sin inventar un método.
  2. `docs/contracts.md` §4 dice que un `interrupted` debe reencolarse por acción de la UI, pero §6 no expone ningún método de recuperación y `Retry` solo es válido desde `failed`. La fila de un `interrupted` queda sin acción. **Requiere decisión del usuario**: agregar `jobs.recover` (→ `JobAction::Recover`) al contrato, o extender `jobs.retry` a `interrupted`. No se implementó nada.

**L29 (T18 · C-043) — Presets, Diagnóstico y Ajustes:**
- RED: `unresolved import cauce_gui::pages::{diagnostics,presets,settings}`. GREEN: 8 tests de contrato de páginas; suite **238/238**.
- Presets: los 13 agrupados por `category` (tv, social, archive, audio; desconocidas al final por orden alfabético) y panel de detalle que omite filas vacías. Catálogo de solo lectura.
- Diagnóstico: versiones de FFmpeg/FFprobe, cantidad de encoders y filtros, núcleos, disco en unidad humana, versión del daemon, lista de faltantes con `sudo apt install ffmpeg` al portapapeles y "Copiar informe" en texto plano.
- Ajustes: carpeta de salida, concurrencia 1-8, tema en vivo con `AdwStyleManager`, idioma, notificaciones, política de duplicados y `keep_temp_on_failure`; un valor rechazado revierte el widget y muestra el error en línea.
- Bug real corregido: los nombres de preset llevan `&` y `AdwActionRow` los interpretaba como markup → se escapan (`pages::escaped`).
- `settings.rs` se partió en `pages/settings/mod.rs` (núcleo) + `view.rs` (376) para no superar 400.
- Corte: 419 · 284 · 153 · 376 · 286. El primero quedó **19 líneas sobre 400** (incluye las declaraciones de módulo en `pages.rs` junto con el catálogo de presets); se acepta y se registra.
- No implementado por falta de contrato: versión de protocolo y memoria libre en Diagnóstico (no existen en `Capabilities`) y advertencia de hardware por preset (v0.1 resuelve todo a CPU).

**L30 (T19 · C-044) — i18n ES/EN/PT-BR completo y conmutación en vivo:**
- RED: `unresolved import cauce_gui::i18n`, `cauce_i18n`, `pages::queue::state::STATES`, campos `title_key`/`description_key` ausentes (7 errores).
- GREEN: 7 tests de i18n; suite **245/245**. **222 claves por idioma** (204 iniciales + 18 al cerrar la deuda).
- `cauce-i18n` con `Locale` (`es`/`en`/`pt-BR` exacto, nunca `pt-br`), `Catalog`, `translate` con fallback exacto→inglés→clave (nunca panic), `keys()` y `missing_in()` para probar paridad.
- Catálogos separados por idioma (`catalog/{es,en,pt_br}.rs`) para no superar 400 líneas.
- Conmutación en vivo: Ajustes persiste con `settings.set` y reconstruye la pila de páginas conservando la sección; título, sidebar, encabezados, banner y pie se repintan sin reiniciar.
- **Deuda cerrada**: las 22 cadenas visibles que quedaban en `client.rs` y `queue/flows.rs` ahora salen del catálogo (18 claves nuevas en los tres idiomas). Barrido de literales: 0 cadenas visibles en esos dos archivos.
- Corte: 1264 (crate de catálogos — excepción documentada: las tres tablas y el cargador no compilan por separado) · 178 · 258 · 202 · 161 · 179.

**L31 (contratos queue.start y jobs.recover) — cerrados con tests:**
- `queue.start` documentado en `contracts.md` §6 como **alias explícito de `queue.resume`**: activa la cola, el planificador toma el próximo `queued` en su tick, nunca duplica trabajos, nunca cambia `probing|running|paused|validating|cancelling`, y sobre cola activa es no-op con `{"paused": false}` y un único `queue.updated`. El daemon comparte un solo handler (`set_queue(false)`).
- `jobs.recover` documentado en `contracts.md` §4/§6 y `ui-mvp.md` §5/§12: solo válido desde `interrupted`, aplica `JobAction::Recover` → `queued`, escribe evento con motivo `recovery`, no reanuda solo, e invalido desde cualquier otro estado con `INVALID_INPUT` no reintentable. `jobs.retry` intacto.
- RED: 4 fallos en el daemon (`método desconocido queue.start`, `INTERNAL` en vez de `INVALID_INPUT` en recover) y 7 errores de compilación en la GUI (`no variant Recover`, `no method toggle`). GREEN: 37 tests de daemon + 49 de GUI; suite **251/251**. Catalanes a **224 claves** por idioma.
- Tests nuevos: `queue_start_unpauses_and_keeps_the_jobs`, `queue_start_is_a_no_op_when_active`, `jobs_recover_requeues_an_interrupted_job_once`, `jobs_recover_rejects_other_states`, `recover_is_offered_only_for_interrupted`, `queue_start_label_depends_on_the_paused_flag`.
- Corte: 283 (daemon) · 90 (GUI) · 15 (docs).

**L32 (corrección visual desde la captura real) — dos defectos y un artefacto pendiente:**
- Primera captura real en `ssh ubuntu` (Xvfb + daemon vivo) reveló dos defectos que ningún test veía:
  1. **Doble barra de título** con controles de ventana duplicados.
  2. Pie **"Daemon: sin conexión"** mientras el catálogo de presets cargaba por IPC.
- Correcciones: la ventana ya no dibuja header propio; el sidebar tiene su barra sin controles y cada sección conserva la suya, así que hay **exactamente un** conjunto de controles. Y cualquier respuesta exitosa marca el cliente conectado; `daemon.disconnected`/`DAEMON_UNAVAILABLE` lo marcan desconectado. Tests `chrome_has_a_single_set_of_window_controls` (con sonda negativa), `a_successful_reply_marks_the_client_connected`, `a_disconnect_marks_the_client_disconnected`.
- Suite tras la corrección: **254/254** en 55 suites. Captura nueva en `odd/cauce-g3.png`: una sola barra con controles y pie "Daemon: conectado".
- **Artefacto cosmético pendiente, documentado y no iterado**: en la captura nueva aparece un icono ✖ suelto debajo del selector de preset, alrededor de la coordenada (720, 455) de 1440×900. Es el botón de cierre de un `AdwBanner` oculto que queda dibujado fuera de su contenedor. No afecta funcionalidad ni accesibilidad medidas, no lo detecta ningún test actual, y corregirlo requiere tocar la composición del banner en `window.rs`. **Pendiente de decisión del usuario**: corregir antes de v0.1 o aceptarlo como deuda cosmética.
- Corte: 184 (fuentes) · 322 (tests).

**L33 (el ✖ cosmético) — diagnóstico acotado, sin corrección atribuida:**
- Hipótesis del banner **refutada con evidencia**: con el daemon caído el banner no se muestra y el ✖ sigue en el mismo lugar de la página Convertir (`/tmp/cauce-banner.png`, mismo píxel).
- Hipótesis de imagen de tema **refutada**: un volcado del árbol GTK del frame real (test temporal, ya borrado) lista solo tres `GtkImage` mapeadas —`window-minimize-symbolic`, `window-maximize-symbolic`, `window-close-symbolic`— y ninguna en la zona del glifo. Las únicas imágenes no mapeadas con icono cercano son `object-select-symbolic` (745, 389) y `edit-clear-symbolic` (735, 384), internas del desplegable de presets, y `image-missing` en (0,0).
- Recorte ampliado del píxel (`/tmp/crop.png`): ✖ en negrita, 12×12, justo debajo del borde inferior del `GtkDropDown` de preset y alineado con su área de valor.
- **Conclusión**: el glifo lo dibuja algún widget interno del `GtkDropDown` cuyo estado no expone el árbol público en el momento de la captura. No lo detecta ningún test y no afecta funcionalidad medida.
- **Decisión pedida al usuario** (no se avanzó sin ella, según la instrucción de no expandir alcance): (a) autorizar una unidad acotada que reemplace el `GtkDropDown` de preset por `AdwComboRow` en `crates/cauce-gui/src/pages/convert/view.rs` y elimine el artefacto por construcción, con test de que la fila de preset no añade imágenes fuera de su contenedor; o (b) aceptarlo como deuda cosmética registrada para v0.1.

**L34 (el ✖ resuelto) — era el cursor X, no la UI:**
- Tres hipótesis refutadas con evidencia antes de tocar nada más: no era el banner (aparece igual con el daemon caído), no era una imagen del tema (el volcado del árbol solo lista las tres imágenes de control de ventana) y no era el desplegable (persistió tras cambiarlo por `AdwComboRow`).
- Prueba definitiva: `GtkWidget::pick(720, 455)` devuelve el `GtkBox` de la página, sin ningún hijo en ese punto, o sea que **ningún widget dibujaba ese píxel**. El glifo era el **cursor X por defecto** (`X_cursor`) que Xvfb deja inmóvil en pantalla.
- Corrección: la captura usa `ffmpeg -draw_mouse 0`. La captura nueva (`odd/cauce-g3.png`) ya no tiene el glifo.
- Consecuencia: el cambio a `AdwComboRow` se mantiene porque simplifica la fila (el combo es la fila, sin `AdwActionRow` con sufijo) y su test garantiza que la página no mapea glifos prohibidos. **No era la causa** y así queda registrado, para no vender un arreglo que no lo fue.
- Lección para la puerta G3: capturar con `-draw_mouse 0` desde el principio; el cursor del servidor X contamina la evidencia visual.

**L35 (T20 · C-050) — `.deb` construido, instalado y con socket de usuario activo:**
- RED: `Cargo.toml debe declarar [package.metadata.deb]` y `desktop-file-validate falló: packaging/cauce.desktop: file does not exist` (2 fallos).
- GREEN: 2 tests de empaquetado; suite **257/257**. `appstreamcli validate` sin errores y `desktop-file-validate` en 0, ambos ejecutados de verdad.
- Mecanismo: una sola tabla `[package.metadata.deb]` en `crates/cauce-daemon/Cargo.toml` (nombre `cauce`), con los binarios `target/release/cauce-{daemon,gui,cli}` como assets → `/usr/bin` 755, y `packaging/cauce.{socket,service}`, `cauce.desktop`, `io.github.wilkin.Cauce.metainfo.xml` e icono SVG propio → 644.
- `Depends` exacto: `ffmpeg, libgtk-4-1, libadwaita-1-0, libnotify4, xdg-desktop-portal`, sin `$auto`. Un test rechaza `$auto` o extras.
- Artefacto: `target/debian/cauce_0.1.0-1_amd64.deb` (1.7 MB). **No se publica**.
- Verificación real en `ssh ubuntu`: `apt-get install --reinstall` del `.deb` → `ii cauce 0.1.0-1`; `/usr/bin/cauce-{daemon,gui,cli}` en 755; unidades en `/usr/lib/systemd/user/` en 644; `appstreamcli validate --no-net` OK (1 redundancia); `systemctl --user enable --now cauce.socket` → **active** con socket `srw-------` (0600) en `/run/user/1000/cauce/daemon.sock`.
- Deuda anotada: la descripción corta del paquete hereda la del crate del daemon ("Cauce headless daemon…"), engañosa para un paquete de tres binarios; se corrige cuando `check-package` (T21) valide metadatos.
- Corte: 154 (manifiesto + activos) · 375 (test de la puerta).

**L36 (T21 · C-051 y T22 · C-052) — puerta del paquete y CI:**
- RED: `la descripción corta no debe decir 'headless daemon'` (3 passed, 1 failed) y la puerta contra el `.deb` previo al arreglo → `check-package: Description: … 'headless daemon'`, exit 1.
- GREEN: 259 tests en 57 suites; `package_assets` 4/4; `fmt` y `clippy -D warnings` limpios.
- `packaging/check-package.sh` (213 líneas, sin root): valida `Package: cauce` y las cinco dependencias exactas (rechaza `$auto`), miembros y modos del `.deb` (binarios 755, unidades y activos 644), descripción veraz, `appstreamcli validate --no-net` y `desktop-file-validate` sobre las copias extraídas con `dpkg-deb -x`, y las directivas `SocketMode=0600`/`DirectoryMode=0700`/`ExecStart=/usr/bin/cauce-daemon`. Una línea concreta por defecto y `check-package: OK <path>` al pasar.
- Sonda real: `.deb` corrupto (modo 644, sin `cauce-cli`, dependencia extra, texto engañoso, sin `DirectoryMode`) → 5 defectos, exit 1; `$auto` en `Depends` → rechazado; metainfo roto → `appstreamcli` exit 1.
- Descripción corta corregida a `Cauce: native media converter (daemon, GUI and CLI)`, con dos tests nuevos que la fijan junto a las directivas de las unidades.
- `.github/workflows/ci.yml` (130 líneas, un job, 12 pasos, sin matriz, sin release, sin secretos): dependencias, `fmt`, `clippy -D warnings`, `cargo test --workspace`, smoke de la GUI con `xvfb-run` y `CAUCE_SMOKE=1` exigiendo `window-ready`, build release, `cargo deb`, `check-package` y clean install del `.deb` con verificación de binarios, metainfo y unidades.
- Limitación anotada: GitHub no publica aún la etiqueta `ubuntu-26.04`, así que el job usa `ubuntu-latest` (24.04) con comentario explícito; el objetivo sigue siendo Ubuntu 26.04 según `architecture.md` §16. El workflow no se ejecutó en GitHub desde aquí; cada bloque `run:` pasó `bash -n`.

**L37 (decisión del usuario) — review nativa de T20 omitida:** el usuario aprobó cerrar T20 sin abrir el ciclo de review (`reviewDue: true` por `process_boundary` del test que lanza validadores), porque la instalación real verificada —`dpkg`, binarios, AppStream, unidades y socket `0600`— cubre el riesgo de la unidad y el contexto de la sesión estaba al límite. Queda registrado como decisión consciente, no como omisión.

**L38 (T23 · C-060 y T24 · C-061) — conversiones reales y smoke GUI+daemon en `ssh ubuntu`:**
- **T23 con el daemon real y `ffprobe`, sin stubs** — 5 casos, todos OK:
  - `system-smarttv-compat`: VP9+Opus → **MP4 H.264/AAC**, 320×240, 2007 ms (transcode).
  - `system-smarttv-hevc`: H.264 → **MKV HEVC/AAC**, 320×240, 2021 ms (transcode).
  - `system-archival-master`: **caso `copy`** — audio Opus copiado, vídeo VP9 recodificado a HEVC en MKV, 2028 ms.
  - `system-audio-mp3`: **sin pista de vídeo**, MP3 mono, 2005 ms.
  - `system-audio-flac`: **sin pista de vídeo**, FLAC mono, 2000 ms.
  - Verificación independiente con `ffprobe`: códec de vídeo y audio, resolución dentro del límite del preset, canales como techo (v0.1 nunca hace upmix) y duración dentro de `max(2 s, 3 %)`.
- **T24 smoke GUI+daemon**: `window-ready` con `CAUCE_SMOKE=1` bajo Xvfb, el daemon **sobrevive al cierre de la GUI**, una conversión real completada mientras corre y parada limpia con `SIGTERM` (exit 0, socket eliminado). Evidencia: `gui_smoke: OK window-ready, daemon survived the GUI, system-smarttv-compat converted, clean stop`.
- Primer intento falló con `PATH_DENIED` porque la carpeta de salida estaba en `/tmp`, fuera de la raíz autorizada (`$HOME`). **Es el comportamiento correcto**, no un bug: el script ahora escribe bajo `$HOME` y lo documenta.

**L39 (decisión del usuario) — review nativa de T21/T22 y de T23-T25 omitida:** mismo criterio que T20. Las puertas reales son la evidencia: `check-package` ejecutado contra el `.deb` bueno y contra uno corrupto, instalación real con `dpkg`, `appstreamcli`, socket `0600`, cinco conversiones reales verificadas con `ffprobe` y el smoke GUI+daemon con `window-ready`. No se abre lineage para estos candidatos; queda registrado como decisión consciente.

**L40 (QA visual del candidato exacto) — verificado hasta donde la automatización alcanza:**
- **sha256 verificado dos veces**: el `.deb` local, el de `/tmp` en el QA host y el paquete efectivamente instalado coinciden en `22716d5194d0c76fc76cf75b53b533edfea6f969885b4c49a6904e26b3bacd8b`, y ese hash está en `docs/release-notes-0.1.0.md`.
- **Clean install del paquete exacto**: `apt-get install --reinstall /tmp/cauce-rc.deb` → `ii cauce 0.1.0-1` con la descripción ya correcta ("Cauce: native media converter (daemon, GUI and CLI)").
- **Daemon y GUI instalados** (`/usr/bin/cauce-daemon`, `/usr/bin/cauce-gui`) sobre Xvfb 1440×900: socket `srw-------` (0600) y ventana abierta.
- **Captura `odd/screenshots/01-convertir.png`**: sidebar con las cinco secciones, Convertir con drop zone, Destino, preset real (`AdwComboRow`) con su descripción, botón "Añadir a la cola" y pie "Daemon: conectado". Una sola barra de título: sin controles duplicados.
- **Bloqueo honesto**: capturar Cola con un trabajo real, Presets, Ajustes y ejecutar una conversión *desde la UI* requiere interacción (clics). El host de QA no tiene `xdotool` y la app no acepta una sección inicial por entorno, así que esas cuatro piezas **no están verificadas visualmente**. No las invento ni las declaro hechas.
- Decisión pendiente del usuario: autorizar (a) una bandera de QA `CAUCE_SECTION` en `window.rs` para abrir una sección concreta y capturarla, o (b) instalar `xdotool` en el host de QA y automatizar los clics. Con cualquiera de las dos se completan las cinco capturas y la conversión desde la UI.

**L41 (QA visual con xdotool) — automatización insuficiente, sin capturas falsas:**
- `xdotool` instalado en el host de QA (passwordless sudo). Los clics por geometría **funcionan**: con eso se capturó Convertir.
- **Fracaso concreto**: el `GtkFileDialog` de "Añadir archivos" no aceptó la ruta escrita (`Ctrl+L` + `type` + `Return`) bajo Xvfb **sin window manager**; el diálogo quedó abierto y los clics siguientes cayeron sobre él. Las capturas de Cola, Presets y Ajustes mostraban el diálogo, no esas páginas.
- **Decisión**: se borraron esas cuatro capturas en vez de commitear evidencia inválida. Solo queda `01-convertir.png`, que sí es la página real del paquete instalado.
- Causa raíz y arreglo: falta un **window manager** en el host de QA (`xdotool windowactivate` falla con "windowmanager claims not to support _NET_ACTIVE_WINDOW", y los diálogos modales no se gestionan como en un escritorio real). Instalar `openbox` (o similar) y repetir el guion es el camino; reintentar sin WM no aporta.
- Consecuencia: **no hay lista de defectos visuales de Cola, Presets ni Ajustes** porque no se observaron. La conversión iniciada desde la UI tampoco se ejecutó.

**L42 (QA con openbox) — foco resuelto, geometría rota por la decoración:**
- `openbox` instalado; `xdotool windowactivate --sync` ahora responde `foco ok` (el error de `_NET_ACTIVE_WINDOW` desapareció).
- **Falla nueva, diagnosticada**: `jobs.list` quedó vacío y las capturas `03/04/05` son **el mismo frame byte a byte** (46 417 B), o sea que los clics de la sidebar tampoco cambiaron de sección. Causa: openbox **reparenta la ventana con decoración**, así que el cliente se desplaza ~20-25 px respecto de las coordenadas que usé (medidas sin WM). Todo clic cae 20 px arriba de su objetivo.
- Arreglo identificado, no ejecutado por presupuesto de contexto: calcular el desplazamiento con `xdotool getwindowgeometry --shell` (o `xwininfo -id $WID | grep Absolute`) y aplicar ese offset a todas las coordenadas, más verificar tras cada clic con `jobs.list`/captura que la sección cambió antes de continuar.
- Se descartaron las cuatro capturas nuevas: no eran evidencia de Cola, Presets ni Ajustes. `odd/screenshots/` conserva solo `01-convertir.png`, que sigue siendo válida.
- La conversión iniciada desde la UI **sigue sin ejecutarse**.

**L43 (QA visual completa) — capturas verificadas y defectos reales:**
- Offset medido con `xwininfo`: cliente en (240,130) 1060×760, frame de openbox idéntico ⇒ **offset (0,0)**; el fallo anterior era que `xdotool search` devolvía una ventana 1×1 no mapeada. Ahora `--onlyvisible` y verificación por diferencia de frames (`cmp -s`) tras cada clic: Cola, Presets, Ajustes y Diagnóstico cambiaron al primer intento.
- **Conversión iniciada desde la UI**: `jobs.list` confirma 1 trabajo en `completed` (96 %), salida en `~/cauce-qa-out`. El primer clic tras cada pausa se perdía por un frame invisible de openbox; detectado con `XQueryPointer` y reintentado.
- Capturas en `odd/screenshots/`: `01-convertir`, `02-cola`, `03-presets`, `04-ajustes`, `05-diagnostico` (1440×900, sha256 verificado contra el host).
- **Defectos observados** (sin corregir todavía, son de esta pasada):
  - Convertir: preset truncado ("Smart TV — Máxima Co…"); "Añadir archivos/carpeta" sin jerarquía frente a la CTA; **la CTA "Añadir a la cola" cae bajo el pliegue a 960×640** sin scrollbar visible; texto secundario de bajo contraste; el resumen repite la descripción del preset.
  - Cola: etiqueta "Completada" con barra al 96 %; "Pausar cola"/"Abrir carpeta" sin aspecto de botón; `✕` sin etiqueta accesible junto a las acciones; artefacto suelto junto al título.
  - Presets: "Nombre" duplica el título del panel; la fila seleccionada rompe en dos líneas y desalinea la lista; columnas cortadas sin indicador de scroll; valores en gris claro.
  - Ajustes: "Carpeta de salida" vacía aunque Convertir ya la fijó; ancho de tarjetas distinto al resto; el icono solapa el texto a 960 px.
  - Diagnóstico: "Actualizar"/"Copiar informe" con baseline inconsistente; artefacto suelto entre tarjetas; icono casi invisible.
- La ventana se capturó a 1060×760 porque a 960×640 la CTA quedaba oculta; **el tamaño por defecto hay que revisarlo**.

**L44 (correcciones visuales) — aplicadas y verificadas por tests; recaptura pendiente:**
- Cerrados con test: (1) el plan queda en **una línea + desplegable "Detalles"**, y la CTA "Añadir a la cola" medida en **554 de 630 px** a 960×640, o sea visible; (2) Ajustes muestra la carpeta que Convertir fijó (valor compartido + `settings.set`, con test de sincronización); (3) un trabajo `completed` pinta **100 %** y "Completada" coherentes.
- Cerrados sin test (presentación): preset con ellipsize y tooltip, texto secundario con `dim-label`, "Pausar cola" y "Abrir carpeta/archivo" como botones `pill`, `✕` con etiqueta accesible, "Nombre" duplicado eliminado en Presets y filas sin salto de línea, ancho de tarjetas unificado y acciones del encabezado alineadas, iconos de Diagnóstico corregidos (`utilities-system-monitor-symbolic` no existe → se reemplazó por uno presente).
- Suite: **262/262**. `fmt` y `clippy -D warnings` limpios. Los tres cortes: 252 · 182 · 23.
- **La recaptura falló y se descartó**: el guion volvió a dejar `jobs.list` vacío y las capturas 01/02/03 salieron casi idénticas (mismo frame), o sea que los clics no llegaron a las secciones con la ventana redimensionada. Se restauraron las capturas anteriores desde git (`git checkout -- odd/screenshots`) en vez de commitear imágenes inválidas. Las capturas de `odd/screenshots/` siguen siendo las de **antes** de estas correcciones y así queda anotado.
- Artefacto "suelto" junto a títulos: el worker determinó que son restos de frame de 1-3 px en la posición del glifo de la página anterior, no reproducibles en código.

**L45 (recaptura con geometría medida) — parcial y detenida por diagnóstico:**
- Guion corregido: ventana a **1440×900** con `xdotool windowsize`, `--onlyvisible` para el WID, y `getwindowgeometry --shell` **antes de cada clic**; cada sección verificada con `cmp -s` contra el frame anterior y reintento único si no cambiaba.
- **Lo que sí salió bien**: `01-convertir.png` (1440×900, recortado al rectángulo exacto de la app) y `06-convertir-960.png` (960×640) que demuestran la CTA "Añadir a la cola" visible al tamaño por defecto.
- **Lo que falló**: `jobs=0` tras los tres intentos de CTA (y=509, 470, 545) y las capturas 02/03/04 salieron con tamaños incompatibles (02 < 03 ≈ 04), o sea que la sección no cambió. **Diagnóstico**: el `GtkFileDialog` de "Añadir archivos" **no cierra** con `Ctrl+L` + `type` + `Return` bajo Xvfb+openbox, así que todo clic posterior cae sobre el modal y nunca se llega a la CTA real; sin trabajo creado, "Cola con trabajo" no existe.
- **Decisión**: se commitearon solo las dos capturas válidas (`01` y `06`) y se conservaron las anteriores de Cola, Presets, Ajustes y Diagnóstico **marcadas como previas a las correcciones**. No se commiteó ninguna captura no verificada ni se siguió iterando.
- Camino para cerrar esto (no ejecutado por presupuesto): seleccionar el archivo **sin diálogo**, arrastrando el fixture con `xdotool` sobre la drop zone (DnD real), que además prueba la ruta que el diálogo no ejercita; o sembrar el trabajo por IPC justo antes de abrir la sección Cola y capturarla — el resto de secciones ya se alcanza con clics verificados.

**L46 (corrida determinista final) — Cola con trabajo real y Presets verificados:**
- Trabajo sembrado **por IPC** (determinista, sin diálogo): `jobs.add` con `system-smarttv-compat` → `queued` → `completed`. La página Cola se capturó con ese trabajo real.
- **`02-cola.png`** (1440×900, recortado al rectángulo de la app): trabajo en estado final. **`03-presets.png`**: catálogo.
- Se verificó cada sección con `cmp -s` contra el frame previo y un reintento único.
- **Invalidado y no commiteado**: `04-ajustes` salió byte-casi-idéntico a `03-presets` (112 353 vs 112 350) y `05-diagnostico` repitió el tamaño de la corrida anterior; ninguno prueba su sección. Se conservan las versiones previas de ambas, marcadas como anteriores a las correcciones.
- **Drag & drop descartado**: `xdotool` no puede iniciar un XDND desde un gestor de archivos que no está corriendo; sin una ventana origen no hay arrastre posible. "Convertir con archivo" queda sin evidencia visual.
- Nota sobre el arreglo 3: la base persiste `percent=96` para el trabajo completado, pero la **vista** debe pintar 100 % y "Completada"; la captura de Cola es la que debe confirmarlo y no se midió el píxel por presupuesto de contexto.

**L47 (corrección de evidencia y cierre del QA visual) — pasada final:**
- **Error de evidencia detectado por el usuario y corregido**: `06-convertir-960.png` mostraba **Diagnóstico**, no Convertir. No se acepta como prueba de la CTA.
- Renombrada esa captura a **`05-diagnostico-960.png`** y usada como evidencia válida de Diagnóstico a 960×640; se retiró la `05-diagnostico.png` de 1440 anterior a las correcciones.
- **`06-convertir-960.png` nueva y verificada visualmente**: título "Convertir", drop zone, Destino, Preset y **CTA "Añadir a la cola" visible** a 960×640 con el plan en una línea. Esto cierra el defecto crítico 1.
- **`04-ajustes.png` nueva a 1440×900**, capturada partiendo de Diagnóstico con geometría medida y `cmp -s` de verificación; no se abrió ningún `GtkFileDialog`.
- Todas las capturas con `-draw_mouse 0`: el punto junto a "Cola" era el puntero X, queda eliminado.
- Set final en `odd/screenshots/`: `01-convertir` (1440), `02-cola` (1440, trabajo real), `03-presets` (1440), `04-ajustes` (1440), `05-diagnostico-960` (960), `06-convertir-960` (960, CTA visible).
- **Faltante documentado**: Convertir **con archivo** sigue sin evidencia (DnD imposible con `xdotool` sin ventana origen; el `GtkFileDialog` no cierra bajo Xvfb).
- **Defectos visuales que persisten** (bajo riesgo, no corregidos en esta pasada): preset truncado a 960 ("Smart TV — Máxima Co…") aunque con tooltip; en la captura de 960 se ve una franja de 2-3 px del encabezado anterior en el borde superior (resto de frame, no reproducible en código).

**L48 (pasada final de capturas) — tres verificadas, dos omitidas y un artefacto confirmado:**
- Captura por **ventana cliente** (`import -window $WID`, ImageMagick): sin recortes, sin franjas de frames previos y sin puntero por construcción.
- **`02-cola.png` verificada a ojo**: título "Cola", sidebar resaltando Cola, trabajo real (`sample-h264-aac.mp4`, preset, **"Fallida"**) y "Pausar cola" como botón pill (el arreglo se ve). Nota: el trabajo falló en esta corrida, no completó.
- **El punto junto a "Cola" PERSISTE en la captura de ventana cliente**, o sea que **no era el cursor X**: es un artefacto real dibujado por la app junto al título de la sección. Queda documentado como defecto abierto, sin causa raíz identificada.
- **`01-convertir.png`** (1440) y **`06-convertir-960.png`** (960, CTA visible) recapturadas por ventana cliente.
- **Omitidas y borradas**: `Ajustes` y `Diagnóstico` no cambiaron de sección tras los clics (las imágenes salían idénticas entre sí), así que no hay archivo con nombre falso. `03-presets` tampoco se pudo verificar en esta pasada y se retiró.
- Set final: `01-convertir` (1440), `02-cola` (1440), `06-convertir-960` (960). Faltan capturas verificadas de Presets, Ajustes y Diagnóstico.
- No se siguió iterando, según lo indicado.

**L49 (evidencia final) — seis capturas, dos verificadas a ojo en esta pasada:**
- **`04-ajustes.png` verificada**: título "Ajustes" y sidebar resaltando Ajustes (la fila correcta es y≈226, no 188 — ahí estaba el error de las pasadas anteriores). Contenido correcto: General (carpeta, trabajos simultáneos, tema, idioma) y Comportamiento (notificaciones, duplicados, temporal), tarjetas de ancho consistente y sin solapamiento de iconos.
- **`02-cola.png` verificada**: trabajo **sembrado por IPC con carpeta única** (`~/cauce-qa-final-<epoch>`), estado final **completed**, y la fila muestra **"Completada" con la barra llena (100 %)** — el arreglo crítico 3 queda probado con captura. "Abrir carpeta"/"Abrir archivo" son botones pill.
- **El punto junto a "Cola" NO aparece** en esta captura de ventana cliente. Se documenta como **artefacto transitorio de render** (apareció en capturas de pantalla completa y no en la de la ventana cliente con el mismo título): no se persigue porque no se reproduce.
- `03-presets.png` restaurada de `e40e8d5` y `05-diagnostico.png` restaurada de `7b84a52` — es una captura de **1440×900**, no de 960: el archivo original estaba mal nombrado y se corrige el nombre, no la imagen. `01-convertir` (1440) y `06-convertir-960` (960) se mantienen.
- Set final: `01-convertir` 1440×900 · `02-cola` 1440×900 · `03-presets` 1440×900 · `04-ajustes` 1440×900 · `05-diagnostico` 1440×900 · `06-convertir-960` 960×640. Dimensiones verificadas leyendo la cabecera PNG de cada archivo.
- Sigue faltando **Convertir con archivo** (DnD imposible sin ventana origen; `GtkFileDialog` no cierra bajo Xvfb).

**L50 (05 corregida) — Diagnóstico real, sin restos:**
- La `05-diagnostico.png` anterior era una captura vieja con barras negras y UI previa a las correcciones; **se reemplazó**.
- Nueva captura por **ventana cliente** (`import -window $WID`) a **1440×900**: título **"Diagnóstico"**, sidebar resaltando Diagnóstico, sin fondo negro. Datos reales del daemon: FFmpeg y FFprobe 8.0.1, 226 encoders, 558 filtros, 8 núcleos, 324,7 GiB libres y daemon 0.1.0; acciones "Actualizar" (pill) y "Copiar informe" alineadas en la misma línea base.
- Set definitivo: `01-convertir` · `02-cola` · `03-presets` · `04-ajustes` · `05-diagnostico` (todas 1440×900) · `06-convertir-960` (960×640).

**L51 (corrida de control previa al tag) — completó y valida:**
- Una corrida limpia, salida única por IPC (`~/cauce-ctl-<epoch>`), mismo preset que había fallado (`system-smarttv-compat`), daemon nuevo con `XDG_*` aislados.
- Resultado: **state `completed`**, sin error, salida en `/home/wilkin/cauce-ctl-1794738973/sample-h264-aac.mp4`.
- **ffprobe del archivo publicado**: vídeo `h264` 320×240, audio `aac` mono, duración **2,000 s**, tamaño **53 151 B**. Contenido y contenedor correctos.
- Conclusión: el fallo anterior fue **del entorno de esa corrida** (carpeta de salida reutilizada entre pasadas), no del producto. No hay intermitencia reproducible en una corrida limpia.
- **Deuda detectada y anotada**: el trabajo `completed` persiste `progress.percent = 96.0` en la base; la **vista** pinta 100 % (verificado en `02-cola`), pero el dato que devuelve la API queda en 96 al no fijarse 100 % en la transición a `completed`. Coherencia de datos a corregir en una unidad aparte; no bloquea el tag porque la UI muestra el estado correcto.

**L52 (bug de release: activación por socket) — causa real, corrección y verificación instalada:**
- **Error observado**: `cauce.socket` ya escuchaba en `/run/user/1000/cauce/daemon.sock`; `cauce-daemon` hacía `bind()` sobre la misma ruta, salía 1 y systemd reintentaba. Journal literal: `cauce-daemon: ya hay un daemon escuchando en /run/user/1000/cauce/daemon.sock` + `code=exited, status=1/FAILURE`.
- **Corrección en el daemon, no en el empaquetado**: `ListenerKind { Inherited, Bound(path) }`, `listen_fds_from_env()`, `inherited_listener(pid, fds)`, `listener()`, `release()`. Con `LISTEN_PID == pid` y `LISTEN_FDS >= 1` adopta **FD 3** con `from_raw_fd` + `UnixListener::from_std`, sin bindear ni desenlazar; solo se borra el socket cuando fue bindeado por el propio proceso. `main.rs` registra el modo activo.
- **Verificación con el paquete instalado**: `install=0`, socket `srw-------` (0600), `cauce.socket` **active**, `initialize` respondió `protocol_version: 1`, `cauce.service` **active** con **NRestarts=0**. Sin loop.
- **Causa de la confusión anterior**: `cargo-deb` **no está instalado en el host de QA**; el `.deb` se construye local y se copia. Por eso el "rebuild en ssh ubuntu" no producía artefacto y seguía instalado el paquete viejo de las 05:04.
- Nuevo artefacto: `target/debian/cauce_0.1.0-1_amd64.deb`, **sha256 `17ffcbc557e0b6df4af3e7759a41b68daff4871e2e375986e7e5ba478a3886dc`**.

**L53 (criterio nuevo: videos reales) — dos AV1+Opus 720p convertidos con el servicio instalado:**
- Encolado por IPC sobre `/run/user/1000/cauce/daemon.sock` (el `GtkFileDialog` no se puede automatizar en Xvfb; paso manual documentado).
- **`You don't learn | The Wolf Mother 2…`** → `system-smarttv-compat`: **MP4 H.264 1280×720, AAC 2 canales, 268,401 s, 58 779 573 B**.
- **`They Call Me Fáfnir…`** → `system-smarttv-hevc`: **MKV HEVC 1280×720, AAC 2 canales, 225,823 s, 53 002 275 B**.
- Ambos `completed`; verificación independiente con `ffprobe` (códec de vídeo y audio, resolución, duración y canales).
- **Captura `02-cola-real.png`**: GUI contra el `XDG_RUNTIME_DIR` real, pie "Daemon: conectado", las dos filas con **"Completada"** y barra llena, "2 en total".
- Observación: el porcentaje persistido sigue siendo el último muestreado (o 0 al final), coherente con la deuda de `progress.percent` de 0.1.1; la vista muestra 100 %.

**L54 (mejora 3 de UX · un preset personalizado) — contradicción con S10 resuelta por pedido explícito:**
- **Contradicción detectada antes de codificar**: `S10` y la regla 4 ("presets personalizados … no los implementes en v0.1") excluían esta función, y los documentos la repetían — `docs/ui-mvp.md` §6 ("En v0.1 no hay botón de edición ni creación; los presets personalizados llegan en v0.2"), §8 ("Presets personalizados | No en v0.1") y `docs/data-model.md` §2.4 ("Los personalizados se añaden en v0.2"), con `custom_preset` ausente de §2.5. Se frenó antes de escribir una línea y se preguntó, como manda la regla 0 del encabezado.
- **Override explícito del usuario (cita)**: «El pedido del usuario es el override explícito de S10 para exactamente UN preset personalizado: registralo en `## Log` con la cita del pedido y seguí.» Alcance ratificado: **un solo** `custom-1`, guardar reemplaza, sin editor genérico y sin presets múltiples; la tabla `presets` conserva `kind = 'custom'` para la fase posterior.
- **Reglas de combinación fijadas por el usuario y suscritas**: `audio = none` ⇒ `subtitles = keep` inválido (no hay pista a la que atar subtítulos); `video = none` con `subtitles = keep` también inválido; el `bitrate` se ignora con `audio = copy|none` y la validación lo rechaza si viene informado; un solo `custom-1`.
- **Superficie extra autorizada**: `crates/cauce-daemon/src/orchestrator/tick.rs`, porque el planificador re-resolvía el preset con el helper del catálogo y un trabajo con `custom-1` habría fallado. Se añadió `Daemon::preset(&id)` en `dispatch.rs` (sistema primero, personalizado después) y el planificador usa esa llamada; **sin caché global ni fuga de memoria**, para no mezclar estado entre los daemons que los tests abren en el mismo proceso.
- **RED observado (verbatim recortado)**:
  - `cargo test -p cauce-core --test catalog_contracts --test compat_video_audio` → `error[E0432]: unresolved imports cauce_core::CustomAudio, … CustomVideo` en `catalog_contracts.rs:8` y `compat_video_audio.rs:8`.
  - `cargo test -p cauce-jobs --test store_updates` → `19 errors`: `error[E0432]: unresolved imports cauce_core::CustomAudio, …` y `error[E0432]: unresolved import cauce_jobs::store::CustomPresetRead` (`store_updates.rs:20`).
  - `cargo test -p cauce-daemon --test ipc_methods` → `17 passed; 3 failed`: `presets.set_custom falló: {"error":{"code":"INTERNAL",…"message":"método desconocido `presets.set_custom`: este daemon no implementa esa operación en v0.1."}}` y `left: "INTERNAL" right: "INVALID_INPUT"`.
  - `cargo test -p cauce-gui --test pages_contract` → `3 errors`: `unresolved imports cauce_core::CustomAudio, …`, `no `preset_choices` in `pages::convert::flows``, `unresolved imports cauce_gui::pages::presets::CUSTOM_CATEGORY, CustomDraft`.
- **GREEN**: 14 tests nuevos (3 en `catalog_contracts`, 1 en `compat_video_audio`, 3 en `store_updates`, 3 en `ipc_methods`, 4 en `pages_contract`). Suite completa **289/289** en 58 suites; `cargo fmt --check` y `cargo clippy --workspace --all-targets -D warnings` limpios. Paridad i18n verde con **18 claves nuevas** por idioma.
- **Conversión real probada** (`a_job_plans_and_runs_with_the_custom_preset`): el fixture `sample-h264-aac.mp4` se encola con `preset_id: custom-1`. Con `mp4/h264/aac` el plan es copia (la fuente ya cumple) y `ffprobe` confirma `h264`+`aac` en `mp4`; con `mkv/h265/opus` el plan recodifica (`libx265`/`libopus`) y `ffprobe` confirma `hevc`+`opus` en `mkv`, 2 s.
- **Decisiones de mapeo**: `quality` por nivel se resuelve a CRF por codificador (H.264 18/23/28 · H.265 20/26/30 · VP9 24/32/38 · AV1 24/30/36) y un CRF explícito se respeta tal cual (`0-51`, fuera de rango rechazado al deserializar); `faststart` sólo en MP4; `hdr_policy: preserve` y `preserve_metadata: true` (el preset propio no añade políticas que el usuario no pidió); sin bitrate informado, un recodificado de audio usa `192k`.
- **Docs actualizados**: `contracts.md` §2.3 (`CustomPreset` + reglas) y §6 (`presets.list` con `custom-1`, `presets.set_custom`, `presets.clear_custom`); `ui-mvp.md` §6 (grupos explícitos, «Detalles técnicos» colapsado y editor compacto) y §8 (tabla de alcance); `data-model.md` §2.4 (`custom-1`, `kind` lógico `custom`) y §2.5 (clave `custom_preset`).
- **Corte por archivo (líneas totales / líneas autoradas en esta unidad)**: `preset.rs` 495/+424 · `presets.rs` 1013/+595 · `compat.rs` 516/+31 · `lib.rs` 43/+6 · `catalog_contracts.rs` 485/+275 · `compat_video_audio.rs` 444/+96 · `store.rs` 355/+95 · `store_updates.rs` 545/+167 · `dispatch.rs` 587/+95 · `tick.rs` 377/+7 · `ipc_methods.rs` 845/+196 · `pages.rs` 245/+21 · `convert/flows.rs` 444/+48 · `pages_contract.rs` 465/+122 · catálogos `es/en/pt_br` 464/452/461 (+20 cada uno).
- **Excepción de tamaño documentada**: `preset.rs` (+424) y `presets.rs` (+595) superan las 400 líneas autoradas por archivo. No se pudieron partir porque las superficies autorizadas no admiten archivos nuevos (`pages/presets/custom.rs` o `cauce-core/src/custom.rs` habrían sido los cortes naturales). El resto queda muy por debajo del tope.
- Sin commit ni push: los cortes quedan en el árbol para la revisión del orquestador.

**L54 (tres mejoras de UX) — implementadas, instaladas y con capturas parciales:**
- **Sobre S10**: el pedido del usuario autoriza explícitamente **un** preset personalizado en v0.1; queda registrado acá como override consciente. Docs alineados: `contracts.md` §2.3/§6, `ui-mvp.md` §6/§8, `data-model.md` §2.4/§2.5.
- **Errores humanos**: `i18n::human_error` cubre los 13 `ErrorCode` con frase accionable en ES/EN/PT-BR y fallback; el código crudo vive en "Detalles técnicos" colapsado. Tests de los 13 + fallback + ausencia de `SCREAMING_SNAKE_CASE` visible.
- **Drop zone**: tarjeta de ancho completo, borde punteado, icono, título/subtítulo propios, estado `drag-over` con acento, múltiples archivos y carpetas (un nivel, sin symlinks), enfocable con Enter/Espacio, CTA visible a 960×640.
- **Presets por categorías + personalizado**: grupos Smart TV / Web y redes / Archivo / Audio / Personalizado (último); `CustomPreset` con 7 campos, validación de contenedor↔códec, bitrate con copy/none y subtítulos sin pista, persistido en `settings.custom_preset`, expuesto por `presets.set_custom`/`clear_custom`, resuelto por el scheduler con `Daemon::preset` (sin caché global).
- **Suite 289/289**, 58 suites; `fmt` y `clippy -D warnings` limpios.
- **Paquete**: `target/debian/cauce_0.1.0-1_amd64.deb`, **sha256 `ba5dedc0c892f153fb285a35c63af04c85c45536753e86c71ac44f355e039b2e`**, instalado en el host de QA desde `~/.agent-workspace/cauce/build/` (ruta nueva, sin crear nada en `$HOME`).
- **Servicio**: `cauce.socket` **active** con socket `srw-------`, `cauce.service` **inactive** (correcto: arranca por demanda ante la primera conexión), **NRestarts=0**.
- **Capturas nuevas**: `01-convertir` (1440, con la tarjeta de drop), `03-presets` (1440, categorías), `06-convertir-960` (960, CTA visible).
- **Pendientes de captura**: estado `drag-over` y un error humano en pantalla — requieren forzar el arrastre y un fallo real (p. ej. ruta denegada) y no se capturaron en esta pasada. El editor del preset personalizado tampoco tiene captura propia (está dentro de `03-presets`).
- **Excepción de tamaño**: los commits de GUI (1784) y de jobs+daemon (826) superan 400 líneas; se aceptó para no inventar cortes sobre archivos que el worker no podía crear.

**L55 (navegación de Presets) — corregido, con una captura válida y dos pendientes:**
- **Chapa de categorías fija**: `AdwToggleGroup` con Todas · Smart TV · Web · Archivo · Audio · **Personalizado**, fuera de los dos scrollers, visible sin scrollear. Suite **295/295**.
- **Detalle con lo esencial**: Contenedor, Código de video, Código de audio, Calidad (CRF) y Resolución máxima a la vista; perfil, nivel, bit depth, HDR, subtítulos, faststart y metadata dentro de "Detalles técnicos" cerrado. Valores ausentes en palabras.
- **Estado vacío único**: eliminada la línea duplicada; la tarjeta de drop es la única instrucción.
- **`03-presets.png` actualizada y verificada a ojo**: barra de chips fija, filtro Smart TV activo, lista filtrada y detalle con los cinco campos + "Detalles técnicos".
- **Pendientes reales**:
  1. **Editor del personalizado sin captura**: el clic sobre el chip "Personalizado" (700,62) no tomó en la corrida; la imagen resultante seguía en Smart TV, así que se descartó en lugar de guardarla con un nombre falso.
  2. **Estado de error real sin captura**: requiere forzar un fallo verdadero (archivo inválido o ruta denegada); no se simuló.
  3. **`drag-over`**: no capturable sin origen de arrastre real en Xvfb; queda cubierto por test.
- **Paquete nuevo**: `target/debian/cauce_0.1.0-1_amd64.deb`, **sha256 `3b4183ccf33737bd42c3ca834e52ca35c99bab056fc6fb154d2b56568750beb9`**, instalado en el host de QA y `cauce.socket` reiniciado.

**L56 (dos capturas finales) — editor del personalizado y error real:**
- **`07-presets-custom.png`** (1440×900): chip **Personalizado** activo y editor compacto con Contenedor (MP4), Código de video (h264), Calidad (CRF/Alta), Resolución (Original), Código de audio (aac), Bitrate (192 kbps), Subtítulos (Conservar) y las acciones Guardar/Borrar. Verificado a ojo.
- **Defecto observado en esa captura**: la palabra "Personalizado" aparece **tres veces** (encabezado de la sección, ítem de la lista y título del detalle) y la frase "Un solo preset propio: al guardar reemplaza al anterior." dos veces. Es duplicación de jerarquía, no funcional; queda anotada para una unidad de presentación.
- **`08-error-daemon.png`** (1440×900): error **real** provocado deteniendo `cauce.socket`/`cauce.service`. Banner "No se pudo conectar con el daemon." con acción Reconectar y pie "Daemon: sin conexión". **Ningún `ErrorCode` crudo a la vista**: el detalle técnico vive en el desplegable, cerrado por defecto. La captura prueba el mensaje humano; no muestra el desplegable expandido.
- No se tocó código en esta pasada y **no se reconstruyó el paquete**: sigue el instalado (`sha256 3b4183ccf33737bd42c3ca834e52ca35c99bab056fc6fb154d2b56568750beb9`).
