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
2. **Un modo es el estado, no un `bool`.** `AppMode::{Viewer, Studio, Settings}` (se llama `AppMode` porque `Mode` ya son los modos de recorte de pantallazo) sustituye a `studio: bool` + `nav_tab` + `settings` suelto.
3. **En `ui`:** carpetas `viewer/`, `studio/`, `settings/` y `shell/` (el marco). Un modo **no importa** a otro; solo ve su vista y lo compartido.
4. **En `app`:** el reparto de acciones en `core/` también se parte por modo (`core/viewer.rs`, `core/studio/`, `core/settings.rs`); los servicios (replay, fuentes, escenas…) siguen siendo de `Core` y **no pertenecen a ningún modo**: los modos piden, los servicios hacen.
5. **Guarda automática** (`tests/architecture.rs`): un archivo de `ui/viewer` no puede nombrar `studio` ni `settings`, ni al revés; y en `app`, `core/viewer` no puede tocar lo de `core/studio`. Falla con un mensaje que dice qué importó quién.

## Cómo se trabaja

Igual que el resto: tarea atómica (cambio pequeño con tests; `fmt`, `clippy -D warnings` y `cargo test --workspace` en verde), **un commit + push por tarea**, **pausa de revisión al terminar cada bloque**. Las capturas de la interfaz (`ui_shots.ps1` + `ui_compare.py` o `--screenshot`) antes y después de cada bloque que toca lo que se ve. La app real solo cuando no se esté jugando y sin abrirla si el autor revisa a mano.

## Bloque A — El modo es el estado

| Tarea | Qué | Listo cuando |
|---|---|---|
| A-1 ✅ | `domain`: `AppMode::{Viewer, Studio, Settings}` (antes `NavTab`) y `Model.mode` en lugar de `studio: bool` y `nav.tab` | Tests de las transiciones; la app se ve igual |
| A-2 ✅ | `Core`: el modo vive en un solo sitio (`view.mode`, con `last_panel` para `Tab`); `studio_open` de los ajustes guardados se mantiene por compatibilidad (restaura Estudio) | Los tests del arnés en verde |
| A-3 ✅ | La interfaz lee `model.mode` (la fila de pestañas, el HUD y el panel). **Cambio respecto a lo previsto:** Ajustes sigue usando el panel de Estudio hasta el bloque B, para que la app siga siendo usable; `AppMode::shows_panel` lo dice | Tests y la app se ven igual |

**Pausa A.**

## Bloque B — Ajustes es una vista propia

**Hecho (2026-11-16).** `AppMode::shows_panel` ahora es solo el Estudio; Ajustes tiene su `SettingsView`/`SettingsAction` (`domain/settings_view.rs`), su vista (`ui/settings/`) y su parte de `Core` (`core/settings_actions.rs`); el Estudio quedó con 5 secciones (Fuentes, Escenas, Audio, Grabación, Estadísticas). Las acciones `SetPreset`/`SetTheme`/`SetLang`/`SetVideoMode` ya no existen como `Action` sueltas. En la ventana, el modo Ajustes oculta la imagen (1 píxel) y dibuja solo su vista y los avisos.

| Tarea | Qué | Listo cuando |
|---|---|---|
| B-1 ✅ | `ui/settings/`: la vista completa (calidad de imagen, modo de vídeo, tema, idioma, bandeja) con su `SettingsView` y `SettingsAction` | Cada elección cambia el ajuste y se guarda |
| B-2 ✅ | Quitar la sección «Ajustes» del panel de Estudio y la transición de Ajustes; la transición queda solo en la barra del Estudio | `grep`: un solo sitio para la transición |
| B-3 ✅ | `Esc` y las pestañas vuelven al modo anterior (Ajustes recuerda de dónde vino); `Tab` abre el Estudio desde cualquier modo. **`P` sigue siendo una tecla global** que cambia la calidad de imagen en cualquier modo; el valor se ve en Ajustes | Tests del arnés |

**Pausa B.**

## Bloque C — El visor, solo mirar

