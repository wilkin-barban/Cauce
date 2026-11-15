# Cauce 0.1.0 — notas de la versión candidata

**Estado:** candidato de release preparado localmente. **Todavía no está publicado; espera la aprobación del mantenedor.** No hay tag, no hay release en GitHub y no se subió ningún artefacto (`odd/tasks/cauce-0.1.md` S15, T25/C-062).

## Control run before the tag

One clean run with a unique output directory through IPC, same preset used by the earlier failed attempt: **completed**, and the published file was verified with `ffprobe` (H.264 320×240, AAC mono, 2.000 s, 53 151 B). The earlier failure was environmental (reused output folder), not a product defect.

## Qué es Cauce

Cauce es un conversor multimedia nativo para Ubuntu 26.04 (amd64), escrito en Rust con GTK4/Libadwaita y su propio daemon de usuario. Usa el **FFmpeg del sistema** (`ffmpeg`/`ffprobe`) como dependencia: no embebe binarios ni transcodificadores propios.

Se añaden archivos o carpetas, se inspeccionan con `ffprobe`, se elige uno de los **13 presets del sistema** y los trabajos entran en una cola persistente (SQLite WAL). El daemon es el dueño de los trabajos: **cerrar la ventana no detiene ninguna conversión**. La GUI habla con el daemon por JSON-RPC 2.0 sobre un socket Unix privado y nunca abre la base ni lanza FFmpeg.

- Licencia: **GPL-3.0-or-later** (`LICENSE`, `NOTICE`).
- Binarios del paquete: `cauce-daemon`, `cauce-gui` y `cauce-cli`.
- Unidades de usuario: `cauce.socket` (socket en `$XDG_RUNTIME_DIR/cauce/daemon.sock`, modo `0600`) y `cauce.service`.

## Criterios de aceptación de v0.1 (`docs/architecture.md` §19)

Se marcan **solo con evidencia observada**. Las referencias `E1`-`E5` son las
evidencias de este candidato; los detalles de ejecución están en
`tests/e2e/latest/README.md`.

| Criterio de §19 | Evidencia |
|---|---|
| Un usuario instala el `.deb` en Ubuntu 26.04 y abre Cauce desde el menú. | **E3** clean install: `apt-get install --reinstall ./cauce_0.1.0-1_amd64.deb` → `ii cauce 0.1.0-1`, binarios en `/usr/bin` (755), metainfo y `.desktop` validados con `appstreamcli`/`desktop-file-validate`; `packaging/check-package.sh` en verde. |
| El daemon arranca por socket y sobrevive al cierre de la GUI. | **E2** + **E4**: el socket se crea en un directorio `0700` con modo `0600` y credenciales de par verificadas (`SO_PEERCRED`); `tests/e2e/gui_smoke.sh` comprueba que el proceso y el socket siguen vivos después de que la GUI salga. |
| Se pueden añadir varios archivos y elegir un preset. | **E1**: `jobs.add` acepta lotes de entradas y devuelve un trabajo por archivo, con el plan y su `reasoning[]`. |
| La cola convierte al menos MP4/H.264, MKV/H.265, WebM/VP9 y audio MP3/FLAC. | **E1** conversiones reales: cinco casos con `ffprobe` sobre el archivo publicado (contenedor, códecs, resolución, duración y canales). El caso WebM/VP9 usa una fuente VP9 real y el preset `system-archival-master`, que copia el audio Opus y transcodifica el video a H.265 (v0.1 no tiene preset con video VP9). |
| El progreso refleja el archivo real, no un temporizador simulado. | Progreso leído de `-progress pipe:1` de FFmpeg (porcentaje, ms procesados, fps, velocidad, ETA) y emitido por el daemon; cubierto por los tests del runner y observable en `jobs.get`/`job.progress` durante **E1**. |
| Pausar, reanudar, cancelar y reintentar funcionan. | Máquina de estados de `docs/contracts.md` §4 con matriz completa en tests; pausa/reanudación por `SIGSTOP`/`SIGCONT` y cancelación `SIGTERM`→`SIGKILL` verificadas sobre un hijo vivo. |
| Una salida inválida no reemplaza el destino. | El trabajo escribe un temporal y solo publica con `rename` atómico después de validar; un fallo de validación deja el destino previo intacto y `keep_temp_on_failure` decide el temporal. |
| El archivo de entrada nunca se sobrescribe. | La resolución de destino jamás devuelve la ruta de entrada (test dedicado) y **E1** comprueba tamaño y mtime de cada fuente antes y después. |
| La GUI funciona bajo Xvfb y el paquete pasa clean-install. | **E4** `tests/e2e/gui_smoke.sh`: `CAUCE_SMOKE=1` imprime `window-ready` bajo `xvfb-run`, el daemon sobrevive, una conversión real completa y `SIGTERM` deja el daemon en exit 0. **E3** clean install. |
| Documentación de paridad publicada y enlazada. | `docs/parity.md` está en el árbol y enlazado desde `docs/architecture.md`. |

