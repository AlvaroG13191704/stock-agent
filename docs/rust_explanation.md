# Guía Técnica: Flujo de Trabajo en Rust

Este documento explica cómo funciona el código del **Stock Local Agent**, desde la arquitectura de agentes hasta el manejo de la persistencia y la interfaz de terminal (TUI).

## 1. Punto de Entrada (`main.rs`)
El programa utiliza el runtime de **Tokio** (`#[tokio::main]`), que permite ejecutar código asíncrono.
- **`Arc` (Atomic Reference Counting)**: Se utiliza `Arc<T>` para compartir el cliente de Ollama y el gestor de base de datos entre diferentes hilos (UI y Orchestrator) de forma segura.
- **Configuración**: Lee variables de entorno del archivo `.env` y define el modelo `gemma4:31b-cloud` por defecto.

## 2. Persistencia (`storage.rs`)
Utiliza **SQLx** para interactuar con SQLite de forma asíncrona.
- **Migraciones**: Al iniciar, ejecuta las migraciones versionadas de `migrations/`, conservando los datos existentes y creando las tablas `conversations`, `messages`, `user_profiles` y `runs`.
- **Manejo de UUIDs**: Cada mensaje y conversación tiene un identificador único (UUID).
- **Perfiles de Usuario**: Almacena las holdings (acciones) como un array JSON en la columna de texto.

## 3. Arquitectura de Agentes y el Poder de los **Traits**
En Rust, un **Trait** es como un contrato o interfaz de otros lenguajes. Define un conjunto de comportamientos que un tipo debe implementar.

### ¿Qué es el Trait `Agent`?
En `src/agents/mod.rs`, verás:
```rust
#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &str;
    async fn process(&self, messages: &[Message], context: &serde_json::Value) -> Result<AgentOutput>;
}
```
Esto le dice a Rust: *"Cualquier cosa que quiera ser un Agente debe tener un nombre y debe ser capaz de procesar mensajes de forma asíncrona"*.

### ¿Cómo se acoplan al flujo?
1. **Polimorfismo**: Gracias al trait, el `Orchestrator` sabe que no importa si el agente es de noticias (`NewsSearcherAgent`) o educativo (`InformerAgent`), todos responden al mismo método `.process()`. Esto nos permite intercambiarlos o encadenarlos fácilmente.
2. **`#[async_trait]`**: Rust nativamente no permite funciones `async` dentro de un trait (por ahora). Usamos esta macro para habilitar la asincronía en nuestros contratos de agentes.
3. **`BaseAgent` y Composición**: En lugar de usar herencia (que Rust no tiene), usamos composición. Cada agente "tiene" un `BaseAgent` que le da las herramientas básicas (conexión a Ollama, configuración del modelo), y luego implementa el trait `Agent` para su lógica específica.
4. **`Send + Sync`**: Al añadir estos requisitos al trait, garantizamos que nuestros agentes puedan moverse entre diferentes hilos de ejecución de forma segura, lo cual es vital para una aplicación asíncrona impulsada por Tokio.

## 4. El Orquestador (`orchestrator.rs`)
Es el cerebro que coordina el flujo. Cada consulta recibe un `run_id`, un canal de eventos y un token de cancelación. El método `handle_query` tiene un límite total de ejecución de cinco minutos y registra el estado del run (`running`, `completed` o `failed`) en SQLite.
1. **Compresión de Contexto**: Si hay más de 10 mensajes, llama al LLM para resumir la historia antigua, manteniendo solo los últimos mensajes "frescos" para no saturar la ventana de tokens (250k).
2. **Perfilamiento**: Verifica si el perfil del usuario está completo. Si no, fuerza al `ProfileAgent` a hablar.
3. **Enrutamiento**: El `RouterAgent` decide si la pregunta es "Educativa" o una "Investigación".
4. **Pipeline Investigativo**: Si es investigación, ejecuta una cadena secuencial de agentes:
   - `HoldingAnalyzer` -> `NewsSearcher` -> `StockData` -> `Formatter`.

## 5. Interfaz de Terminal (`ui.rs`)
Utiliza `Ratatui` para renderizar la interfaz y `Crossterm` para capturar eventos del teclado.
- **Event Loop**: Un bucle `while` que escucha teclas como `TAB` (cambiar conversación), `F2` (nuevo chat) o `Enter` (enviar mensaje).
- **Eventos de ejecución**: La UI recibe eventos con `run_id` mediante un canal asíncrono. Esto permite mostrar trazas, finalizar el estado de carga y mostrar errores sin consultar una traza global compartida.
- **Cancelación**: Al salir, cambiar de conversación o borrar un chat, se cancela el run activo mediante `CancellationToken`.
- **Async Execution**: Cuando envías un mensaje, se lanza una tarea asíncrona para que la interfaz no se congele mientras el agente piensa o busca en la web.

## 6. Modelado de Datos (`models.rs`)
Define estructuras claras usando **Serde** (`Serialize/Deserialize`).
- Permite convertir objetos de Rust a JSON (para la API de Ollama o la DB) y viceversa con facilidad.

---

### Resumen del Ciclo de Vida de una Pregunta:
1. **Usuario teclea** -> `ui.rs` captura el Enter y crea un `run_id`.
2. **UI envía query** -> `orchestrator.rs` registra el run y emite eventos de progreso.
3. **Orquestador analiza perfil** -> Si falta info, pide al usuario.
4. **Orquestador analiza intención** -> Decide qué agentes especializados usar.
5. **Agentes buscan en Web** -> `OllamaClient` hace peticiones HTTP con timeouts.
6. **Formateador une todo** -> Genera el Markdown final.
7. **Run finaliza** -> Se registra como completado o fallido; la UI siempre libera el estado de carga.
8. **UI renderiza** -> El usuario ve la respuesta y la trazabilidad en colores.
