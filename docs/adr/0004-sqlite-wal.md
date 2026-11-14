# ADR 0004 — Persistencia con SQLite WAL

- **Estado:** Aceptado
- **Fecha:** 2026-11-14
- **Decisores:** Wilkin Barban
- **Ámbito:** estado de trabajos, presets y configuración

## Contexto

Cauce necesita recuperar trabajos tras un reinicio, mantener historial de transiciones y evitar colisiones de salida. Los archivos JSON sueltos no dan transacciones ni consultas fiables para una cola concurrente.

## Decisión

El daemon usará SQLite en modo WAL, con migraciones versionadas, claves foráneas y una única conexión de escritura. La base vivirá en `$XDG_DATA_HOME/cauce/cauce.db`.

Tablas iniciales: `schema_version`, `jobs`, `job_events`, `presets`, `settings`, `output_reservations`. Watchers tendrá sus tablas en la fase de paridad.

## Consecuencias

- Estado durable y recuperable.
- Transacciones para reservar salidas y cambiar estados.
- La GUI no lee la base directamente; consulta al daemon.
- Hay que mantener migraciones y hacer backup antes de cambios de esquema.
- La base no contendrá secretos ni credenciales.

## Alternativas descartadas

- **JSON en disco:** simple, pero sin transacciones ni consultas seguras.
- **Base de datos del sistema:** dependencia innecesaria para una app de usuario.

## Validación

- Un trabajo `running` pasa a `interrupted` al reiniciar y puede reencolarse.
- Dos trabajos no reservan la misma salida.
- Las migraciones son idempotentes y conservan datos.
- El backup previo a una migración es recuperable.
