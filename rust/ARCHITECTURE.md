# Arquitectura de Dockview

Definida el 2026-11-12, después de desacoplar la app una vez (ver `ARCHITECTURE_REVIEW.md`). El objetivo es **no tener que volver a desacoplarla**: cada tipo de código tiene un sitio, y el compilador impide que una capa importe lo que no debe.

## Los crates

| Crate | Es | Contiene | No puede |
|---|---|---|---|
| `dockview-domain` | El **qué** | Reglas y datos puros: escenas, transiciones, geometría, ajustes, grabación como datos, diagnóstico, textos (`i18n`), paleta (`theme`), el **modelo de vista** (`Model`, `Action` y sus vistas), los **eventos** (`AppEvent`), los datos de dibujo (`Vertex`, `GlyphUpload`) y los **puertos** (traits) | Windows, hilos propios, disco, ventana |
| `dockview-platform` | El **cómo** en Windows | Adaptadores: captura, audio, codificador, GPU, compositor, grabador, renderizador de vídeo y de interfaz (Direct3D), atajos, ajustes en disco, carpeta de clips, contadores. Implementa los puertos | Conocer la app o la interfaz |
| `dockview-ui` | El **cómo se ve** | Construye un fotograma de interfaz (`Ui`, controles, panel de estadísticas, avisos) a partir de un `Model` y devuelve `Action`s y una lista de dibujo. Texto con cosmic-text | Windows, Direct3D, servicios, la ventana |
| `dockview-app` | El **cuándo** (biblioteca) | `Core`: los servicios (`Replay`, `Sources`, `Scenes`, `Telemetry`, `AutoSwitch`…), cómo se compone el `Model` y qué hace cada `Action`. Trabaja **solo contra puertos** | winit, Windows, la interfaz, `platform` |
| `dockview` (binario) | La **raíz de composición** y la ventana | `main`, la ventana y el bucle de eventos (winit), el cursor y la pantalla completa. Crea los adaptadores reales, se los da a `Core`, y conecta `Core` ↔ `ui` ↔ renderizador | Lógica |

```
                       dockview (binario)
                 /          |          |         \
         dockview-ui   dockview-app   |   dockview-platform
                 \          |          |         /
                          dockview-domain
```

Las flechas solo van hacia abajo. `ui`, `app` y `platform` no se ven entre sí: se hablan por `Model`, `Action`, `AppEvent`, los puertos y los datos de dibujo, que viven en `domain`, y solo el binario los conecta. Como son crates distintos, romper la regla **no compila**.

## Cómo se reparte el trabajo en tiempo de ejecución

- **`Core` es dueño de todo el estado de aplicación.** Cada servicio es dueño de su estado y de nada más; no hay estado compartido mutable entre servicios. `Core` los coordina.
- **El modelo de vista se compone en `Core`**, con una función por servicio (`replay_view`, `sources_view`, `scenes_view`…) que recibe datos neutros del servicio y devuelve la parte del `Model` que le toca. Cada una se prueba sola. `Core::model()` solo las junta.
- **Órdenes entran, eventos salen.** La interfaz devuelve `Action`; `Core::apply(action)` la ejecuta contra los servicios. Los servicios devuelven `AppEvent` (por ejemplo «clip guardado», «fuente perdida»), sin ningún tipo de interfaz; el binario decide cómo mostrarlo (hoy, un aviso). Es una lista que se vacía en cada vuelta del bucle; cuando lleguen los plugins se amplía a un bus.
- **Lo que `Core` pide a la ventana lo pide por un puerto**, `Host`: pantalla completa, excluir la ventana de las capturas, elegir una carpeta. El binario lo implementa con winit y Windows.
- **El binario no decide nada**: cada vuelta del bucle es «leer entrada → `Core::tick` → `Core::model` → `ui.frame(model)` → dibujar → `Core::apply(acciones)`».

## Reglas

