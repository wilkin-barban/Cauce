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
- Resumen del plan para el primer archivo: “Copiar video / Convertir audio a AAC / Quitar subtítulos”.
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
| Eliminar | No borra el archivo de entrada; pide confirmación si está activo |

Nunca se muestra un trabajo como completado si la validación falló.

## 6. Página Presets

- Catálogo de los 13 presets, agrupado por Smart TV, Web, Archivo, Optimización, Audio y Música.
- Cada tarjeta muestra nombre, descripción, contenedor, códecs y límites.
- Al seleccionar, muestra el plan resumido y una advertencia si el preset depende de hardware no disponible.
- En v0.1 no hay botón de edición ni creación; los presets personalizados llegan en v0.2.
- Un preset del sistema no se puede modificar ni eliminar.

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
| Presets personalizados | No en v0.1 |

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
5. Los trabajos interrumpidos aparecen con acción **Reencolar**.

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
