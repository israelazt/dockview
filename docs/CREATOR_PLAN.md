# Luminifex: modo Creador (2026-11-25)

## Por qué

El autor quiere hacer clips (también en formato vertical) y montarlos en un editor. Para eso necesita **cada fuente en su propio archivo**: el juego limpio, la cámara, el audio del juego y el micrófono. Hoy el audio ya va en pistas separadas dentro del MP4 (mezcla, juego, micrófono), pero el vídeo es uno solo: la imagen compuesta, con la cámara pegada encima si es una capa.

Decisión del autor (2026-11-25): esto no va en el visor directo. Se crea un **modo nuevo para crear contenido** y el visor pierde toda la responsabilidad de grabar: queda solo para jugar.

## Quién es dueño de qué

| Modo | Para qué | Qué tiene |
|---|---|---|
| **Visor directo** | Jugar | Imagen, pantalla completa, volumen, lecturas, **foto del juego** (D1) |
| **Creador** (nombre provisional, D5) | Grabar material para editar | Grabar por separado, Guardar clip por separado, cámara de control, medidores, estado de la toma |
| **Estudio** | Producir en vivo | Escenas, capas, transiciones, emisión, y su grabación de la imagen compuesta |
| **Ajustes** | Configurar la app | Lo de siempre, más las opciones del Creador (carpeta, calidad, cámara) si no caben en el modo |

La diferencia entre Estudio y Creador: **el Estudio arma la imagen final; el Creador guarda las piezas para armarla después.**

## Cómo será el Creador

- **El juego en grande**, como en el visor, con pantalla completa: se sigue jugando desde aquí.
- **Una vista pequeña de la cámara** para comprobar el encuadre. Se puede ocultar y **nunca** se graba sobre el juego.
- **Medidores** del audio del juego y del micrófono, para saber que suenan antes de grabar.
- **Barra:** Grabar, Guardar clip (con su duración), foto.
- **Estado de la toma:** tiempo, qué fuentes se están grabando, espacio libre y un aviso en cuanto una falla.

Cada toma es una carpeta:

```
2026-11-25 14-30-12 Toma/
  juego.mp4         vídeo limpio de la fuente principal, a su resolución
  camara.mp4        la cámara entera, sin recortar
  audio-juego.wav
  audio-mic.wav
  toma.json         qué hay, cuándo empezó cada archivo y el ajuste de sincronía
```

Todos los archivos empiezan en el mismo instante (mismo reloj), así que en el editor se alinean al principio de la línea de tiempo. El vertical (9:16) se encuadra en el editor; el mismo material sirve para horizontal.

## Regla del sonido: solo el juego

Decidido con el autor (2026-11-25): a veces escucha un podcast o ve un vídeo en el PC mientras juega. **Eso nunca entra en una toma.** Solo se graba lo que viene del juego en directo y el micrófono.

Cómo está hoy el código, según la fuente principal:

| Fuente | De dónde sale el sonido | ¿Entra el podcast? |
|---|---|---|
| Capturadora | La entrada de audio de la capturadora (HDMI) | **No** |
| Ventana | Solo el sonido de ese programa (loopback del proceso) | **No** |
| Monitor | Todo el sonido del PC (loopback del sistema) | **Sí** |
| Micrófono | El micrófono | Solo si suena por altavoces y el micrófono lo oye |

Qué hace el Creador:
- Con una **capturadora o una ventana** graba como hoy.
- Con un **monitor** no usa el sonido de todo el PC: pide elegir el programa del juego y graba solo ese (loopback del proceso). Si no se elige, la toma va sin sonido del juego y lo dice, en vez de grabar el podcast.
- El **micrófono** no puede distinguir: si el podcast suena por altavoces, lo oye. El Creador avisa (una vez) de usar auriculares cuando hay micrófono y la salida no lo es, si Windows lo sabe decir.

## Decisiones abiertas

| | Pregunta | Propuesta |
|---|---|---|
| D1 | ¿La foto del juego se queda en el visor? | Sí: es para compartir mientras se juega |
| D2 | ¿Guardar clip sale del visor? Consecuencia: un shiny en el visor no se puede rescatar en vídeo | Sí, y el replay no corre en el visor (no graba de fondo) |
| D3 ✅ | ¿Qué cámara? | **Canon M50 Mark II por HDMI limpio a la segunda capturadora** (`pacoxi-budget`, hasta 1080p50). Se abre como cualquier capturadora; el retraso se mide con el ajuste de C-4 |
| D4 | ¿Qué editor? (CapCut, DaVinci Resolve, Premiere…) | Decide si los WAV hacen falta o basta con las pistas del MP4, y el formato de vídeo |
| D5 | ¿Se llama «Creador»? | Sí, provisional |
| D6 | ¿La grabación del Estudio se queda? | Sí: graba la imagen compuesta, como hoy |
| D7 | ¿El Creador muestra las escenas del Estudio? | No: muestra la fuente principal limpia; las escenas son del Estudio |

## Cómo se trabaja

