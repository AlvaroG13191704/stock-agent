# Arquitectura del Agente de Bolsa y Flujo de Orquestación

Este documento describe cómo el Agente de Bolsa procesa las consultas de los usuarios, gestiona el perfilado y realiza investigaciones detalladas utilizando un patrón de orquestación multi-agente.

## Descripción General del Sistema

El sistema utiliza una **Arquitectura Agéntica de Tubería y Filtro (Pipe-and-Filter)** donde cada agente es una unidad especializada responsable de una parte específica del análisis de inversión.

## Flujo de Orquestación de Agentes

```mermaid
graph TD
    User([Consulta del Usuario]) --> Store[Guardar en SQLite]
    Store --> Context[Gestor de Contexto]
    
    subgraph "Estrategia de Contexto"
    Context -- "> 10 msgs" --> Summary[Agente de Resumen]
    Context -- "< 10 msgs" --> Active[Búfer Activo]
    Summary --> Active
    end

    Active --> ProfileCheck{¿Perfil Completo?}
    
    subgraph "Modo Incorporación (Onboarding)"
    ProfileCheck -- "No" --> ProfileAgent[Agente de Perfil]
    ProfileAgent --> UserResponse([Pedir Preguntas de Perfil])
    end

    ProfileCheck -- "Sí" --> Router[Agente Enrutador]
    
    subgraph "Detección de Intención y Activos"
    Router -- "Educativo" --> Informer[Agente Informador]
    Router -- "Investigación" --> IsDiscovery{¿Es Descubrimiento?}
    IsDiscovery -- "Sí" --> NewsDisc[Buscador: Encontrar Tickers]
    IsDiscovery -- "No" --> NewsSearch[Buscador: Noticias Reales]
    NewsDisc --> NewsSearch
    end

    subgraph "Pipeline de Investigación"
    NewsSearch --> StockData[Agente de Datos Bursátiles]
    StockData --> Formatter[Agente Formateador]
    end

    Informer --> Formatter
    Formatter --> FinalOutput([Retornar Reporte Premium + Fuentes])
    FinalOutput --> Store
```

## Detalles de los Componentes

### 1. Gestor de Contexto
Para manejar el límite de 250k tokens de manera eficiente, el orquestador "comprime" automáticamente las conversaciones con más de 10 mensajes. Toma los primeros $N-2$ mensajes, genera un resumen semántico y lo antepone a las últimas 2 interacciones para mantener el flujo lógico inmediato sin saturar el modelo.

### 2. Agente de Perfil
Antes de que se brinde cualquier consejo de inversión, el sistema se asegura de conocer la experiencia, nivel de conocimiento, plataformas y tenencias actuales (holdings) del usuario.

### 3. Agente Enrutador
Clasifica la intención del usuario y extrae información sobre los objetivos:
- **Educativo**: Preguntas generales sobre mecánicas del mercado.
- **Investigación**: Análisis profundos de empresas o búsquedas temáticas.
  - Extrae **Tickers** directamente de la consulta.
  - Detecta **Modo Descubrimiento** si el usuario pide ideas nuevas (ej. "Busca acciones de IA").

### 4. Pipeline de Investigación
- **Buscador de Noticias (Modo Dual)**: 
  - **Fase de Descubrimiento**: Identifica tickers relevantes basados en temas.
  - **Fase de Análisis**: Obtiene noticias y sentimiento, preservando las URLs de las fuentes.
- **Datos Bursátiles**: Obtiene precios de "Hoy", "1 semana" y "1 año" para cálculos precisos.
- **Formateador**: Sintetiza todo en un reporte con tablas visuales premium y citación de fuentes.


## Configuración del Modelo en la Nube
El sistema utiliza el modelo en la nube **Gemma 4** para tareas de alto razonamiento.
- **ID del Modelo**: `gemma4`
- **Ventana de Contexto**: 250k tokens (optimizado mediante resumen dinámico).
