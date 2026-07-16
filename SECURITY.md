# Seguridad

## Alcance

Stock Local Agent procesa consultas, contenido web y credenciales de proveedores. No ejecuta operaciones financieras ni tiene acceso a cuentas de broker.

## Secretos

Nunca publiques:

- `OLLAMA_API_KEY`.
- `MARKET_API`.
- Archivos `.env`.
- Bases SQLite con conversaciones reales.
- Logs que contengan cabeceras o URLs con tokens.

El repositorio ignora `.env` y `stock_agent.db` por defecto. Usa `.env.example` como plantilla sin secretos.

Las claves se leen desde variables de entorno y no deben imprimirse en trazas ni incluirse en URLs de reportes. Si una clave se filtra, revócala en el proveedor inmediatamente.

## Reportar una vulnerabilidad

No abras una issue pública para una vulnerabilidad que pueda contener secretos o permitir acceso no autorizado. Contacta a los mantenedores mediante el canal privado disponible en el perfil del repositorio y proporciona:

- Descripción del problema.
- Pasos mínimos para reproducirlo.
- Impacto estimado.
- Versión o commit afectado.
- Mitigación conocida, si existe.

No adjuntes claves reales, conversaciones privadas ni dumps de bases de datos.

## Riesgos conocidos

- El contenido de búsqueda web es externo y no confiable; los agentes lo delimitan y deben ignorar instrucciones incrustadas.
- Las APIs de mercado tienen límites, retrasos, cobertura variable y errores de permisos.
- El modelo puede producir interpretaciones incorrectas; el sistema separa datos de interpretación, pero no sustituye una verificación humana.
- El modo TUI usa una base SQLite local sin cifrado a nivel de aplicación; protege el archivo y el equipo donde se ejecuta.
