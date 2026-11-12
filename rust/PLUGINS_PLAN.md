# Plugins — plan (decidido 2026-11-12)

Objetivo: que Dockview pueda recibir **plugins de la comunidad**, como OBS. De momento **no se escribe ningún plugin ni el cargador**: se prepara la estructura interna para que añadirlos después no obligue a rehacer nada.

Decisión del autor: los plugins son para una **comunidad abierta** (cualquiera puede publicar uno). Eso exige interfaz pública estable y versionada, y seguridad pensada desde el principio.

## Qué podría hacer un plugin

| Punto de extensión | Ejemplo | Dónde engancha |
|---|---|---|
| **Fuente** | Navegador, visor de chat, otra cámara | Publica frames en un `FrameSlot`, como una capturadora |
| **Transición / curva** | Rebote, deslizamiento, elástico | Registro de animaciones (las nuestras son las primeras entradas) |
| **Efecto de capa** | Recorte, color, marco | Después de que exista un compositor con shaders |
| **Salida** | RTMP, otro formato de archivo | Lee el slot de salida del compositor |
| **Acción / atajo** | «Cambiar a la escena 2» | Registro de acciones |
| **Automatización** | Reaccionar a «empezó la grabación» | Bus de eventos |
| **Panel de interfaz** | Lista de alertas | Lo más difícil; el último |

## Dos niveles de plugin

1. **Nativos (DLL con interfaz C)**: fuentes, efectos, transiciones y salidas. Máxima velocidad y acceso a los frames. **No se pueden aislar**: corren dentro de Dockview con sus permisos, igual que en OBS. Rust no tiene ABI estable, así que la interfaz es C (`repr(C)`), con versión; nada de tipos de Rust cruzando la frontera.
2. **De control (proceso aparte)**: automatización, alertas, chat, mandos. Hablan con Dockview por una API local (websocket o tubería con nombre, solo en `localhost`, con permisos por plugin). Si fallan, Dockview no se entera. Valen en cualquier lenguaje.

No se descarta WebAssembly para efectos sencillos, pero no sirve para video en tiempo real (no ve la GPU); se decide con un caso real.

## Seguridad (comunidad abierta)

- Los plugins viven en `%APPDATA%\Dockview\plugins\<id>\` con un `manifest.json`: id, nombre, versión, **rango de versiones de la API**, autor, tipo y, para los de control, los **permisos** que pide.
- **Desactivados hasta que el usuario los active**, y la primera vez se avisa sin rodeos: «este plugin ejecuta código con acceso completo a tu PC».
- **Modo seguro**: si Dockview se cae mientras carga o ejecuta un plugin, al reiniciar ese plugin queda desactivado y se avisa.
- La API rechaza plugins con versión incompatible; sin actualizaciones automáticas; lista visible de plugins en Ajustes.
- Ningún plugin puede bloquear el hilo de dibujo: los frames pasan por slots (siempre gana el más nuevo) y las llamadas lentas tienen tiempo límite.
- Más adelante, valorar hash o firma de plugins y una lista de confianza.

## Preparación interna (sin plugins todavía)

Cada paso se mide y se sube por separado. Las funciones propias de Dockview usan **los mismos registros** que usarán los plugins, para que la interfaz se pruebe a diario.

| Paso | Qué | Por qué |
|---|---|---|
| P-1 | Un trait `VideoSource` (arrancar sobre un slot, estado, parar) que unifique la fuente principal, las capas de encima y el patrón de prueba | Hoy `main.rs` las trata por separado; con el trait, una fuente de plugin sería una más, y `main.rs` adelgaza |
| P-2 | `SourceRef` abierto: variante `plugin` con id y configuración en JSON, y carga tolerante (un tipo desconocido se conserva y se muestra como «no disponible») | Una escena guardada con una fuente de plugin no debe romper el archivo ni perderse |
| P-3 | Bus de eventos interno (`escena activada`, `grabación iniciada`, …) | Lo usarán la interfaz, los atajos y la automatización |
| P-4 | Registros de curvas/transiciones y de acciones, con las nuestras como primeras entradas | Las animaciones de escena nacen como «plugins internos» |
| P-5 | Crate `dockview-plugin-api` aparte, pequeño y versionado, con solo los tipos de la interfaz C (sin cargador) | Es lo único que un plugin vería |
| P-6 | Cargador de plugins nativos: manifiesto, aviso de consentimiento, modo seguro | Cuando haya un primer plugin real |
| P-7 | API de control local con permisos | Después del cargador |

## Reglas

- La API pública es **pequeña y estable**: es más fácil añadir que quitar. Versionado semántico; un cambio incompatible sube la versión mayor.
- **Memoria**: quien reserva, libera; los buffers de video los pone Dockview.
- **Hilos**: cada plugin trabaja en su hilo; nunca en el de dibujo.
- **GPU**: de entrada las fuentes entregan frames NV12 en RAM (como las nuestras); compartir texturas es una mejora posterior.
- Se documenta la API y se ofrece una plantilla de plugin antes de aceptar contribuciones.
- Módulos pequeños, como el resto del proyecto.

## Orden (actualizado: los plugins van al final)

El autor decidió que los plugins se hacen **al final**, después de las escenas, las animaciones, el editor rápido y el streaming. Hasta entonces se sigue el plan de `CAPTURE_PLAN.md` sin parar a preparar nada. Lo único que se vigila mientras tanto, sin coste extra: no añadir tipos cerrados nuevos donde pueda ser abierto, y mantener los módulos pequeños. Los pasos P-1 a P-7 se retoman cuando llegue el momento; P-1 (trait común de fuentes) y P-2 (tipos de fuente abiertos) son los primeros, y conviene hacerlos antes de añadir muchas más fuentes.

## Riesgos

- Una interfaz demasiado ambiciosa que haya que romper pronto: por eso solo se publica lo que Dockview ya usa por dentro.
- Código ajeno dentro del proceso: sin aislamiento posible para los nativos; la defensa es el consentimiento, el modo seguro y la transparencia.
- Mantenimiento: con una comunidad abierta hay que sostener la compatibilidad entre versiones.