| Tarea | Qué | Listo cuando |
|---|---|---|
| C-1 ✅ | La píldora sin sincronía de audio (la sincronía queda en la sección Audio del Estudio). **Pendiente:** carpeta `ui/viewer/` con `ViewerView` y `ViewerAction` | El visor solo emite sus acciones |
| C-2 ↩ | **Revertida (2026-11-16, a petición del autor):** el HUD vuelve a abrir la lista de fuentes del visor (`source_picker`), porque cambiar de lo que se captura desde el Estudio no le sirve para mirar. El Estudio conserva también su lista «Capas y fuentes» | Dos sitios para elegir la fuente, ambos con las mismas acciones |
| C-3 ✅ | El panel flotante de estadísticas se borró; el botón del pulso del HUD (`Action::OpenStats`) abre el Estudio en Estadísticas, y `--stats` también. No existía una tecla `I` | Tests |
| C-4 ✅ | Teclas por modo: `V`, `A`, `←` `→`, `Ctrl+Z` solo en el Estudio; salir del Estudio termina la edición de capas; el volumen (`↑` `↓`) sigue en todos los modos | Tests |

**Pausa C.**

## Bloque D — El Estudio, solo producir

| Tarea | Qué | Listo cuando |
|---|---|---|
| D-1 ◐ | Las secciones del panel quedan: Fuentes, Escenas, Audio (con sincronía), Grabación, **Emisión**, Estadísticas. **Pendiente:** `StudioView` y `StudioAction` como tipos propios (hoy siguen en el `Action` plano) | `Action` del Estudio separada |
| D-2 ✅ | El selector flotante del visor se borró (C-2); cambiar de fuente es solo de la lista «Capas y fuentes» del Estudio | Un solo selector |
| D-3 ✅ | Sección **Emisión** del Estudio: servicio (Twitch, YouTube, Kick u otro), dirección del servidor, clave (se escribe y no se vuelve a mostrar), imágenes por segundo, bitrate, botón Emitir/Detener y estado en vivo. Acciones nuevas `SetStreamFps`/`SetStreamKbps`; el campo de texto del panel sirve ya para el nombre de escena, la dirección y la clave (`EditTarget`) | Sección clara |

**Pausa D.**

## Bloque E — Guardas, documentos y memoria

| Tarea | Qué | Listo cuando |
|---|---|---|
| E-1 ✅ | Guarda de `architecture.rs` para `ui`: ningún módulo de un modo nombra un módulo de otro (visor, Estudio, Ajustes). **Pendiente:** la guarda equivalente para `core` | Falla si alguien lo rompe |
| E-2 ✅ | `ARCHITECTURE.md` (sección «Los tres modos»), `README.md` (cómo emitir) y la memoria al día | Docs al día |

## Riesgos

1. **Partir `Model` y `Action` toca casi todos los archivos de `ui` y `core`.** Por eso va en tareas pequeñas, con una migración que mantenga los tests en verde (aliases temporales) y las capturas de la interfaz como prueba.
2. **Los atajos globales de replay y grabación** (`Ctrl+Alt+S`, `Ctrl+Alt+R`) siguen siendo de Luminifex, no de un modo: siguen funcionando con la ventana cerrada.
3. **Quitar cosas del visor** (la lista de fuentes, el panel `I`, `V`/`A`) cambia hábitos del autor: se confirma cada una en su tarea y se deja constancia aquí.
4. **El panel de Estudio hoy guarda cuál sección estaba abierta**; al quitar Ajustes de él hay que mantener compatibilidad con lo guardado.

## Estado al cerrar (2026-11-16)

Bloques A, B, C, D y E hechos, con dos cosas aplazadas a propósito: partir `Model`/`Action` por modo (`ViewerView`/`ViewerAction`, `StudioView`/`StudioAction`) y la guarda de modos para `core`. Además se añadió lo necesario para **poder emitir**: la sección Emisión del Estudio y el pegado con `Ctrl+V` en los campos de texto del panel.

**Para emitir por primera vez** (la prueba real contra un servidor sigue pendiente, por decisión del autor): abrir Luminifex, Modo Estudio, sección **Emisión**; elegir el servicio; pegar la clave de emisión (`Ctrl+V`) y pulsar Intro; ajustar imágenes por segundo y bitrate (30 fps y 4500 kbps por defecto sirven para Twitch); pulsar **Emitir**. La fila de pestañas muestra «Conectando…» y luego «En directo». La dirección de Kick cambia de vez en cuando: si no conecta, se edita a mano en el campo de la dirección.
