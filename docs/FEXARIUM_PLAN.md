# Fexarium: desacoplar la app y crear el monorepo (2026-11-16)

> **En pausa desde 2026-11-16:** el autor decidió trabajar **solo en Luminifex**. El Bloque 0 (monorepo) está hecho; los bloques 1 a 5 esperan. Ahora manda `MODES_PLAN.md` (cada modo hace lo suyo).

## Qué se decidió

- La familia de productos se llama **Fexarium** (el nombre es **tentativo**: se guarda en un solo sitio para cambiarlo con una línea).
- **Luminifex**: solo visor directo y modo Estudio (captura, escenas, grabación, replay, emisión).
- **Capifex**: el capturador de pantallazos, **app aparte y funcional**, residente.
- **Motifex**: el editor de vídeo, app aparte. **En esta tarea solo se reserva su sitio** (carpeta y README); no se escribe código.
- Un **monorepo** (este mismo repositorio, en esta rama), con un solo workspace de Cargo.
- El C# antiguo de la raíz **se borra de la rama** (la historia de git lo conserva en `main`).
- Capifex pasa a ser una app funcional, no solo código movido.

Lo que no se decidió y aquí se propone (se confirma al pasar por cada bloque):
- **Crates compartidos:** `fexarium-kit`, `fexarium-ui`, `fexarium-win`. Los de cada app llevan su nombre (`luminifex-*`, `capifex-*`).
- **Datos:** `%APPDATA%\Fexarium\<App>\` (`Luminifex`, `Capifex`, `Motifex`), con una carpeta común para lo compartido. Luminifex copia el `settings.json` viejo de `%APPDATA%\Dockview` la primera vez, sin borrarlo; Capifex lee de ahí la sección `capture` la primera vez.
- **Clips y grabaciones:** la carpeta que el usuario ya eligió se respeta; la carpeta por defecto nueva es `Vídeos\Luminifex`. Lo que ya existe en `Vídeos\Dockview` no se mueve.
- **Bandeja y atajos de replay en Luminifex:** se quedan (hacen falta para grabar con la ventana cerrada). Capifex tiene su propia bandeja y su propio atajo.

## Cómo queda el repositorio

```
/Cargo.toml                 el workspace
/README.md  /ARCHITECTURE.md
/docs/                      los planes (STUDIO_PLAN, REDESIGN_PLAN, HARDENING_PLAN…)
/apps/luminifex/            binario  luminifex.exe   (hoy crates/dockview)
/apps/capifex/              binario  capifex.exe
/apps/motifex/              solo README: reservado
/crates/
    fexarium-kit/           compartido, portable: tema, tamaños, logo, registro, geometría, datos de dibujo, atajos (texto), traducciones (mecanismo)
    fexarium-ui/            compartido: el motor de interfaz (Ui, texto, entrada, movimiento, avisos)
    fexarium-win/           compartido, Windows: GPU y dibujo de la interfaz, ventana de presentación, atajos globales, bandeja, registro en archivo
    luminifex-domain/       lo de Luminifex que hoy está en dockview-domain
    luminifex-stream/       hoy dockview-stream
    luminifex-platform/     hoy dockview-platform, sin lo compartido ni lo de Capifex
    luminifex-ui/           las pantallas de Luminifex (controles, HUD, Estudio…)
    luminifex-core/         hoy dockview-app (Core y servicios), sin el capturador
    capifex-core/           reglas y servicio del capturador (selección, objetivo, ajustes, servicio)
    capifex-win/            captura de pantalla fija, ventana de selección, portapapeles, archivo de imagen
/installer/  /tools/  /design/
```

Las dependencias solo van hacia abajo, y las apps no se ven entre sí:

```
apps/luminifex                       apps/capifex
  luminifex-{core,ui,platform}         capifex-{core,win}
        luminifex-domain  luminifex-stream
              \                     /
          fexarium-ui   fexarium-win
                  fexarium-kit
