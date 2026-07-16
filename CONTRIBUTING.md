# Contribuir

Gracias por tu interés en Stock Local Agent. El proyecto busca ser un espacio claro para experimentar con agentes asíncronos, TUI en Rust y datos de mercado verificables.

## Antes de abrir un cambio

1. Revisa [`README.md`](README.md), [`docs/arquitectura.md`](docs/arquitectura.md) y [`docs/testing.md`](docs/testing.md).
2. Busca issues existentes para evitar trabajo duplicado.
3. Para cambios grandes, abre primero una issue con el problema, la propuesta y sus trade-offs.
4. No incluyas claves, bases SQLite, salidas con información privada ni archivos `.env`.

## Flujo local

```bash
git checkout -b feature/descripcion-corta
cargo fmt
cargo check --locked
cargo test --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```

No es necesario hacer commit desde las herramientas automáticas; el autor debe revisar el diff antes de publicarlo.

## Principios de diseño

- Prefiere cambios pequeños y enfocados.
- Mantén los límites entre orquestación, agentes, proveedores, persistencia y TUI.
- Usa tipos explícitos para contratos entre agentes.
- No uses el LLM como fuente de precios o hechos verificables.
- Conserva provenance: fuente, timestamp, moneda y exchange cuando estén disponibles.
- Devuelve errores accionables y no ocultes fallos con defaults silenciosos.
- Respeta cancelación, timeouts y límites de concurrencia.
- No agregues llamadas de red reales a tests unitarios.
- Añade un mock o fixture determinista para cada comportamiento nuevo.

## Pull requests

Una PR debe incluir:

- Qué problema resuelve.
- Por qué se eligió esa solución.
- Archivos afectados.
- Pruebas ejecutadas.
- Cambios de configuración o migraciones.
- Impacto en privacidad, coste, rate limits o claves.

Para cambios de TUI, incluye una captura o una descripción clara del comportamiento en terminal.

## Estilo de commits

Usa mensajes breves y descriptivos, por ejemplo:

```text
feat(market): add yahoo fallback for restricted symbols
fix(router): tolerate nullable optional fields
test(ui): cover horizontal input scrolling
```

## Licencia

Al contribuir aceptas que tu trabajo se distribuya bajo la licencia MIT del proyecto.