Evidencias de i18n (**E5**): los catálogos ES/EN/PT-BR tienen el mismo conjunto
de 224 claves, con un test que falla ante cualquier diferencia y conmutación de
idioma en vivo sin reiniciar la aplicación.

## Instalación

Requisitos: Ubuntu 26.04 amd64 con FFmpeg 8.0.x del sistema.

```bash
# 1. Dependencias del sistema (las declara el paquete, apt las resuelve)
sudo apt install ffmpeg libgtk-4-1 libadwaita-1-0 libnotify4 xdg-desktop-portal

# 2. Instalar el .deb candidato (no está en ningún repositorio todavía)
sudo apt install ./cauce_0.1.0-1_amd64.deb
```

```bash
# 3. Activar el socket de usuario: el daemon arranca al primer uso
systemctl --user enable --now cauce.socket
systemctl --user status cauce.socket      # active (listening)

# 4. Abrir Cauce desde el menú de GNOME, o directamente:
cauce-gui
```

El daemon queda vivo aunque se cierre la ventana; las conversiones siguen. Para
detenerlo: `systemctl --user stop cauce.socket cauce.service`. La base de datos y
los logs viven en `$XDG_DATA_HOME/cauce/`.

## Limitaciones conocidas de 0.1.0

- **Sin fixture HDR**: la cadena de tone-mapping HDR→SDR está implementada y
  probada a nivel de filtros, pero no hay muestra HDR en el conjunto e2e, así que
  esa ruta no se ejercita de punta a punta.
- **`normalize_audio` rechazado**: v0.1 no tiene filtro de normalización, así que
  cualquier preset que la pida se rechaza con `PLAN_INVALID` en vez de degradar
  el audio en silencio.
- **La pausa de cola vive en memoria**: al reiniciar el daemon la cola vuelve a
  estar activa (los trabajos activos pasan a `interrupted` con evento `recovery`
  y se reencolan a mano con `jobs.recover`).
- **Etiqueta de CI pendiente**: GitHub todavía no publica el runner `ubuntu-26.04`,
  así que el workflow usa `ubuntu-latest` con un comentario explícito. El objetivo
  soportado sigue siendo Ubuntu 26.04.
- **Nombres de preset asimétricos**: en `docs/presets.md` tres presets no llevan
  el prefijo "Smart TV —"/"Samsung TV —" del lado inglés
  (`system-smarttv-compat`, `system-smarttv-hevc`, `system-android-tv`). Se
  respetó el documento al pie de la letra; unificarlos exige editar la spec.
- **`cauce-cli` es un marcador de posición**: el binario se instala, pero en v0.1
  la CLI es interna y su interfaz pública queda para una fase posterior.
- Fuera de alcance por decisión (S10): watchers, historial visual, presets
  personalizados y aceleración por hardware (VAAPI/NVENC/QSV).

## Qué falta para publicar

Este documento es el candidato, no un anuncio: falta la aprobación explícita del
mantenedor para crear el tag, subir el `.deb` y publicar las notas. Nada de eso se
hizo.