```

## Cómo se trabaja

Igual que el resto de los planes: cada tarea es atómica (un cambio con sus tests, `fmt`, `clippy -D warnings` y `cargo test --workspace` en verde), **un commit + push por tarea**, y **pausa de revisión al terminar cada bloque**. Las guardas de `tests/architecture.rs` (dependencias por crate, 400 líneas) se actualizan **en la misma tarea** que mueve el crate, nunca después. Los renombrados se hacen con `git mv` para conservar la historia. Antes y después de cada bloque que toca la interfaz: `ui_shots.ps1` + `ui_compare.py`.

## Bloque 0 — El monorepo (sin cambiar ningún nombre de crate)

Deja la app igual, pero con la estructura de carpetas final.

| Tarea | Qué | Listo cuando |
|---|---|---|
| 0-1 | Borrar el C# de la raíz (`Dockview.csproj`, `Dockview.sln`, `Core/`, `ViewModels/`, `Services/`, `Themes/`, `Assets/`, `Properties/`, `Dockview.Tests/`, `*.xaml*`, `app.manifest`) y limpiar `.gitignore` | `git ls-files` sin C#; `cargo test` igual |
| 0-2 | Subir `rust/` a la raíz con `git mv` (`Cargo.toml`, `Cargo.lock`, `crates/`, `installer/`, `tools/`, `design/`, `ui-previews/`); los planes a `docs/`; `README.md` y `ARCHITECTURE.md` en la raíz | El workspace compila desde la raíz |
| 0-3 | Arreglar las rutas: los tres flujos de `.github/workflows` (sin `working-directory: rust`), `installer/*.iss`, `tools/*.ps1`, `.gitignore`, `architecture.rs` y los documentos | La CI pasa en GitHub |
| 0-4 | Mover `crates/dockview` a `apps/luminifex` (solo la carpeta; el paquete se sigue llamando `dockview`) | Compila y pasa |

**Pausa 0.**

## Bloque 1 — Nombres y datos

| Tarea | Qué | Listo cuando |
|---|---|---|
| 1-1 | `fexarium-kit::brand`: `FAMILY = "Fexarium"` y `APP = "Luminifex"`; rutas de datos, nombres de archivo y título salen de ahí | Un solo sitio para cambiar el nombre |
| 1-2 | Datos: `%APPDATA%\Fexarium\Luminifex\settings.json` y el registro; migración de lo viejo en el primer arranque (copiar, no borrar) | Test de migración; los ajustes de hoy aparecen |
| 1-3 | Clips y grabaciones: carpeta por defecto `Vídeos\Luminifex` y nombres «Luminifex …»; lo ya guardado no se toca | Test de rutas |
| 1-4 | Renombrar los paquetes de Luminifex: `dockview-domain`→`luminifex-domain`, `-stream`, `-platform`, `-ui`, `-app`→`luminifex-core`, y el binario `dockview`→`luminifex` (ejecutable, instalador, CI, release) | `luminifex.exe` arranca; todo en verde |

**Pausa 1.**

## Bloque 2 — La capa compartida

Se saca de los crates de Luminifex lo que Capifex también va a necesitar, **sin cambiar comportamiento**.

| Tarea | Qué | Listo cuando |
|---|---|---|
| 2-1 | `fexarium-kit`: tema, diseño (tamaños), marca, registro, geometría y `Rect`, datos de dibujo (`Vertex`, `DrawList`), atajos (`HotkeySpec`), `Lang` y el mecanismo de traducción (cada app define sus propias claves) | `luminifex-domain` depende de `fexarium-kit`; tests |
| 2-2 | `fexarium-ui`: el motor (`Ui`, texto, entrada, movimiento, avisos, ajuste de texto); las pantallas se quedan en `luminifex-ui` | Capturas de la interfaz idénticas |
| 2-3 | `fexarium-win`: dibujo de la interfaz en GPU, presentador, atajos globales, bandeja, registro en archivo, hilos | `luminifex-platform` más pequeño; tests |
| 2-4 | Reescribir `architecture.rs` para el grafo nuevo (dependencias permitidas por crate, ninguna app depende de otra) y el CI de Linux para los crates portables | Las guardas fallan si alguien rompe la regla |

**Pausa 2.**

## Bloque 3 — Sacar el capturador

| Tarea | Qué | Listo cuando |
|---|---|---|
| 3-1 | `capifex-core`: `select`, `snap`, `desktop`, `capture_settings` (sin `keep_in_tray`), `overlay_input`, `image` y el servicio de captura, con sus tests | Compila y pasa solo |
| 3-2 | `capifex-win`: captura de pantalla fija, ventana de selección, archivo de imagen, portapapeles | Sus tests pasan |
| 3-3 | Quitar de Luminifex todo lo del capturador: botón, atajo, menú de la bandeja, `--snap`, `snap::ENABLED`, `ResidentService` de capturas, sección `capture` de los ajustes | `grep` sin restos; los tests de Luminifex en verde |
| 3-4 | `keep_in_tray` pasa a los ajustes propios de Luminifex (con migración) | El replay con la ventana cerrada sigue |

**Pausa 3.**

## Bloque 4 — Capifex funcional

| Tarea | Qué | Listo cuando |
|---|---|---|
| 4-1 | `apps/capifex`: el binario residente (bandeja, atajo global, ajustes propios en `Fexarium\Capifex`) | Arranca y se queda en la bandeja |
| 4-2 | La selección sobre la pantalla (rectángulo, ventana, completa, forma libre) con la superposición de `capifex` | Una captura de cada modo |
| 4-3 | Guardar, copiar al portapapeles, aviso y abrir la carpeta; migración de la sección `capture` vieja | Prueba con la app real |
| 4-4 | Ventana de ajustes mínima (atajo, carpeta, formato, retardo) y el instalador de Capifex | Instalador probado |

**Pausa 4.**

## Bloque 5 — Distribución, documentos y Motifex

| Tarea | Qué | Listo cuando |
|---|---|---|
| 5-1 | CI y publicación por app: etiquetas `luminifex-v…` y `capifex-v…`, un artefacto por app | Una etiqueta de prueba produce los dos |
| 5-2 | `apps/motifex/README.md`: qué será, qué formatos de Luminifex leerá (pistas de audio separadas, archivo lateral con marcas de escena y clips) | El contrato escrito |
| 5-3 | `README.md` raíz, `ARCHITECTURE.md` y la memoria del proyecto al día | Docs sin referencias a `dockview` ni a `rust/` |

**Pausa 5.**

## Riesgos

1. **El número de crates sube de 6 a 10.** Se acepta a cambio de que cada app solo compile lo suyo; la guarda de dependencias es lo que lo mantiene ordenado.
2. **Partir `domain` y `ui` toca muchos imports.** Por eso va en el bloque 2, en tareas pequeñas, con las capturas de la interfaz como prueba de que nada cambió.
3. **La CI de Linux** exige que los crates portables compilen allí: `fexarium-kit`, `fexarium-ui` y los crates de reglas deben seguir sin Windows.
4. **Capifex necesita su propio bucle de ventana** (bandeja, atajo, superposición). Se aprovecha lo que hoy vive en `shell/capture.rs` y `capture_overlay.rs`, pero el cableado nuevo se prueba con la app real, no solo con tests.
5. **El nombre Fexarium es tentativo.** Está en `fexarium-kit::brand` y en los nombres de crate/carpeta; cambiarlo después es mecánico, pero cuanto antes se decida, menos hay que tocar.
6. **El historial de `main` conserva el C#.** Si hiciera falta recuperarlo: `git checkout main -- <ruta>`.
