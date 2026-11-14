# Cauce — Backlog técnico

**Estado:** planificación, sin implementación  
**Fuentes:** [`architecture.md`](architecture.md), [`parity.md`](parity.md), [`adr/`](adr/)  
**Objetivo:** convertir las especificaciones en unidades de trabajo verificables, con dependencias y puertas de calidad explícitas.

## Convenciones

- Los IDs son estables; no se renumeran al cambiar de fase.
- `P0` bloquea la planificación o la release; `P1` es fundacional; `P2` es MVP; `P3` es paridad posterior.
- Cada item debe caber en una revisión coherente y terminar con sus pruebas y documentación.
- No se estima en tiempo. Se ordena por dependencia y riesgo.
- Un item no se considera terminado si contradice un ADR o deja la matriz de paridad desactualizada.

## Secuencia de dependencias

```text
C-001 Licencia
   │
C-010 Workspace
   │
C-011 Tipos de dominio
   ├── C-012 Parser FFprobe
   ├── C-013 Catálogo de presets
   └── C-014 Compatibilidad
          │
       C-015 Command builder
          │
       C-016 Salida y validación
          │
C-020 Diagnóstico FFmpeg
   │
C-021 Runner y progreso
   │
C-030 SQLite
   │
C-031 Cola y estados
   │
C-032 Daemon/IPC
   │
C-033 systemd
   │
C-040 GUI shell
   ├── C-041 Convertir
   ├── C-042 Cola
   └── C-043 Presets/Diagnóstico/Ajustes
          │
C-050 Empaquetado
   │
C-051 Check-package
   │
C-052 CI Ubuntu 26.04
   │
C-060 Integración real
   │
C-061 GUI smoke
   │
C-062 Release MVP
```

## Backlog del MVP

| ID | Fase | Prio | Entrega | Depende de | Aceptación |
|---|---|---|---|---|---|
| `C-001` | 0 | P0 | Ratificar licencia y completar `LICENSE`/`NOTICE` | ADR 0006 | La licencia queda aceptada y documentada antes de la release |
| `C-010` | 1 | P1 | Workspace Rust y crates vacíos con fronteras del ADR | C-001 | `cargo metadata` lista los crates y no hay dependencias invertidas |
| `C-011` | 1 | P1 | Tipos de dominio: snapshot, plan, job, preset, ajustes | C-010 | Serialización estable y tests de contrato |
| `C-012` | 1 | P1 | Parser FFprobe JSON | C-011 | Fixtures de video/audio/subtítulos y errores cubiertos |
| `C-013` | 1 | P1 | Catálogo de los 13 presets del sistema | C-011 | IDs y campos coinciden con `parity.md`; tests de catálogo |
| `C-014` | 1 | P1 | Motor de compatibilidad video/audio/subtítulos | C-012, C-013 | Copy/transcode/remove se decide por stream y se explica |
| `C-015` | 1 | P1 | Command builder seguro | C-014 | Tests de argv exacto; no hay shell ni flags arbitrarios |
| `C-016` | 1 | P1 | Resolución de salida y validador | C-011, C-015 | Duplicados, temporal, duración, streams y rename atómico |
| `C-017` | 1 | P1 | Fixtures multimedia pequeños y reproducibles | C-010 | Se generan con FFmpeg o se versionan con licencia compatible |
| `C-020` | 2 | P1 | Localizador y diagnóstico de FFmpeg del sistema | C-010 | Versión, ffprobe, encoders y errores accionables |
| `C-021` | 2 | P1 | Runner de procesos y parser de progreso | C-020, C-015 | Progreso, ETA, pausa, cancelación y cierre de grupo |
| `C-030` | 2 | P1 | Esquema SQLite, migraciones y reservas | C-011 | WAL, migraciones idempotentes y backup recuperable |
| `C-031` | 2 | P1 | Máquina de estados y scheduler | C-016, C-021, C-030 | Transiciones, concurrencia, recuperación y reintento |
| `C-032` | 2 | P1 | Daemon e IPC JSON-RPC | C-031 | Métodos/eventos del ADR; socket 0600; reconexión |
| `C-033` | 2 | P1 | Socket y servicio systemd de usuario | C-032 | La GUI activa el daemon y las conversiones siguen al cerrar la GUI |
| `C-040` | 3 | P2 | Shell GTK4/Libadwaita y cliente del daemon | C-032, C-033 | Abre, conecta, reconecta y muestra estado sin bloquear |
| `C-041` | 3 | P2 | Página Convertir | C-040, C-012, C-013, C-014 | Añadir archivos/carpetas, inspeccionar, elegir preset y encolar |
| `C-042` | 3 | P2 | Página Cola | C-040, C-031 | Progreso, pausa, reanudar, cancelar, reintentar y eliminar |
| `C-043` | 3 | P2 | Presets, Diagnóstico y Ajustes | C-040 | Catálogo legible, FFmpeg/CPU/disco, tema/idioma/concurrencia |
| `C-050` | 4 | P2 | `.deb`, desktop, AppStream, iconos y unidades | C-033, C-040 | Paquete instalable con dependencias declaradas |
| `C-051` | 4 | P2 | `check-package` | C-050 | Valida binarios, unidades, metadatos, permisos y dependencias |
| `C-052` | 4 | P2 | CI Ubuntu 26.04 | C-051 | fmt, clippy, tests, GUI Xvfb y clean-install |
| `C-060` | 5 | P2 | Integración de conversión real | C-021, C-031 | MP4/H.264, MKV/H.265, WebM/VP9 y audio MP3/FLAC |
| `C-061` | 5 | P2 | Smoke de GUI y daemon | C-040, C-052 | Xvfb abre la ventana y el daemon responde |
| `C-062` | 5 | P2 | Release MVP v0.1 | C-060, C-061 | `.deb`, checksums, notas, QA y criterios de `parity.md` |

