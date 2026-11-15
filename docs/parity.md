# Cauce — Matriz de paridad

**Estado:** borrador normativo, sin implementación  
**Referencia:** FF-Converter `v1.1.1`  
**Catálogo de presets:** [`presets.md`](presets.md)  
**Objetivo:** que ninguna función del producto actual se pierda por olvido y que el MVP tenga límites explícitos.

**Planificación detallada:** [`backlog.md`](backlog.md) · **Decisiones:** [`adr/`](adr/)

## Leyenda

| Estado | Significado |
|---|---|
| **MVP** | Debe estar terminado para v0.1 |
| **Planificado** | Tiene fase y criterio, pero no entra en v0.1 |
| **Posterior** | Depende de una capacidad externa o de una fase futura |
| **Fuera** | No forma parte de Cauce |
| **Parcial** | Existe una versión mínima; la paridad completa llega después |

## Matriz

| Función / comportamiento | FF-Converter 1.1.1 | Cauce v0.1 | Cauce v0.2 | Cauce v1.0 | Criterio de aceptación |
|---|---|---|---|---|---|
| Instalación en Ubuntu | `.deb` + AppImage | **MVP**: `.deb` | `.deb` | `.deb` | `apt install` en Ubuntu 26.04; binarios, socket y `.desktop` presentes |
| Añadir archivos | Drag & drop + selector | **MVP** | **MVP** | **MVP** | Añadir varios archivos y una carpeta; rutas autorizadas por portal |
| Inspección de medios | FFprobe | **MVP** | **MVP** | **MVP** | Muestra duración, contenedor, video, audio y subtítulos |
| Presets del sistema | 13 presets | **MVP**: catálogo completo | **MVP** | **MVP** | Cada preset produce un plan válido y explicable |
| Presets personalizados | Crear/editar/eliminar | **Planificado** | **MVP** | **MVP** | Guardar, editar, duplicar y eliminar sin corromper presets del sistema |
| Compatibilidad de video | copy/transcode | **MVP** | **MVP** | **MVP** | Copia cuando codec, contenedor, resolución y HDR son compatibles; si no, recodifica |
| Compatibilidad de audio | copy/transcode | **MVP** | **MVP** | **MVP** | Selecciona pista default/idioma/índice; copia si el contenedor lo permite |
| Compatibilidad de subtítulos | copiar/convertir/eliminar/burn-in | **Parcial**: copiar/eliminar | **MVP**: convertir | **MVP** | MP4/MKV/WebM resuelven subtítulos según política |
| Selección de streams | Video/audio/subtítulo | **MVP**: audio y video | **MVP** | **MVP** | `-map` correcto; nunca se mapea un stream inexistente |
| HDR/SDR | Preservar/tone-map | **Planificado** | **Planificado** | **MVP** | Un preset HDR→SDR produce un plan reproducible y validado |
| Codificación CPU | x264/x265/SVT-AV1 | **MVP** | **MVP** | **MVP** | Conversión real de fixtures con checksum/ffprobe |
| Aceleración GPU | NVENC/QSV/AMF/VAAPI | **Planificado** | **Planificado** | **MVP** | VAAPI/NVENC se detectan y prueban; fallback CPU explícito |
| Diagnóstico de hardware | Encoders + CPU + prueba | **MVP**: CPU/FFmpeg | **MVP** | **MVP** | La UI muestra versión, encoders, CPU y causa de indisponibilidad |
| Cola persistente | SQLite + recuperación | **MVP** | **MVP** | **MVP** | Reiniciar GUI/daemon conserva trabajos y reencola interrumpidos |
| Concurrencia | Límite configurable | **MVP**: 1–8, CPU | **MVP** | **MVP** | Nunca supera el límite; la cola sigue avanzando |
| Pausa/reanudación | Por trabajo y por cola | **MVP** | **MVP** | **MVP** | SIGSTOP/SIGCONT al grupo de procesos; estado persistido |
| Cancelación | Por trabajo y cola | **MVP** | **MVP** | **MVP** | No deja procesos huérfanos; conserva o elimina temporal según ajuste |
| Reintento | Trabajos fallidos | **MVP** | **MVP** | **MVP** | Vuelve a `queued` sin duplicar salida ni entrada |
| Progreso real | `-progress pipe:1` | **MVP** | **MVP** | **MVP** | Porcentaje, FPS, velocidad, bitrate y ETA avanzan durante la conversión |
| Validación de salida | FFprobe + duración/streams | **MVP** | **MVP** | **MVP** | Una salida inválida no reemplaza el destino |
| Duplicados | suffix/skip/overwrite | **MVP** | **MVP** | **MVP** | Nunca se sobrescribe el archivo de entrada |
| Carpetas vigiladas | Chokidar, reglas, post-acción | **Planificado** | **MVP** | **MVP** | Detecta archivo estabilizado, no reprocesa y aplica keep/delete/move |
| Historial | Lista y detalle | **Planificado** | **MVP** | **MVP** | Persistente, filtrable y con transiciones auditables |
| Logs | Niveles, filtros, búsqueda, persistencia | **Parcial**: básico | **MVP** | **MVP** | Rotación, filtros y exportación sin rutas sensibles |
| Notificaciones | Completado/fallido | **MVP**: opcional | **MVP** | **MVP** | Desactivadas por defecto; el daemon emite una sola notificación terminal con el nombre base |
| Tema | Claro/oscuro/sistema | **MVP** | **MVP** | **MVP** | Sigue el tema del sistema y permite override |
| Idiomas | ES/EN | **MVP**: ES/EN/PT-BR | **MVP**: ES/EN/PT-BR | **MVP**: ES/EN/PT-BR | Cambio en vivo sin reiniciar; claves completas y fallback |
| Actualizaciones | Auto-update Electron | **Fuera del MVP** | **Posterior** | **Posterior** | En `.deb`, abrir la página oficial de releases; no auto-update silencioso |
| Windows/macOS | Sí | **Fuera** | **Fuera** | **Fuera** | Cauce es Ubuntu-native; no hay binarios para otros SO |
| FFmpeg embebido | Sí | **Fuera** | **Fuera** | **Fuera** | Cauce usa `ffmpeg` del sistema; se declara como dependencia |

