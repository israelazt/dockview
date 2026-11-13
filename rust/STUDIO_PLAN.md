# Modo Estudio — diseño y plan (2026-11-12)

## Por qué

La píldora y sus popovers ya no alcanzan: micrófono, replay, grabación, fuentes, escenas, estadísticas y ajustes compiten por el mismo espacio, y el editor de escenas (E-3c) necesita mucho más que un popover. Hay pantalla de sobra: se separan las funciones en un panel propio.

## Diseño

Dos modos con la misma base:

- **Ver** (el actual). El video ocupa toda la ventana; la píldora flotante aparece al mover el ratón y se oculta sola. Solo lleva lo esencial.
- **Estudio** (nuevo). Un **panel lateral acoplado a la derecha** (320 px lógicos). El video se reduce al espacio restante manteniendo su proporción (no se tapa). El panel tiene secciones, cada una con su espacio.

```
Modo Ver                         Modo Estudio
┌──────────────────────────┐     ┌────────────────────┬─────────────┐
│                          │     │                    │ ▸ Fuentes   │
│          VIDEO           │     │       VIDEO        │ ▸ Escenas   │
│                          │     │   (se reduce)      │ ▸ Audio     │
│   ( píldora flotante )   │     │                    │ ▸ Grabación │
└──────────────────────────┘     │  ( píldora )       │ ▸ Ajustes   │
                                 │                    │ ▸ Stats     │
                                 └────────────────────┴─────────────┘
```

**Píldora (modo Ver y Estudio):** estado, resolución/fps, volumen, sincronía, grabar, guardar clip, **botón Estudio**, pantalla completa. Los botones de fuentes, escenas, replay y ajustes pasan a abrir su sección del panel (bloque D).

**Cambiar de modo:** botón en la píldora **y** tecla **Tab** (hoy libre). Esc cierra el panel si está abierto. El estado se guarda en `settings.json` (`studioOpen`).

**Reglas**
- El modo Ver debe verse **idéntico** al actual hasta el bloque D (capturas píxel a píxel).
- El panel respeta «Mostrar animaciones» de Windows: se abre con fundido y deslizamiento de ~180 ms, o al instante.
- Pantalla completa oculta el panel (se ve solo el video); al salir, vuelve el modo que había.
- La latencia del visor (~1,8 ms sin grabar) no puede empeorar; se mide en el bloque E.
- Dónde vive cada cosa: reglas y estado en `dockview-domain` (con tests), el panel y sus widgets en `dockview-ui`, el rectángulo del video en `dockview-platform` (renderer), el cableado en `shell/`. Las guardas de tamaño (400 líneas) y de dependencias siguen vigentes.

## Cómo se trabaja

Cada tarea es atómica: un cambio pequeño, con sus tests, que deja el repositorio compilando y con `fmt`, `clippy` sin avisos y `cargo test --workspace` en verde. **Una tarea = un commit + push.** Al terminar un **bloque** completo se hace una **pausa para revisar** lo hecho (capturas, app real, decisiones) antes de empezar el siguiente.

Verificación estándar de cada bloque: `ui_shots.ps1` + `ui_compare.py` contra la línea base, y una ejecución real (`--video C7 --record`).

## Bloque A — Cimientos (sin cambio visible)

| Tarea | Qué | Listo cuando |
|---|---|---|
| A-1 | `domain`: `Model.studio: bool` y `Action::ToggleStudio` / `SetStudio(bool)`; reglas puras con tests | Tests de alternar y de pantalla completa que lo oculta |
| A-2 | `domain` (o `ui`, puro): función `studio_layout(ventana, escala, abierto) -> {video, panel}` con anchos mínimos y máximos | Tests: ventana pequeña, grande, HiDPI, cerrado = todo el video |
| A-3 | `platform`: `letterbox_in(rect, video)` y `Renderer::set_video_rect(Option<Rect>)`; sin rectángulo = igual que hoy | Tests de `letterbox_in`; capturas idénticas |
| A-4 | `settings`: campo `studioOpen` opcional, compatible con archivos viejos | Test de cargar un `settings.json` anterior |
| A-5 | `app`: `Core` guarda/restaura el modo y atiende `ToggleStudio` (aún sin interfaz que lo pida) | Test en `Core` con ajustes en memoria |

**Pausa A:** revisar que nada cambió a la vista y que el diseño de A-2 sirve.

## Bloque B — Armazón del panel

