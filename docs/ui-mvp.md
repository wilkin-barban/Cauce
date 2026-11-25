# Cauce — Especificación de la GUI MVP

**Estado:** normativo para v0.1  
**Stack:** GTK4 + Libadwaita  
**Decisiones:** añadir a la cola y luego iniciar; 13 presets del sistema; CLI interna.

## 1. Objetivo

La GUI debe permitir preparar una conversión por lotes sin ocultar el estado real de la cola ni el daemon. El usuario revisa archivos, preset y destino antes de iniciar. Cerrar la ventana no cancela los trabajos.

La GUI es un cliente: no ejecuta FFmpeg, no abre SQLite y no conoce detalles internos del command builder.

## 2. Estructura

```text
┌─────────────────────────────────────────────────────────────┐
│ Cauce                                      [daemon: activo] │
├───────────────┬─────────────────────────────────────────────┤
│ Convertir     │                                             │
│ Cola          │  Contenido de la página seleccionada        │
│ Presets       │                                             │
│ Diagnóstico   │                                             │
│ Ajustes       │                                             │
│               │                                             │
│ [FFmpeg 8.0]  │                                             │
└───────────────┴─────────────────────────────────────────────┘
```

- `AdwApplicationWindow` con `AdwNavigationSplitView`.
- Sidebar con páginas y estado resumido del daemon/FFmpeg.
- Barra inferior de estado: daemon, trabajos activos y errores recientes.
- Sin menú web, sin navegador embebido y sin Electron.

## 3. Flujo principal

1. La app abre y consulta `system.capabilities` y `jobs.list`.
2. El usuario entra en **Convertir**.
3. Añade archivos o una carpeta por selector del portal o drag & drop.
4. La app inspecciona con `media.inspect` y muestra los archivos válidos y los rechazados.
5. El usuario elige carpeta de salida y uno de los 13 presets.
6. Pulsa **Añadir a la cola**.
7. La app muestra la cola; el usuario revisa y pulsa **Iniciar cola** o **Iniciar** en un trabajo.
8. La app muestra progreso real; puede pausar, reanudar, cancelar o reintentar.
9. Al completar, ofrece abrir la carpeta o el archivo.

La acción de añadir nunca inicia una conversión por sí sola.

## 4. Página Convertir

### Contenido

- Zona de drop: “Arrastra videos, audios o una carpeta”.
- Botones **Añadir archivos** y **Añadir carpeta**.
- Lista de archivos seleccionados con nombre, duración, resolución, códec y estado de inspección.
- Selector de carpeta de salida; por defecto la última usada.
- Selector de preset, agrupado por categoría.
- **Selector de pista de audio** por archivo, con la etiqueta de cada pista: idioma, códec, cantidad de canales y bitrate (`spa · eac3 · 5.1 · 640 kbps`). Sin idioma declarado lo dice, no deja el paréntesis vacío.
- **Interruptor de realce de voz**: encendido pide `stereo_dialogue_boost` o `dual_track_universal`, apagado deja `passthrough`. Se deshabilita con el motivo visible cuando la pista elegida tiene menos de dos canales.
- **Selector de subtítulos** más un par de radios **Suave / Incrustar**. Ambos controles se deshabilitan con el motivo visible cuando el archivo no trae subtítulos; el modo incrustar sigue disponible para una pista de mapa de bits, que es el único modo que puede llevarla.
- **Resumen del plan** derivado de la misma selección que viaja en el pedido —idioma, modo de audio y subtítulos—, nunca de una frase escrita aparte que pueda contradecirlo.
- Selector de resolución (`original`, `1080p`, `720p`, `480p`) y de calidad (CRF), que sobrescriben lo que el preset decidió y dejan constancia en el `reasoning[]` del plan.
- Botón **Añadir a la cola**.
- Aviso de archivos rechazados con el motivo, sin bloquear los válidos.

### Estados

