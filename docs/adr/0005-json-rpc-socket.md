# ADR 0005 — IPC JSON-RPC por socket Unix

- **Estado:** Aceptado
- **Fecha:** 2026-11-14
- **Decisores:** Wilkin Barban
- **Ámbito:** comunicación daemon/GUI/CLI

## Contexto

La GUI y la CLI necesitan consultar trabajos, enviar comandos y recibir progreso en tiempo real. Una API HTTP local amplía la superficie de red; una biblioteca compartida en el mismo proceso impide separar el daemon.

## Decisión

Usar JSON-RPC 2.0 sobre NDJSON en `$XDG_RUNTIME_DIR/cauce/daemon.sock`, con permisos `0600`, verificación `SO_PEERCRED`, límite de 1 MiB por mensaje y conexiones exclusivas del usuario.

La GUI enviará comandos y recibirá eventos. La CLI usará el mismo contrato. El daemon será la única fuente de verdad.

## Consecuencias

- Contrato explícito y fácil de probar con sockets temporales.
- Sin puertos TCP ni autenticación por token.
- Hay que versionar métodos/eventos y manejar clientes atrasados.
- La GUI debe reconectar y resincronizar tras cerrar/abrir.

## Alternativas descartadas

- **HTTP local:** más superficie y menos natural para eventos.
- **D-Bus:** integración GNOME interesante, pero mayor complejidad y menos portabilidad para la CLI.
- **Biblioteca en proceso:** no permite que la GUI se cierre sin detener el daemon.

## Validación

- Un cliente sin permisos no puede abrir el socket.
- La GUI recibe progreso y reconstruye el estado con `jobs.list`.
- Un mensaje mayor a 1 MiB se rechaza.
- La CLI y la GUI usan el mismo método para listar trabajos.
