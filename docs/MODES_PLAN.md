# Luminifex: cada modo hace lo suyo (2026-11-16)

## Por qué

Luminifex tiene tres modos: **Visor directo**, **Modo Estudio** y **Ajustes**. Hoy se mezclan: Ajustes no es un sitio propio (abre el panel de Estudio en su sección «Ajustes»), la transición Suave/Corte está en Ajustes y en la barra del Estudio, las estadísticas están en el visor y en el Estudio, el visor puede cambiar de fuente, y una sola `Model` y una sola `Action` (unas 50 variantes planas) sirven a todas las pantallas, así que cualquiera puede leer y pedir cualquier cosa.

Objetivo: **en el visor, solo lo del visor; en el Estudio, solo lo del Estudio; en Ajustes, solo ajustes.** Y que el código lo imponga, no solo la costumbre.

**Alcance (decidido 2026-11-16):** desde ahora se trabaja **solo en Luminifex**. Capifex y Motifex quedan en pausa; los bloques 1 a 5 de `FEXARIUM_PLAN.md` esperan. Lo que ya está hecho de ese plan (monorepo, apps/luminifex, tres pestañas) se queda.

## Quién es dueño de qué

Decidido con el autor el 2026-11-16 (Ajustes: vista propia; grabar: botones rápidos en el visor y configuración en Estudio; el visor: solo mirar y lecturas; audio: volumen y micrófono en el visor, sincronía en Estudio). Lo marcado con (*) es una consecuencia que propongo y que se confirma al llegar a su tarea.

### Visor directo — mirar
- La imagen y la pantalla completa (`F`, `F11`, `Esc`).
- La píldora de acciones rápidas: **Guardar clip**, **Grabar**, **volumen**, **silenciar el micrófono**, **mostrar u ocultar las lecturas**, **pantalla completa**.
- Las **lecturas** (HUD): nombre de la fuente, latencia, fps mostrados, pérdidas por segundo, RAM, CPU, GPU, estado del replay. Solo se leen: **no abren la lista de fuentes**.
- Teclas propias: volumen (`↑` `↓`).
- (*) El panel flotante de estadísticas (`I`) sale del visor: lo detallado es del Estudio. `I` abre el Estudio en Estadísticas.
- (*) Cambiar de capturadora o de entrada de audio con `V` y `A` es de Estudio, no del visor.

### Modo Estudio — producir
- **Escenas** (lista, atajos `F1`–`F9`, editor de capas sobre la imagen) y **Capas y fuentes**.
- **Fuentes:** capturadora, monitor o ventana como fuente principal.
- La **barra de transición** y las herramientas del lienzo (imán, cuadrícula, ajustar).
- **Audio:** dispositivos de entrada y de micrófono y la **sincronía** (`←` `→`).
- **Grabación y replay:** la configuración (repetición instantánea, duración, calidad, carpeta) y sus botones.
- **Emisión** en directo.
- **Estadísticas y diagnóstico** completos.
- Teclas propias: `Ctrl+Z` en el editor, `V`, `A`, `←` `→`, `F1`–`F9`.

### Ajustes — configurar la app
- **Calidad de imagen** (`P` la cambia), **modo de vídeo**, **tema**, **idioma**.
- Opciones de la app: mantener en la bandeja al cerrar la ventana (el replay sigue con la ventana cerrada).
- **No** lleva la transición (es del Estudio) ni nada de grabación (es del Estudio).
- Es una **vista propia**: al abrirla reemplaza el contenido (sin vídeo ni escenas) y se vuelve con las pestañas.

### Compartido (el marco de la ventana, no de un modo)
Barra de título, fila de pestañas con el estado de la derecha (codificador, pérdidas, emisión), avisos, tema e idioma resueltos, pantalla completa y su animación. Nada de esto toma decisiones de un modo.

## Cómo se impone en el código

1. **Un modo, un módulo, un modelo, una acción.** `Model` se parte en lo compartido (`ShellView`: tema, idioma, modo, pantalla completa, estado) y una vista por modo (`ViewerView`, `StudioView`, `SettingsView`). `Action` se parte en `Action::{Viewer, Studio, Settings, App}` con un enum por modo; `App` lleva lo del marco (cambiar de modo, pantalla completa, salir).
2. **Un modo es el estado, no un `bool`.** `Mode::{Viewer, Studio, Settings}` sustituye a `studio: bool` + `nav_tab` + `settings` suelto.
3. **En `ui`:** carpetas `viewer/`, `studio/`, `settings/` y `shell/` (el marco). Un modo **no importa** a otro; solo ve su vista y lo compartido.
4. **En `app`:** el reparto de acciones en `core/` también se parte por modo (`core/viewer.rs`, `core/studio/`, `core/settings.rs`); los servicios (replay, fuentes, escenas…) siguen siendo de `Core` y **no pertenecen a ningún modo**: los modos piden, los servicios hacen.
5. **Guarda automática** (`tests/architecture.rs`): un archivo de `ui/viewer` no puede nombrar `studio` ni `settings`, ni al revés; y en `app`, `core/viewer` no puede tocar lo de `core/studio`. Falla con un mensaje que dice qué importó quién.

