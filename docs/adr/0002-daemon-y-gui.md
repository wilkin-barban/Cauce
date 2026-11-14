# ADR 0002 — Separar daemon y GUI

- **Estado:** Aceptado
- **Fecha:** 2026-11-14
- **Decisores:** Wilkin Barban
- **Ámbito:** ciclo de vida de trabajos y procesos

## Contexto

Las conversiones por lotes pueden durar más que una ventana abierta. FF-Converter mantiene la cola en el proceso Electron; al cerrar la aplicación se interrumpen los trabajos, aunque se intenta recuperarlos al volver a abrir. Cauce necesita que la cola continúe y que la GUI pueda cerrarse o reiniciarse sin perder estado.

## Decisión

Cauce se dividirá en:

- `cauce-daemon`: dueño de la cola, SQLite, trabajos, watchers y procesos FFmpeg.
- `cauce-gui`: cliente GTK4 que se conecta por socket Unix.
- `cauce-cli`: cliente de diagnóstico y automatización.

El daemon se activa por `cauce.socket` como servicio de usuario. La GUI no ejecuta FFmpeg ni accede a SQLite.

## Consecuencias

- Cerrar la GUI no detiene conversiones.
- La GUI se puede reconectar al daemon y reconstruir el estado.
- Hay que definir y versionar el contrato IPC.
- El paquete debe instalar unidades systemd de usuario y manejar el arranque/cierre.
- Aumenta la complejidad inicial frente a un único proceso, pero evita una migración posterior costosa.

## Alternativas descartadas

- **GUI monolítica:** más simple al inicio, pero no cumple “las conversiones siguen al cerrar la GUI”.
- **Daemon sin GUI:** no ofrece la experiencia de escritorio requerida.

## Validación

- Cerrar la GUI durante una conversión no mata el proceso FFmpeg.
- Reabrir la GUI muestra el progreso y el estado actualizados.
- El daemon se activa desde el socket y sobrevive a la reconexión del cliente.