1. **Todo lo que toca hardware, disco o la ventana está detrás de un puerto** (trait en `domain`). Los servicios se prueban con dobles falsos, sin capturadora, GPU ni ventana. El adaptador real vive en `platform` (o, para la ventana, en el binario).
2. **Receta de una función nueva**, siempre en este orden:
   1. Modelo y reglas puras en `domain`, con tests.
   2. Un puerto, si necesita hardware, disco o ventana.
   3. El servicio en `app`, probado con un doble falso del puerto, y su parte del `Model`.
   4. El adaptador en `platform`.
   5. La vista en `ui`: lee del `Model`, emite `Action`s.
   6. El cableado en el binario (y solo eso).

   Si una función no cabe en esta receta, se discute antes de escribirla.
3. **Guardas automáticas** (tests que corren con `cargo test`): ninguna fuente de más de **400 líneas** (los de tests, de 1000) y las dependencias de cada `Cargo.toml` solo las permitidas por la tabla de arriba. Una excepción se escribe en el propio test, con su motivo.
4. **Convenciones**
   - Un error que el usuario debe conocer es un `AppEvent`; uno que no, se registra con `eprintln!`. Los fallos de hardware se devuelven como `Result<_, String>` a través de los puertos.
   - Un puerto se llama por lo que **necesita** el servicio (`ReplayBuffer`), no por cómo se implementa (`Mf…`).
   - Un servicio expone **datos** (`buffer_info()`), no vistas; la vista la compone `Core`.
   - Una función pura que decide algo va en `domain` o en el servicio, con su test; nunca en `Core::apply` ni en el binario.

## Dónde va cada cosa

| Quiero… | Va en |
|---|---|
| Una regla de decisión (qué hacer cuando…) | `domain`, como función pura con tests |
| Un tipo que cruza capas (`Action`, `AppEvent`, `Model`, un puerto) | `domain` |
| Leer un dispositivo, un archivo o la GPU | `platform`, tras un puerto de `domain` |
| Coordinar varias cosas con estado (grabar, escenas, medidas) | un servicio en `app` |
| Convertir datos del servicio en lo que muestra la pantalla | una función `*_view` en `app` |
| Dibujar o reaccionar al ratón | `ui` |
| Algo de la ventana (cursor, pantalla completa, foco) | el binario, tras el puerto `Host` |
| Crear un objeto real y pasárselo a otro | el binario |
| Un hilo o un recurso de Windows | `platform` |

## Estado de la migración

- [x] Desacoplar `App` en módulos y servicios con tests (D-1…D-8).
- [x] Paso 1: `dockview-domain` con los módulos puros.
- [x] Paso 2: `dockview-core` → `dockview-platform`, importar el dominio directamente, `settings` y `replay` al dominio, `SettingsStore` como primer puerto.
- [x] Paso 3a–3c: `clips`, `hotkey` y `perf` a `platform`; tipos de grabación al dominio; puertos y eventos del replay; `ReplayService` probado con dobles falsos.
- [ ] Paso 4: `Model`, `Action`, `i18n` y `theme` al dominio; datos de dibujo (`Vertex`, `GlyphUpload`) al dominio.
- [ ] Paso 5: crate `dockview-ui` (la interfaz sin Direct3D).
- [ ] Paso 6: los renderizadores de Direct3D (vídeo e interfaz) a `platform`.
- [ ] Paso 7: puertos de las fuentes de vídeo y de audio, del compositor y de las escenas, y el puerto `Host`.
- [ ] Paso 8: crate `dockview-app` como biblioteca (`Core`) y el binario `dockview` solo con la ventana y el cableado.
- [ ] Paso 9: las guardas automáticas (límite de líneas y dependencias), partir lo que las incumpla.

Cada paso deja la app funcionando, con todos los tests, una prueba con la app real y la comparación de la interfaz píxel a píxel, y se commitea y sube por separado.