## Catálogo inicial de presets

Los 13 presets del sistema se portan al MVP con sus identificadores estables. Los nombres visibles se localizan en **ES/EN/PT-BR desde v0.1**.

| ID | Preset | Salida | v0.1 |
|---|---|---|---|
| `system-smarttv-compat` | Smart TV — Máxima Compatibilidad | MP4 H.264/AAC | **MVP** |
| `system-smarttv-hevc` | Smart TV — Calidad Eficiente | MKV H.265/AAC | **MVP** |
| `system-youtube-social` | YouTube & Redes Sociales | MP4 H.264/AAC | **MVP** |
| `system-archival-master` | Archivo / Máxima Calidad | MKV H.265/copy | **MVP** |
| `system-fast-compress` | Compresión Rápida | MP4 H.265/AAC | **MVP** |
| `system-audio-mp3` | Extraer Audio — MP3 320k | MP3 | **MVP** |
| `system-audio-flac` | Extraer Audio — FLAC | FLAC | **MVP** |
| `system-apple-music-export` | Exportación Apple Music | M4A AAC | **MVP** |
| `system-samsung-tv` | Samsung TV — 4K | MP4 H.265/AAC | **MVP** |
| `system-lg-tv` | LG TV — HEVC 4K | MKV H.265/AAC | **MVP** |
| `system-android-tv` | Android TV / Google TV | MKV H.265/AAC | **MVP** |
| `system-audio-alac` | Extraer Audio — ALAC | M4A ALAC | **MVP** |
| `system-audio-wav` | Extraer Audio — WAV PCM | WAV PCM | **MVP** |

Los presets con `hardwarePreference: auto` se resuelven a CPU en el MVP; la selección GPU se activa en v0.3.

## Qué significa “paridad completa”

Cauce v1.0 será paridad funcional con FF-Converter en Linux, no una copia de Electron:

- Mismas capacidades de conversión, presets, cola, watchers, historial, diagnóstico y logs.
- Misma seguridad de archivos: temporales, validación, rename atómico y no sobrescritura de entrada.
- Misma experiencia de usuario en español e inglés, más portugués de Brasil.
- Diferencias deliberadas: GTK4 nativo, daemon persistente, FFmpeg del sistema, sin auto-update de Electron y sin Windows/macOS.

La paridad de hardware significa **detectar y usar lo que el equipo permita**, con fallback explicable a CPU; no significa garantizar VAAPI/NVENC en cualquier GPU.

## Fases

### v0.1 — MVP

- Añadir archivos/carpetas.
- FFprobe, presets del sistema, plan de conversión.
- Cola persistente, concurrencia, pausa, cancelación, reintento.
- Progreso real, validación, duplicados.
- Diagnóstico de FFmpeg/CPU, logs básicos, ajustes.
- Daemon + GUI GTK4.
- `.deb` y clean install.

### v0.2 — Flujo automático y memoria

- Watchers con estabilización y deduplicación.
- Historial visual y detalle de transiciones.
- Presets personalizados.
- Logs persistentes con filtros y exportación.
- Mejoras de accesibilidad y revisión de traducciones ES/EN/PT-BR.

### v0.3 — Calidad y aceleración

- VAAPI/NVENC/QSV con pruebas reales y fallback.
- HDR/SDR, subtítulos convertidos/incrustados.
- Selección avanzada de streams.
- Ajustes de calidad y perfiles por dispositivo.

### v1.0 — Paridad

- Cierre de la matriz anterior.
- Documentación, QA en Ubuntu 26.04 y release estable.
- Sin funciones de Windows/macOS.

## Pruebas de aceptación del MVP

1. Instalar el `.deb` en Ubuntu 26.04 limpio.
2. Abrir Cauce, añadir un MP4 y un MKV, elegir preset y enviar a cola.
3. Verificar progreso real y pausa/reanudación.
4. Cerrar la GUI y comprobar que el daemon termina la conversión.
5. Reabrir la GUI y ver el trabajo completado.
6. Repetir con audio MP3/FLAC y comprobar que no hay stream de video.
7. Forzar un fallo de FFmpeg y comprobar que el destino anterior no se pierde.
8. Ejecutar `cargo fmt --check`, clippy, tests y GUI bajo Xvfb.
9. Verificar que `apt install ./cauce_*.deb` resuelve `ffmpeg` y las bibliotecas GTK.

## Decisiones fuera de alcance

- No se implementará Windows, macOS, AppImage ni auto-update.
- No se empaquetará FFmpeg/FFprobe en el `.deb`.
- No se aceptarán argumentos FFmpeg arbitrarios en el MVP.
- No se promete aceleración por hardware hasta que exista una prueba real por backend.
