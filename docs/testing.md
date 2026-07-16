# Estrategia de pruebas

El proyecto prioriza pruebas deterministas que no dependan de Ollama, Finnhub, Yahoo ni credenciales reales. Las pruebas públicas deben poder ejecutarse después de clonar el repositorio:

```bash
cargo test --locked
```

## Mocks disponibles

### `MockMarketDataProvider`

Está definido en `src/test_support.rs` y se compila únicamente durante tests. Permite configurar:

- Una respuesta exitosa con precio, moneda, exchange y URL de origen.
- Un error controlado.
- Un contador de llamadas para validar caché y fallback.

Esto permite probar agentes sin llamar a un proveedor externo.

### Proveedores secundarios

`FallbackMarketDataProvider` se prueba con un proveedor primario que falla y un secundario mock que responde correctamente. Este caso valida la política:

```text
primary error → fallback success
```

Si ambos proveedores fallan, el error conserva el contexto de ambas fuentes.

### Caché

La caché TTL se prueba verificando que dos solicitudes equivalentes (`aapl` y `AAPL`) produzcan una sola llamada al proveedor interno mientras la entrada siga vigente.

### Ollama HTTP mock

`src/ollama.rs` contiene un servidor TCP local mínimo para devolver una respuesta HTTP controlada. Se valida que `OllamaClient`:

- Decodifica una respuesta de chat.
- Lee `prompt_eval_count`.
- Lee `eval_count`.
- No necesite una API key real.

### Parsing y contratos

También se prueban:

- JSON limpio.
- JSON dentro de bloques Markdown.
- Arrays JSON.
- JSON precedido por texto.
- Llaves dentro de strings.
- `RouteDecision` con valores opcionales `null`.
- Normalización y validación de tickers.

## Qué no prueban los mocks

Los mocks no garantizan que una API externa siga disponible ni que una cuenta tenga permisos sobre un endpoint. Para una validación manual opcional:

1. Configura credenciales localmente en `.env`.
2. No las compartas ni las incluyas en logs.
3. Prueba una consulta de bajo volumen.
4. Verifica la URL y la fecha de cada fuente del informe.

Las pruebas unitarias nunca deben depender de esa validación manual.

## Checklist antes de publicar

```bash
cargo fmt -- --check
cargo check --locked
cargo test --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```

Para observar nombres y mensajes de tests:

```bash
cargo test --locked -- --nocapture
```