## Cómo se trabaja

Igual que el resto: tarea atómica (cambio pequeño con tests; `fmt`, `clippy -D warnings` y `cargo test --workspace` en verde), **un commit + push por tarea**, **pausa de revisión al terminar cada bloque**. Las capturas de la interfaz (`ui_shots.ps1` + `ui_compare.py` o `--screenshot`) antes y después de cada bloque que toca lo que se ve. La app real solo cuando no se esté jugando y sin abrirla si el autor revisa a mano.

## Bloque A — El modo es el estado

| Tarea | Qué | Listo cuando |
|---|---|---|
| A-1 | `domain`: `Mode::{Viewer, Studio, Settings}` y su transición (qué abre cada pestaña, qué hace `Esc`, `Tab`); sustituye `NavTab` y el `studio: bool` del modelo | Tests de las transiciones; la app se ve igual |
| A-2 | `Core`: el modo vive en un solo sitio (`view.mode`); `studio_open` de los ajustes guardados se mantiene por compatibilidad (restaura Estudio) | Los tests del arnés en verde |
| A-3 | La interfaz lee `model.mode`; el panel de Estudio deja de abrirse para Ajustes (Ajustes muestra una vista vacía por ahora) | Capturas de Visor y Estudio idénticas |

**Pausa A.**

## Bloque B — Ajustes es una vista propia

| Tarea | Qué | Listo cuando |
|---|---|---|
| B-1 | `ui/settings/`: la vista completa (calidad de imagen, modo de vídeo, tema, idioma, bandeja) con su `SettingsView` y `SettingsAction` | Cada elección cambia el ajuste y se guarda |
| B-2 | Quitar la sección «Ajustes» del panel de Estudio y la transición de Ajustes; la transición queda solo en la barra del Estudio | `grep`: un solo sitio para la transición |
| B-3 | La tecla `P` y los textos de Ajustes; `Esc` y las pestañas vuelven al modo anterior | Tests del arnés |

**Pausa B.**

## Bloque C — El visor, solo mirar

| Tarea | Qué | Listo cuando |
|---|---|---|
| C-1 | `ui/viewer/`: la píldora (sin sincronía de audio), el HUD y la grabación en curso, con `ViewerView` y `ViewerAction` | El visor solo emite sus acciones |
| C-2 | El HUD deja de abrir la lista de fuentes; la tarjeta de fuente es solo lectura | Sin `source_picker` en el visor |
| C-3 | El panel flotante de estadísticas sale del visor; `I` abre Estudio en Estadísticas | Tests de la tecla |
| C-4 | Teclas por modo: `V`, `A`, `←` `→`, `Ctrl+Z` solo en el Estudio | Test de que en el visor no hacen nada |

**Pausa C.**

## Bloque D — El Estudio, solo producir

| Tarea | Qué | Listo cuando |
|---|---|---|
| D-1 | `StudioView` y `StudioAction`; las secciones del panel quedan: Fuentes, Escenas, Audio (con sincronía), Grabación, Estadísticas | `Action` del Estudio separada |
| D-2 | La lista de fuentes del Estudio absorbe lo que el visor hacía (cambiar de fuente) y el selector flotante desaparece o se vuelve interno del Estudio | Un solo selector |
| D-3 | La emisión (URL, clave, estado) en su sección o en Grabación, sin tocar nada del visor | Sección clara |

**Pausa D.**

## Bloque E — Guardas, documentos y memoria

| Tarea | Qué | Listo cuando |
|---|---|---|
| E-1 | Guardas de `architecture.rs`: ningún módulo de `ui` ni `core` importa a otro modo | Fallan si alguien lo rompe |
| E-2 | `ARCHITECTURE.md` (la sección de modos), `README.md` y la memoria al día | Docs sin la mezcla vieja |

## Riesgos

1. **Partir `Model` y `Action` toca casi todos los archivos de `ui` y `core`.** Por eso va en tareas pequeñas, con una migración que mantenga los tests en verde (aliases temporales) y las capturas de la interfaz como prueba.
2. **Los atajos globales de replay y grabación** (`Ctrl+Alt+S`, `Ctrl+Alt+R`) siguen siendo de Luminifex, no de un modo: siguen funcionando con la ventana cerrada.
3. **Quitar cosas del visor** (la lista de fuentes, el panel `I`, `V`/`A`) cambia hábitos del autor: se confirma cada una en su tarea y se deja constancia aquí.
4. **El panel de Estudio hoy guarda cuál sección estaba abierta**; al quitar Ajustes de él hay que mantener compatibilidad con lo guardado.