## Backlog de paridad posterior

| ID | Fase | Entrega | Depende de | Aceptación |
|---|---|---|---|---|
| `C-070` | v0.2 | Watchers persistentes | C-031, C-060 | Archivo estabilizado, sin reprocesar, keep/delete/move |
| `C-071` | v0.2 | Historial visual | C-030, C-040 | Lista/detalle persistente y filtros |
| `C-072` | v0.2 | Presets personalizados | C-013, C-030 | Crear, editar, duplicar y eliminar sin tocar los del sistema |
| `C-073` | v0.3 | VAAPI/NVENC/QSV | C-020, C-021, C-060 | Detección, prueba corta y fallback CPU explícito |
| `C-074` | v0.3 | Subtítulos y HDR avanzados | C-014, C-015 | Convertir/incrustar subtítulos y tone-map validado |
| `C-075` | v1.0 | Logs avanzados, notificaciones y exportación | C-040, C-052 | Rotación, filtros, exportación y redacción |
| `C-076` | v1.0 | Actualización desde release | C-050 | El `.deb` informa versión nueva y abre la página oficial; sin auto-update silencioso |

## Puertas de calidad

### G0 — Especificación

- ADRs aceptados o marcados explícitamente como propuestos.
- Arquitectura y matriz de paridad enlazadas.
- Licencia ratificada antes de release.

### G1 — Núcleo

- Tests de parser, compatibilidad, presets, command builder y validación.
- Ningún test requiere GPU ni red.
- Argumentos FFmpeg verificados como argv, no string.

### G2 — Daemon

- SQLite WAL y migraciones.
- IPC con socket temporal y `SO_PEERCRED`.
- Recuperación tras reinicio y cierre de GUI.

### G3 — GUI

- Xvfb smoke.
- El cliente no accede a FFmpeg/SQLite.
- Estados vacío, error y daemon caído son distinguibles.

### G4 — Paquete

- `check-package` completo.
- Clean install en Ubuntu 26.04.
- `ffmpeg` y GTK4 resueltos por APT.
- Daemon arranca por systemd de usuario.

### G5 — Release

- Matriz de paridad actualizada.
- Checksums y notas publicadas.
- Criterios de aceptación del MVP marcados.
- Sin warnings/errores en la validación de CI.

## Definition of Ready

Un item puede empezar cuando:

- Tiene alcance y aceptación explícitos.
- Sus dependencias están completas.
- No contradice un ADR.
- Tiene fixture o prueba definida si toca FFmpeg.
- Tiene documentación de usuario o decisión asociada.

## Definition of Done

Un item está terminado cuando:

- Implementación y pruebas pasan.
- `cargo fmt`/clippy y la suite relevante pasan.
- Se actualizó la documentación afectada.
- Se actualizó `parity.md` si cambia una capacidad.
- No se introducen secretos, rutas arbitrarias ni flags inseguros.
- El paquete o la integración correspondiente se validó si aplica.

## Fuera del alcance del backlog

- Windows, macOS y AppImage.
- FFmpeg embebido.
- Auto-update silencioso.
- Nube, cuentas o telemetría.
- DRM, streaming en vivo o descarga de medios.
