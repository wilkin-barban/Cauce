# ADR 0006 — Licencia GPL-3.0-or-later

- **Estado:** Propuesto, ratificar antes de la primera release
- **Fecha:** 2026-11-14
- **Decisores:** Wilkin Barban
- **Ámbito:** licencia del código y distribución

## Contexto

FF-Converter usa MIT. Segmenta usa GPL-3.0-or-later. Cauce es una implementación nueva que puede reutilizar ideas y contratos del proyecto MIT, y usa FFmpeg del sistema (proceso separado).

## Decisión propuesta

Publicar Cauce bajo **GPL-3.0-or-later** y conservar la atribución del código MIT reutilizado en un archivo `NOTICE`.

## Consecuencias

- El código y las mejoras derivadas permanecen abiertos.
- Coherencia con Segmenta y con una distribución que podría incorporar FFmpeg GPL en el futuro.
- Si se usa código MIT, se puede incorporar a GPL conservando avisos.
- MIT sería más permisivo para reutilización propietaria, pero no es la recomendación.

## Validación

- Existe un archivo `LICENSE` con el texto GPL-3.0.
- `NOTICE` enumera cualquier código MIT reutilizado.
- La release declara la licencia y las obligaciones de terceros.
- La decisión se ratifica antes de publicar v0.1.

## Alternativa

MIT: más simple y compatible con FF-Converter, pero permite derivados propietarios. Si se elige MIT, se crea un ADR que reemplace este.