| Estado | Comportamiento |
|---|---|
| Inicial | Drop zone visible; sin lista |
| Inspeccionando | Spinner local en la lista, no overlay de toda la app |
| Listo | Resumen de plan y botón habilitado |
| Parcial | Archivos válidos + bloque de rechazados |
| Error | Mensaje concreto; conservar archivos válidos |
| Daemon caído | Deshabilitar acciones y mostrar “Reconectar” |
| FFmpeg ausente | Bloqueo con acción `sudo apt install ffmpeg` |

## 5. Página Cola

### Contenido

- Barra superior: `Iniciar cola`, `Pausar cola`, contador de activos/en espera.
- Lista de trabajos con:
  - Nombre de entrada.
  - Preset.
  - Estado.
  - Barra de progreso; indeterminada si no hay duración.
  - Porcentaje, FPS, velocidad y ETA.
  - Acciones: pausar/reanudar, cancelar, reintentar, eliminar, abrir carpeta.
- Estado vacío: “No hay trabajos en la cola”.
- Los trabajos nuevos aparecen arriba; el orden es por prioridad y creación.

### Acciones

| Acción | Regla |
|---|---|
| Iniciar cola | Toma trabajos `queued` hasta el límite; no duplica activos |
| Pausar cola | No inicia nuevos; los activos pueden pausarse individualmente |
| Pausar trabajo | Solo si está `running`; estado `paused` visible |
| Reanudar | Solo si está `paused` |
| Cancelar | Confirmación si el trabajo está activo; temporales según ajuste |
| Reintentar | Solo `failed`; vuelve a `queued` |
| Reencolar | Solo `interrupted`; llama `jobs.recover` y el trabajo vuelve a `queued` |
| Eliminar | No borra el archivo de entrada; pide confirmación si está activo |

Nunca se muestra un trabajo como completado si la validación falló.

## 6. Página Presets

- Catálogo de los 13 presets en **grupos explícitos y navegables**: Smart TV (`tv`), Web y redes (`social`), Archivo (`archive`), Audio (`audio`) y **Personalizado** (`custom`), siempre último. La barra lateral muestra un encabezado por grupo y, al seleccionar una fila, el panel de detalle sólo habla de ese preset.
- Cada fila muestra nombre y una línea con contenedor y códecs; el panel de detalle muestra el nombre una sola vez, una frase de descripción y, dentro de un desplegable **«Detalles técnicos»** cerrado por defecto, los valores técnicos (contenedor, códecs, CRF, bitrate, resolución, HDR, subtítulos, faststart, metadatos).
- Al seleccionar, muestra una advertencia si el preset depende de hardware no disponible.
- Un preset del sistema no se puede modificar ni eliminar: el aviso de solo lectura sigue visible en su panel.
- El grupo **Personalizado** contiene exactamente un preset propio (`custom-1`). Su panel ofrece un editor compacto con siete desplegables (contenedor, video, calidad, resolución, audio, bitrate, subtítulos), **Guardar** y **Borrar**. Guardar reemplaza el anterior; no hay editor genérico ni presets múltiples. Una combinación inválida se explica en línea, sin llamar al daemon.
- El preset guardado aparece en el desplegable de **Convertir** sin reiniciar la app (`presets.list`).

## 7. Página Diagnóstico

Debe mostrar:

- Versión de Cauce, daemon y protocolo.
- Ruta, versión y estado de `ffmpeg`/`ffprobe`.
- Encoders/filtros requeridos y si faltan.
- Núcleos de CPU y memoria disponible.
- Espacio libre en la carpeta de salida.
- Estado del socket/daemon.
- Botón **Actualizar diagnóstico**.
- Botón **Copiar informe** sin rutas sensibles.

Si FFmpeg falta, la página explica la instalación y no permite iniciar trabajos.

## 8. Página Ajustes