Igual que en los otros planes: tarea atómica con tests, `fmt`, `clippy -D warnings` y `cargo test --workspace` en verde, **un commit + push por tarea**, **pausa de revisión al terminar cada bloque**. Capturas de la interfaz antes y después de los bloques que cambian lo que se ve. La app real no se abre mientras el autor juega; el autor revisa la interfaz a mano.

## Bloque A — El modo existe y el visor se aligera

| Tarea | Qué | Listo cuando |
|---|---|---|
| A-1 | `domain`: `AppMode::Creator`, su pestaña y sus textos (es/en); la guarda de modos de `tests/architecture.rs` gana el Creador | Tests de navegación con cuatro modos |
| A-2 | `ui`: el módulo `creator/` con la imagen y su barra (Grabar, Guardar clip con duración, foto), sin lo del visor | Tests de la barra; la guarda no deja que `creator` use `controls` ni `studio` |
| A-3 | El visor pierde Grabar, Guardar clip, la duración y el micrófono (según D1 y D2); la tira de atajos y las teclas globales se ajustan | Tests del visor; captura antes y después |
| A-4 | `Core`: el replay corre solo en el Creador (y en el Estudio, si D2 lo pide); entrar y salir del modo lo enciende y apaga sin perder lo que se graba | Tests del arnés: cambiar de modo grabando no corta la grabación |

Pausa: el autor revisa el modo nuevo y el visor.

## Bloque B — La toma por separado: juego y audios

| Tarea | Qué | Listo cuando |
|---|---|---|
| B-1 | `domain`: `Take` (carpeta, nombres, `toma.json`) y sus reglas: nombre de la carpeta, archivos según lo que hay, partes si cambia la resolución | Tests puros |
| B-2 | Un tap del **vídeo limpio**: los frames de la fuente principal antes de componerla. Hoy `FrameSlot::publish` reenvía (`forward`) antes de dar a los taps, así que el slot de entrada no los ve: hay que darle un tap propio que funcione con el reenvío | Test: con una escena activa, el tap ve el frame de la fuente y no el compuesto |
| B-3 | `platform`: grabar `juego.mp4` desde ese tap (reutiliza el codificador y el MP4 fragmentado de `recording.rs`) | Test con el patrón de prueba: el archivo dura lo grabado y es legible |
| B-4 | `platform`: `audio-juego.wav` y `audio-mic.wav` del mismo reloj que el vídeo; el WAV se cierra bien aunque la app se cierre (cabecera reescrita al final y cada pocos segundos) | Test: duraciones iguales a ±1 frame |
| B-4b | La regla del sonido: con un monitor, el sonido del juego sale de un programa elegido y nunca del loopback del sistema; sin programa elegido, sin sonido del juego y con aviso | Tests del arnés: con un monitor, el Creador no abre `Loopback::System` |
| B-5 | `Core` + Creador: Grabar empieza y para la toma; el estado de la toma (tiempo, archivos, espacio) y los avisos | Tests del arnés con la máquina falsa |

Pausa: el autor graba una toma y la abre en su editor.

## Bloque C — La cámara

| Tarea | Qué | Listo cuando |
|---|---|---|
| C-1 | La cámara como **fuente propia del Creador** (no una capa de escena): elegirla y abrirla, según D3 | Tests de elección y de reconexión |
| C-2 | La vista pequeña de control, que se oculta, nunca entra en `juego.mp4` | Tests de la vista; la guarda lo comprueba |
| C-3 | `camara.mp4`: un segundo codificador. **Medir** en el PC del autor que dos codificadores no afectan al juego (fps mostrados, pérdidas, GPU) antes de seguir | Medición escrita en este plan |
| C-4 | Ajuste de sincronía de la cámara (ms, como el del audio), guardado en `toma.json` | Tests puros del ajuste |

Pausa: el autor revisa la cámara y la sincronía.

## Bloque D — Guardar clip por separado

| Tarea | Qué | Listo cuando |
|---|---|---|
| D-1 | Un búfer de repetición por fuente (juego, cámara) y el audio de cada una | Tests: la memoria sigue el límite de RAM que ya existe |
| D-2 | Guardar clip escribe una carpeta como la de la toma con los últimos segundos de todo | Test: los archivos del clip empiezan en el mismo instante |

Pausa: el autor prueba un clip de un momento inesperado.

## Bloque E — Calidad

| Tarea | Qué | Listo cuando |
|---|---|---|
| E-1 | Avisos claros: una fuente que se cae a media toma, poco disco, la cámara sin señal | Tests de cada aviso |
| E-2 | Documentar el formato de la toma (`toma.json`) para Motifex | Sección en `FEXARIUM_PLAN.md` |
| E-3 | Capturas finales y revisión del autor | Visto bueno |

## Costes que hay que vigilar

- **Rendimiento:** la cámara es un segundo vídeo que comprimir (C-3 lo mide antes de seguir). El bloque D además guarda en memoria los segundos de dos vídeos.
- **Disco:** a 40 Mbps el juego son unos 18 GB por hora; una cámara a 12 Mbps unos 5 GB más; cada WAV unos 0,7 GB por hora.
