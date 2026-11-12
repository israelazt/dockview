# Dockview UI — diseño y plan

## Objetivo

Una interfaz elegante y mínima que casi no se ve: el video ocupa toda la ventana y los controles aparecen solo cuando hacen falta. Oscuro y claro, inglés (EE. UU.) y español (Latinoamérica).

## Decisiones tomadas

| Tema | Decisión |
|---|---|
| Tecnología | **Dibujo propio** sobre el D3D11 actual (sin egui). Texto en un atlas de glifos, formas con shaders SDF. |
| Tipografía | **Inter** (OFL). Iconos con una fuente de iconos pequeña (p. ej. Lucide, ISC) tratada como otra fuente. |
| Layout | **Píldora flotante inferior**, translúcida, que aparece al mover el ratón y se oculta a los 3 s. Ajustes en un popover sobre la píldora. |
| Tema | Automático (sigue Windows) + selector manual Auto / Oscuro / Claro. |
| Idioma | Automático (idioma de Windows: `es-*` → español, cualquier otro → inglés) + selector Auto / English / Español. |

## Diseño

**Píldora** (de izquierda a derecha): punto de estado + `Live` · `1080p · 60 fps` — volumen (icono + deslizador) — sincronía (`−  +10 ms  +`) — ajustes — pantalla completa.

**Popover de ajustes:** Video (lista de dispositivos), Audio (lista), Calidad de render (segmentado: Performance / Balanced / Quality), Tema (Auto / Oscuro / Claro), Idioma (Auto / English / Español).

**Tokens de color** (parten de la paleta de la app C#): fondo `#0F0F0F`, texto `#F2F2F2`, texto secundario `#8A8A8A`, acento `#00AAFF`, en vivo `#22C55E`, error `#EF4444`. El tema claro tiene su par invertido con el mismo acento y contraste AA.

**Tipografía:** Inter 13 px para cuerpo, 11 px para etiquetas, pesos Regular y Medium. Los números (fps, volumen, ms) usan celdas de ancho fijo para que no "bailen" al cambiar.

**Movimiento:** fundido de 200 ms al mostrar/ocultar; el cursor se oculta con los controles en pantalla completa.

**Atajos:** se mantienen todos los actuales; los controles en pantalla son una vía adicional.

## Riesgos

1. **Calidad del texto.** Un rasterizador simple (`fontdue`) no aplica el kerning avanzado (GPOS) de Inter. Se decide con una prueba visual en la tarea U-0 (fontdue vs. `cosmic-text`, que sí hace shaping).
2. **HiDPI.** Todo se dimensiona en píxeles lógicos y se escala con el factor de la ventana.
3. **Texto de la ventana nativa.** La barra de título sigue siendo la de Windows en esta fase; los botones de ventana propios quedan para después.

## Tareas

### U-0 · Spike de texto — HECHO
- **U-0.1** ✅ Inter 4.1 (Regular, Medium, SemiBold) y Lucide (iconos) con sus licencias en `rust/crates/dockview-app/assets/fonts/`. Pendiente más adelante: recortar (subset) las fuentes para reducir ~2,7 MB a unos cientos de KB.
- **U-0.2** ✅ Comparación a 11/13/16 px. **Decisión: `cosmic-text`.** Aplica el kerning de Inter (`AVATAR`, `Toy`, `Ty` más ajustados), ligaduras (`fi`, `fl`), espaciado más uniforme y permite números tabulares con `font_features` (`tnum`), cosa que `fontdue` no hace. Coste: un binario de prueba de 3,5 MB con ambas librerías y las dos fuentes; asumible.

### U-1 · Capa de dibujo — HECHO
- **U-1.1** Atlas de glifos (textura R8) y dibujo de texto en un pase de UI sobre el video.
- **U-1.2** Rectángulos redondeados con borde y sombra suave (SDF en el shader), con transparencia.
- **U-1.3** Escalado por factor DPI y clip de regiones.

### U-2 · Tema (U-2.1 hecho: tokens y tests de contraste)
- **U-2.1** `Theme` con tokens oscuro/claro (módulo puro, con tests de contraste).
- **U-2.2** Resolver Auto con `window.theme()` y reaccionar a `ThemeChanged`.

### U-3 · Idiomas (U-3.1 y U-3.2 hechos: tablas, detección del idioma de Windows)
- **U-3.1** Tablas de cadenas `en-US` y `es-419` con una función `tr(Key)`; un test comprueba que ambas tienen las mismas claves.
- **U-3.2** Detección del idioma de Windows; Auto → español si empieza por `es`, si no inglés.
- **U-3.3** Campos `theme` y `language` en `AppSettings` (compatibles con el JSON de la app C#).

### U-4 · Widgets
- **U-4.1** Estado de interacción (ratón, foco, arrastre) y botón de icono.
- **U-4.2** Deslizador (volumen) y control de paso (sincronía).
- **U-4.3** Control segmentado y lista desplegable.

### U-5 · Píldora y popover
- **U-5.1** Píldora inferior con estado, volumen, sincronía, ajustes y pantalla completa.
- **U-5.2** Popover de ajustes: dispositivos, calidad, tema, idioma.
- **U-5.3** Auto-ocultado a 3 s con fundido y cursor oculto en pantalla completa.

### U-6 · Cierre
- **U-6.1** Quitar el título dinámico (la información pasa a la píldora) y los avisos de error visibles en pantalla.
- **U-6.2** Prueba visual en oscuro/claro, español/inglés, 100 %/150 % de escala.
- **U-6.3** Actualizar README con capturas.

## Cómo ver la interfaz

```powershell
dockview.exe --ui-preview --theme dark|light|auto --lang en|es|auto --scale 1.5
dockview.exe --ui-preview --screenshot salida.png   # guarda la ventana y sale
```

Las capturas de referencia están en `rust/ui-previews/`.
