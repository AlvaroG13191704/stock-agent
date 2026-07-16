# Changelog

Este archivo resume cambios relevantes del proyecto. El formato sigue una convención inspirada en [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Milestone 5 — Public preview

- README público con audiencia, instalación, arquitectura, límites y troubleshooting.
- `.env.example` consolidado y documentado.
- `DATABASE_URL` configurable desde el entorno.
- Guía de pruebas con mocks deterministas.
- Documentación de contribución y seguridad.
- Licencia MIT.
- Mock reutilizable de proveedor de mercado.
- Mock HTTP local para respuestas de Ollama y uso de tokens.
- Prueba de fallback de proveedor.

### Milestone 4 — Resiliencia y datos

- Finnhub como proveedor opcional con Yahoo Finance como fallback.
- Caché TTL de datos de mercado.
- Resolución de tickers mediante evidencia web cuando los proveedores no encuentran el símbolo.
- Manejo de permisos HTTP 403 con mensajes explicativos.
- Reintentos limitados para errores de red, timeouts, 429 y 5xx.
- Resultados parciales para investigación multi-ticker.

### Milestone 3 — Orquestación y TUI

- Uso de tokens de entrada, salida y total.
- Etapas tipadas de progreso.
- Reintento de consultas fallidas desde la TUI.
- Cursor Unicode y scroll horizontal en la entrada.
- Loop asíncrono con `EventStream`.
- Concurrencia limitada para noticias y datos de mercado.

## Próximo

- CI multiplataforma.
- Tests end-to-end con fixtures HTTP reutilizables.
- Streaming de respuestas del modelo.
- Observabilidad opcional con logging estructurado.