| Ajuste | v0.1 |
|---|---|
| Carpeta de salida | Sí |
| Concurrencia (1–8) | Sí; por defecto 1 |
| Tema | Sistema/claro/oscuro |
| Idioma | ES/EN/PT-BR |
| Notificaciones | Sí, por defecto desactivadas |
| Política de duplicados | Suffix por defecto |
| Conservar temporal al fallar | No por defecto |
| FFmpeg | Solo sistema; diagnóstico de ruta |
| Presets personalizados | Sí: exactamente uno (`custom-1`), guardado en `settings` y editable desde su grupo; el resto queda fuera de v0.1 |

Los cambios se validan y persisten mediante `settings.set`. Un valor inválido no se guarda.

## 9. Diálogos y mensajes

- **Confirmación de cancelación:** “Se detendrá el trabajo; el archivo final no se publicará”.
- **Confirmación de eliminación activa:** “El trabajo se detendrá y se quitará de la cola”.
- **FFmpeg ausente:** “Cauce necesita FFmpeg del sistema. Instalalo con `sudo apt install ffmpeg`”.
- **Daemon caído:** “No se pudo conectar con el daemon. [Reconectar] [Ver diagnóstico]”.
- **Salida inválida:** “FFmpeg terminó, pero la salida no superó la validación. Se conservó el destino anterior”.
- **Archivo ocupado:** ofrecer `suffix` o cambiar la carpeta; nunca borrar el original.

Los errores se muestran dentro de la página afectada, no sólo como toast; los toasts se usan para confirmaciones breves.

## 10. Accesibilidad y teclado

- Navegación completa con Tab y flechas.
- Etiquetas accesibles en botones con icono.
- No depender sólo del color para estados.
- Contraste compatible con el tema del sistema.
- Atajos:
  - `Ctrl+O`: añadir archivos.
  - `Ctrl+Shift+O`: añadir carpeta.
  - `Ctrl+Q`: cerrar GUI sin detener el daemon.
  - `Ctrl+,`: Ajustes.
  - `Espacio`: pausar/reanudar el trabajo seleccionado.
- La UI debe funcionar con lector de pantalla y escalado de texto.

## 11. Notificaciones

- Desactivadas por defecto.
- Si el usuario las activa, las emite el **daemon**, una sola vez por trabajo terminal.
- Completado: “Conversión completada: `<nombre>`”.
- Fallido: “Conversión fallida: `<nombre>`”.
- Se usa solo el nombre base, nunca la ruta completa.
- La GUI no emite una segunda notificación; sus toasts son locales.
- Si el escritorio no soporta notificaciones, se registra un warning y la conversión no falla.

## 12. Reconexión

Si el daemon se cae o se reinicia:

1. La GUI muestra un banner no bloqueante.
2. Intenta reconectar con backoff.
3. Al reconectar, llama `jobs.list` y reconstruye la cola.
4. No inventa estados ni pierde selecciones locales.
5. Los trabajos interrumpidos aparecen con acción **Reencolar**: la fila llama `jobs.recover` y el trabajo queda `queued` en la lista. No se reanuda por sí solo.

## 13. Fuera de la GUI v0.1

- Watchers.
- Historial visual.
- Editor de presets.
- Configuración de VAAPI.
- Auto-update.
- Temas personalizados.
- CLI pública dentro de la GUI.

Estas funciones se documentan en [`parity.md`](parity.md) y [`backlog.md`](backlog.md).

## 14. Criterios de aceptación

- [ ] Añadir archivos y carpeta y ver la inspección sin bloquear la ventana.
- [ ] Seleccionar preset y destino, y añadir a la cola sin iniciar.
- [ ] Iniciar la cola y ver progreso real.
- [ ] Pausar/reanudar/cancelar/reintentar con estados correctos.
- [ ] Cerrar la GUI y comprobar que el daemon sigue trabajando.
- [ ] Reabrir y reconstruir la cola.
- [ ] Mostrar FFmpeg ausente con acción concreta.
- [ ] Mostrar un error de validación sin marcar el trabajo como completado.
- [ ] Smoke test bajo Xvfb.