| Tarea | Qué | Listo cuando |
|---|---|---|
| B-1 | `ui`: widget `Panel` (fondo, borde, cabecera, lista de secciones plegables, desplazamiento con rueda) | Tests de plegar/desplegar y de desplazamiento; captura del panel vacío |
| B-2 | `ui`: animación de abrir/cerrar (ancho + fundido), instantánea sin animaciones | Tests de la curva y del caso sin animaciones |
| B-3 | `ui`: botón Estudio en la píldora (icono + tooltip) y textos i18n en/es | Captura de la píldora; guarda de i18n verde |
| B-4 | `shell`: Tab alterna el modo; Esc cierra el panel; se guarda en ajustes | Prueba con ventana real (Tab abre y cierra) |
| B-5 | `shell`: calcula `studio_layout` cada fotograma y llama a `set_video_rect` | El video se reduce sin deformarse; captura con el panel abierto |
| B-6 | `ui`: el puntero sobre el panel no cuenta como puntero sobre el video; el cursor y el auto-ocultado de la píldora lo respetan | Tests de ruteo del puntero |

**Pausa B:** abrir y cerrar el panel en la app real, medir latencia, revisar el aspecto.

## Bloque C — Secciones dentro del panel

Cada sección se construye en el panel **sin quitar** todavía su popover.

| Tarea | Sección | Contenido |
|---|---|---|
| C-1 | Ajustes | Tema, idioma, calidad de render |
| C-2 | Fuentes | Capturadoras, pantallas y ventanas (hoy en el popover de fuentes) |
| C-3 | Grabación y replay | Calidad, carpeta, última grabación, repetición instantánea |
| C-4 | Audio | Volumen, sincronía, micrófono y su nivel |
| C-5 | Escenas | Lista, activar, crear, borrar, añadir fuente |
| C-6 | Estadísticas | FPS, latencia, CPU/GPU (hoy un panel flotante aparte) |

Por tarea: la sección usa las mismas `Action` que el popover (no se duplica lógica), tiene tests con el `Rig` de `controls/tests.rs` y una captura.

**Pausa C:** revisar las seis secciones juntas; decidir qué popovers sobran.

## Bloque D — Limpieza de la píldora

| Tarea | Qué | Listo cuando |
|---|---|---|
| D-1 | Los botones de la píldora abren el panel en su sección (en vez del popover) | Test: clic en «escenas» abre Estudio en Escenas |
| D-2 | Quitar los popovers migrados y su código muerto | Archivos más pequeños; tests actualizados |
| D-3 | Borrar confirmación al eliminar una escena (límite conocido de E-3b) | Test del diálogo |
| D-4 | Renombrar una escena desde la interfaz (límite conocido de E-3b) | Test del renombrado |

**Pausa D:** el modo Ver cambia de aspecto aquí (píldora más corta). Revisar con capturas nuevas y fijar la línea base.

## Bloque E — Calidad y cierre

| Tarea | Qué | Listo cuando |
|---|---|---|
| E-1 | Medir latencia con el panel abierto y cerrado (`--show-stats`, sin y con grabación) | Cifras anotadas; sin empeorar |
| E-2 | Ventana de 720p, 1080p y pantalla completa; escala 100 %, 125 %, 150 % | Capturas de las combinaciones |
| E-3 | Grabar con el panel abierto: el archivo no contiene el panel | MP4 revisado (video + 3 pistas) |
| E-4 | Actualizar `ARCHITECTURE.md`, `UI_PLAN.md` y este plan; guardar el estado en memoria | Docs al día |

**Pausa E:** revisión final del modo Estudio.

## Después

Con el panel hecho se retoma `CAPTURE_PLAN.md`: la prueba del morph con clics reales, **E-3c** (editor libre: mover y redimensionar capas sobre el video, lista de capas en la sección Escenas) y E-3d.

## Riesgos

1. **Rectángulo del video.** Hoy el renderer calcula el letterbox con la ventana entera; A-3 lo generaliza. Hay que cuidar que las capturas de pantalla (`--screenshot`) y el compositor no asuman la ventana completa.
2. **Texto y espacio.** 320 px con Inter 13 px obliga a revisar el largo de los textos en español (suele ser más largo que en inglés); B-1 y C-* lo comprueban con capturas.
3. **Dos caminos de código a la vez.** Entre los bloques C y D conviven popover y sección. Se acepta para poder comparar y revertir; D limpia.
