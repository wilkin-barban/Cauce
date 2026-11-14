# ADR 0001 — Usar FFmpeg del sistema

- **Estado:** Aceptado
- **Fecha:** 2026-11-14
- **Decisores:** Wilkin Barban
- **Ámbito:** distribución Ubuntu y dependencia multimedia

## Contexto

FF-Converter incluye FFmpeg y FFprobe estáticos para funcionar sin dependencias externas. Esa decisión hace que el `.deb` actual ronde los 203 MB y obliga a mantener y auditar una build GPLv3 propia. Cauce apunta a Ubuntu 26.04, donde el repositorio del sistema distribuye FFmpeg 8.0.1 con `libx264`, `libx265`, `libsvtav1` y VAAPI.

## Decisión

Cauce usará los binarios `ffmpeg` y `ffprobe` instalados por el sistema. El `.deb` declarará `ffmpeg` como dependencia y el daemon validará versión, binarios y capacidades al arrancar.

## Consecuencias

- El paquete de Cauce será mucho más pequeño y no duplicará códecs ni bibliotecas.
- Las actualizaciones de seguridad de FFmpeg llegan por APT.
- Cauce no funcionará sin FFmpeg; la GUI mostrará una acción concreta para instalarlo.
- La versión disponible puede variar entre versiones de Ubuntu y debe validarse en ejecución.
- El MVP no soportará binarios FFmpeg embebidos.

## Alternativas descartadas

- **FFmpeg embebido:** paridad de empaquetado con FF-Converter, pero aumenta mucho el tamaño y las obligaciones de distribución.
- **Híbrido sistema/embebido:** más modos y diagnóstico; se puede reconsiderar en una fase posterior, no en el MVP.

## Validación

- `ffmpeg -version` y `ffprobe -version` responden.
- La comprobación de arranque confirma los encoders requeridos.
- Un `.deb` limpio en Ubuntu 26.04 instala `ffmpeg` mediante APT.
- Si falta FFmpeg, Cauce no acepta trabajos y muestra el diagnóstico.
