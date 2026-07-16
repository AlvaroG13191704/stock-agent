## Resumen

Describe el cambio y el problema que resuelve.

## Tipo de cambio

- [ ] Bug fix
- [ ] Nueva funcionalidad
- [ ] Refactor
- [ ] Documentación
- [ ] Cambio de configuración o migración

## Validación

- [ ] `cargo fmt -- --check`
- [ ] `cargo check --locked`
- [ ] `cargo test --locked`
- [ ] `cargo clippy --all-targets --all-features --locked -- -D warnings`
- [ ] `git diff --check`

## Seguridad y operación

- [ ] No incluí secretos ni datos privados.
- [ ] Los tests nuevos no dependen de APIs externas.
- [ ] Documenté cambios de configuración, costes o rate limits.
- [ ] Añadí o actualicé mocks para los comportamientos nuevos.

## Notas para revisión

Incluye riesgos, decisiones de diseño y próximos pasos.
