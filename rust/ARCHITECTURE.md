# Arquitectura de Dockview

Definida el 2026-11-12, después de desacoplar la app una vez (ver `ARCHITECTURE_REVIEW.md`). El objetivo es **no tener que volver a desacoplarla**: cada tipo de código tiene un sitio, y el compilador impide que una capa importe lo que no debe.

## Las capas

| Crate | Qué es | Qué contiene | No puede |
|---|---|---|---|
| `dockview-domain` | El **qué**: reglas y datos | Escenas, transiciones, geometría, el modelo de ajustes, diagnóstico, selección de modo, aritmética de audio y de anillos, nombres de dispositivos, `FrameSlot`. Define los **puertos** (traits) y los **eventos** (`AppEvent`) | Usar Windows, hilos, disco ni ventana |
| `dockview-platform` | El **cómo** en Windows: adaptadores | Captura, audio, codificador, GPU, compositor, grabador, guardado de ajustes en disco, `Worker`. Implementa los puertos | Conocer la app o la interfaz |
| `dockview-app` | El **cuándo**: casos de uso | Los servicios (`Replay`, `Sources`, `Scenes`, `Telemetry`, `AutoSwitch`…). Trabajan **solo contra puertos** y emiten eventos | Importar `ui` o `platform` |
| `dockview-ui` | El **cómo se ve** | Dibujo, widgets, textos. Lee un `Model`, devuelve `Action`s | Importar los servicios |
| `dockview` (binario) | La **raíz de composición** | Crea los adaptadores, se los da a los servicios, conecta la interfaz y arranca el bucle de eventos | Tener lógica |

```
              dockview (binario)
             /     |       \       \
       dockview-ui  dockview-app  dockview-platform
              \         |          /
               dockview-domain
```

Las flechas solo van hacia abajo. `app` y `ui` no se ven entre sí: se hablan por `Model` / `Action` / `AppEvent`, que viven en `domain`, y el binario las conecta. Como son crates distintos, romper la regla no compila.

## Cuatro reglas

1. **Todo lo que toca hardware o disco está detrás de un puerto** (un trait en `domain`): `FrameSource`, `Recorder`, `AudioInput`, `SettingsStore`, `Clock`. Los servicios se prueban con dobles falsos, sin capturadora ni GPU. El adaptador real vive en `platform`.
2. **Órdenes entran, eventos salen.** La interfaz envía `Action`; los servicios devuelven `AppEvent` («clip guardado», «fuente perdida»…), sin ningún tipo de la interfaz. La app los recoge de una lista en cada vuelta del bucle y la interfaz decide cómo mostrarlos. Cuando lleguen los plugins, este mismo canal se amplía a un bus.
3. **Receta de una función nueva**, siempre en este orden:
   1. Modelo y reglas puras en `domain`, con tests.
   2. Un puerto, si necesita hardware o disco.
   3. El servicio en `app`, probado con un doble falso del puerto.
   4. El adaptador en `platform`.
   5. La vista en `ui`: lee del `Model`, emite `Action`s.
   6. El cableado en el binario (y solo eso).

   Si una función no cabe en esta receta, se discute antes de escribirla.
4. **Guardas automáticas**: un test falla si un archivo de código pasa de **400 líneas** (los de tests, de 1000), y el compilador falla si una capa importa a otra que no debe.

## Dónde va cada cosa

| Quiero… | Va en |
|---|---|
| Una regla de decisión (qué hacer cuando…) | `domain`, como función pura con tests |
| Un tipo que cruza capas (`Action`, `AppEvent`, `Model`) | `domain` |
| Leer un dispositivo, un archivo o la GPU | `platform`, tras un puerto de `domain` |
| Coordinar varias cosas con estado (grabar, escenas, medidas) | un servicio en `app` |
| Dibujar o reaccionar al ratón | `ui` |
| Crear un objeto real y pasárselo a otro | el binario |
| Un hilo o un recurso de Windows | `platform` |

## Estado de la migración

- [x] Desacoplar `App` en módulos y servicios con tests (D-1…D-8).
- [x] Paso 1: crear `dockview-domain` con los módulos puros que ya están aislados (`audio_sync`, `device`, `device_names`, `diagnostics`, `frame`, `geometry`, `pcm`, `scene`, `stream`, `transition`). `dockview-core` los reexporta hasta el paso 5. Quedan para el paso 2 `settings` (tiene E/S de disco) y `replay` (usa un tipo del codificador).
- [~] Paso 2: **hecho** el renombrado de `dockview-core` a `dockview-platform` y que todos importen `dockview_domain` directamente (sin reexports). **Falta** mover `settings` y `replay` a `domain` y definir los puertos del grabador, las fuentes, el audio y el guardado de ajustes.
- [ ] Paso 3: eventos de dominio (`AppEvent`) sin `ToastKind`, y servicios que usan puertos (`ReplayService` primero).
- [ ] Paso 4: partir `dockview-app` en `app`, `ui` y el binario con el cableado.
- [ ] Paso 5: las guardas automáticas (límite de líneas). Los reexports temporales ya se quitaron en el paso 2.

Cada paso deja la app funcionando, con todos los tests y una prueba con la app real, y se commitea y sube por separado.
