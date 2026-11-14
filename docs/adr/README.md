# Architecture Decision Records — Cauce

Los ADRs registran decisiones que cambian la arquitectura o el contrato del producto. Son normativos: una implementación no debe contradecirlos sin un ADR nuevo que los reemplace.

| ADR | Decisión | Estado |
|---|---|---|
| [0001](0001-ffmpeg-del-sistema.md) | Usar FFmpeg del sistema | Aceptado |
| [0002](0002-daemon-y-gui.md) | Separar daemon y GUI | Aceptado |
| [0003](0003-gtk4-libadwaita.md) | Interfaz GTK4/Libadwaita | Aceptado |
| [0004](0004-sqlite-wal.md) | Persistencia con SQLite WAL | Aceptado |
| [0005](0005-json-rpc-socket.md) | IPC JSON-RPC por socket Unix | Aceptado |
| [0006](0006-licencia-gpl.md) | Licencia GPL-3.0-or-later | Propuesto |

## Cuándo crear un ADR

- Cambia una decisión ya aceptada.
- Introduce una dependencia de sistema o una frontera de proceso.
- Cambia el contrato IPC, el esquema SQLite o la licencia.
- Añade una función que afecta la paridad o el empaquetado.

Un ADR debe incluir contexto, decisión, consecuencias, alternativas y validación. Los ADRs aceptados no se editan para cambiar la decisión: se crea uno nuevo que los reemplace y se actualiza este índice.
