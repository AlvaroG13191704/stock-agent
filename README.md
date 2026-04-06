# 💹 Stock Local Agent (Cloud Edition)

Un asistente de inversión inteligente basado en Rust que utiliza una arquitectura multi-agente y una interfaz de terminal (TUI) premium para proporcionar análisis de acciones en tiempo real con IA.

## 🚀 Características Principales

- **Orquestación Multi-Agente**: Seis agentes especializados que colaboran para analizar noticias, sentimiento y precios.
  - `Router`: Identifica la intención del usuario.
  - `Profile`: Gestión dinámica del perfil de inversión.
  - `NewsSearcher`: Analista de noticias y sentimiento.
  - `StockData`: Extracción de precios históricos (Hoy, 1S, 1A).
  - `Informer`: Experto educativo en finanzas.
  - `Formatter`: Generador de reportes ejecutivos Markdown.
- **Interfaz TUI Premium**: Construida con `ratatui` para una experiencia inmersiva.
  - **Trazabilidad en Vivo**: Panel lateral que muestra qué agente está activo y qué está haciendo.
  - **Auto-scroll Inteligente**: Scroll manual con flechas que se reactiva al enviar mensajes.
  - **Highlighter de Markdown**: Visualización enriquecida de negritas, títulos y tablas.
- **Arquitectura Asíncrona**: Procesamiento no bloqueante que permite ver cómo la IA "piensa" mientras la interfaz responde a tus entradas.
- **Persistencia Local**: Base de datos SQLite para mensajes y perfiles de usuario.
- **Localización Total**: Interfaz, comandos y cerebros de IA configurados al 100% en Español.

## 🛠️ Requisitos e Instalación

1. **Rust**: Asegúrate de tener instalado el toolchain de Rust (v1.70+).
2. **Ollama Cloud**: Configura tu API Key en el entorno.

```bash
# Clonar e instalar dependencias
cargo build

# Configurar variables de entorno (.env)
OLLAMA_BASE_URL="https://ollama.com"
OLLAMA_API_KEY="tu_api_key_aqui"
DEFAULT_MODEL="gemma4:31b-cloud"
```

## 🎮 Guía de Usuario (Hotkeys)

| Tecla | Acción |
| --- | --- |
| `Enter` | Enviar mensaje |
| `Tab` | Cambiar entre conversaciones |
| `F2` | Nueva conversación |
| `F4` | Borrar conversación actual |
| `F5` | **Reset Perfil** (Reinicia el onboarding) |
| `↑` / `↓` | Scroll manual línea a línea |
| `PgUp` / `PgDn` | Scroll rápido de 10 líneas |
| `Esc` | Salir de la aplicación |

## 📐 Arquitectura Técnica

Para una explicación detallada de cómo funciona el flujo de datos entre los agentes de Rust y el cliente de Ollama, consulta nuestra documentación extendida:

- [📘 Flujo de Trabajo y Traits (Rust)](docs/rust_explanation.md)
- [🏗️ Diagrama de Arquitectura](docs/architecture.md)

---
*Desarrollado con ❤️ para inversores que aman la terminal.*